# Veil 技术架构

版本：1.10  
状态：实现栈为 Rust（x64 MSVC）+ egui 小面板 + 原生托盘；`src/` 为 Cargo workspace。辅助输出随应用带文件，安装时或面板装设备，已有同一 MTT 则接管。日常预览可用。公开产品未发布。私有预览打包流程见 [RELEASE.md](RELEASE.md)，不是可公开安装。  
日期：2026-09-21

本文是公开产品的实现架构。产品合同见 [PRD.md](PRD.md)，形态与运行时合同见 [PRODUCT_DESIGN.md](PRODUCT_DESIGN.md)。没有实测证据的条目不得写成已完成或已兼容。

## 1. 文档分工

| 文档 | 回答的问题 |
| --- | --- |
| PRD | 用户要什么、什么算成功 |
| PRODUCT_DESIGN | 托盘、按需路径、双进程、安装同意、验收边界 |
| **本文** | 用什么技术做、进程怎么切、仓库怎么摆 |

产品要求已经锁在前两份里。本文不再重开临时关闭、黑窗、自研驱动、跨重启再关、MSIX、Electron。

## 2. 已锁定的实现选择

| 项 | 选择 | 说明 |
| --- | --- | --- |
| 语言与运行时 | Rust，x64 MSVC（`x86_64-pc-windows-msvc`） | CCD 结构体尺寸与查询标志由单测锁住；首版不发布 x86 / ARM |
| 界面 | egui 小面板 + 原生托盘（优先 wgpu，初始化失败尝试 glow） | 单击托盘打开；关面板退回托盘；不引入浏览器控件 |
| 安装 | 传统安装包：WiX 5 引导 EXE（Burn）+ 应用 MSI | 便于提权安装已签名驱动；不用 MSIX |
| 首版范围 | 含辅助虚拟输出 | 无第二物理屏时，允许关光全部物理屏；文件随应用，设备可安装时或面板装 |
| 关屏机制 | CCD `SetDisplayConfig` 停路径 | 不写 `SDC_SAVE_TO_DATABASE` |
| 进程 | 界面进程 + 独立恢复进程；关屏 APPLY 只发生在恢复进程 | 父进程退出后恢复进程仍回放拓扑 |
| 驱动协助 | 安装、启用、禁用辅助输出时拉起提权助手 | 日常关一块、且已有活动第二物理屏时不要求管理员 |

否决项：WinUI 3 首版、Tauri/Electron、用浏览器控件、重写 CCD 结构但不锁 ABI、借用机器上已有的向日葵 / GameViewer 等虚拟屏。

目标系统：**Windows 11 x64**。Windows 10 与其它 GPU 组合未测，不得写入支持列表。

## 3. 运行时切分

```text
安装器 (elevated, 一次性)
  └─ 写入 Veil.App / Veil.Recovery / 辅助 VDD 文件（始终带上）
  └─ 默认安装 INF，设备禁用；也可稍后由面板安装
  └─ 不自动开机自启、不自动关屏

Veil.App.exe (用户会话, 不提权)
  ├─ 单实例互斥 Local\Veil；第二实例发 Local\Veil.ShowPanel
  ├─ 托盘与 egui 面板
  ├─ 调用 Veil.Engine：枚举、门禁、VALIDATE
  ├─ 需要安装或启用辅助输出时启动 Veil.DriverHelper（UAC）
  └─ 启动 Veil.Recovery，只发「关这些 / 恢复」意图
        │
        │ 会话目录 %LOCALAPPDATA%\Veil\session-<id>\
        │ ready.json / arm.json / release.json / result.json
        ▼
Veil.Recovery.exe (同一用户会话, 脱离 Job, 无窗口)
  ├─ 注册 Ctrl+Alt+Shift+F10
  ├─ 登记挂起/恢复与会话显示电源通知（失败不阻止关屏）
  ├─ ready 之后才允许 arm
  ├─ VALIDATE 通过才 APPLY 停路径
  ├─ 监视热键、release.json、父进程、拓扑、执行间隙、电源事件
  └─ 回放保存拓扑；失败则 SDC_TOPOLOGY_INTERNAL 兜底
```

**硬规则：**

1. 界面进程可以 VALIDATE，**不得**自己 APPLY 保持关闭。APPLY 与恢复必须在同一个恢复进程里，避免父进程在恢复之后再次关屏。
2. 关屏前恢复进程必须 `ready.json.hotkeyRegistered=true`，且 `arm.json.pid` 与该进程一致。
3. 停路径后必须仍有至少一条活动输出路径。`remainingActive=0` 或 VALIDATE 非 0 则不 APPLY。
4. 不把 `SC_MONITORPOWER` 当作保持关闭。产品不实现系统熄屏入口。

`Veil.Engine` 是类库，App 与 Recovery 都引用它。App 用它做列表和门禁；Recovery 用它做 VALIDATE / APPLY / 回放。

## 4. 能力引擎

每次操作重新枚举活动路径，不缓存过期拓扑。

### 4.1 路径角色

- `internal`：内嵌输出技术（Internal / eDP / UDI embedded）
- `virtual`：适配器或监视器路径命中 IddCx / MTT / ROOT\DISPLAY 等特征
- `external`：其余已连接目标
- `placeholder`：`DEFAULT_MONITOR`，不当作可关物理屏

界面只列出 `internal` 与 `external`。向日葵 / GameViewer 等其它虚拟屏既不出现在列表，也不作为第二目标。辅助 MTT 同样不出现在列表；它只在「将关光全部物理屏」时作为隐藏退路。

物理屏身份优先用适配器 LUID + 目标 ID + 监视器设备路径；热插拔后对不上则视为新设备，默认开启。

### 4.2 关屏决策

按 [PRODUCT_DESIGN.md](PRODUCT_DESIGN.md) 第 2 节：

| 用户意图 | 条件 | 动作 |
| --- | --- | --- |
| 关一块或多块，关完仍有活动物理屏 | VALIDATE 0 且剩余活动路径 ≥ 1 | 清除目标路径 ACTIVE。若被关的是当前主屏（留下的源不在原点），把留下的源模式挪到 `(0,0)` |
| 关完后物理路径变为 0 | 用户已同意安装或启用辅助输出 | 没有设备则先安装或接管 → 提权启用 → 确认出现活动虚拟路径 → 再停全部物理路径 |
| 任一前一步失败 | — | 结束本轮关闭要求，执行一次恢复；恢复未确认则保留恢复入口 |

已锁定行为（不是全平台保证）：

- 双物理屏扩展桌面：只停一块且留下的源不在原点时，允许 `adjustedOrigin`。
- 仅内屏：原生停最后路径会 VALIDATE 失败。已签名 MTT 作第二目标后，走共用源/克隆退路；必须在面板说明「显示拓扑可能变化」，不得写成无感。

目标集合是用户选中的物理路径列表。克隆调整只留给「物理路径将变为 0、必须靠辅助输出」这一支。已有第二物理屏时禁止无故改克隆。

### 4.3 状态

每块物理屏：

- 用户要求：`开启` / `保持关闭`
- 已确认：`已显示` / `已关闭` / `处理中` / `失败` / `未知`

失败不得显示为已关闭。睡眠、拔插、辅助输出消失后重新枚举。重启后要求清空。

睡眠或待机中断后回放关屏前拓扑并打开面板，是产品要求。自动再关不是本轮要求。实现必须：电源/待机通知优先；调度间隙与漏掉的待机同样只回放不 reapply；VALIDATE/APPLY 失败则结束要求并说明；禁止循环 APPLY。热插拔仍允许单次再关。Rust 睡醒未机旁验证。

## 5. 恢复进程协议

显示电源的熄灭、变暗、点亮只作为观察事件记录，不依据熄屏时长推断系统睡眠。明确挂起／恢复使用系统电源通知；执行间隙仍以未知中断处理。同一次间隙中断仅回放一次基线，必要时执行一次 INTERNAL 兜底。

会话目录用文件握手，不引入 RPC。

| 文件 | 写入方 | 含义 |
| --- | --- | --- |
| 拓扑 JSON（path/mode 原始字节） | App，在 arm 前 | 启用 VDD 后的握手快照；恢复使用启用前 baseline.json；不含设备实例隐私以外的额外字段 |
| `ready.json` | Recovery | `pid`、`hotkeyRegistered` |
| `arm.json` | App | 必须等于 Recovery 的 pid，之后才允许 APPLY |
| `release.json` | App | 用户恢复全部 / 退出 |
| `result.json` | Recovery；进程已死时也可由 App 补写 | 结束原因与恢复结果 |
| `events.jsonl` | Recovery（App 在补写 result 时也可追加） | 追加时间线：ready / apply / settle / interrupt / reapply / finish。给操作者复盘，不是心跳替代 |
| `vdd-request.json` | Recovery 再关需要辅助输出时 | 界面 Poll 后提权 enable 一次；出现后再 APPLY。未完成不得循环 APPLY |

结束原因需能区分：`hotkey`、`release`、`parent-exit`、`suspend-resume`（睡眠/待机）、`execution-gap`（调度间隙兜底）、`unexpected-topology`、`error`、`recovery-exit`（恢复进程已死、未写结果）。未 arm 时也要响应 `release.json`，不得一直停在「等待 arm」。卸载检查协议 v2 的编号化 `result.json` 与维护门禁。

创建恢复进程时使用 `CREATE_BREAKAWAY_FROM_JOB | CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW`。父进程崩溃不得带走恢复进程。恢复进程自身崩溃仍不保证回放拓扑；界面保留失败上下文，用户再次恢复时启动 `--restore-only` 进程。只有重新枚举确认活动物理屏，且本次操作承担启用责任时才尝试 disable；状态未知则保留辅助输出。人工兜底仍是 `Win+Ctrl+Shift+B`，再不行重启。

热键：`MOD_NOREPEAT | MOD_SHIFT | MOD_CONTROL | MOD_ALT` + `VK_F10`。注册失败则拒绝关屏，面板必须可见地写「不可用」。

## 6. 界面进程

协调器区分恢复结果已消费、辅助输出清理、面板通知三个状态。保留会话目录是为了重试，不表示旧结果需要再次消费。同一操作只通知一次，失败 heartbeat 无最终结果时也通知；清理失败或取消不会被下次轮询覆盖。显式恢复全部重新检查仍连接的基线物理屏：已恢复时仅重试清理，否则启动恢复专用进程。清理未完成继续阻止新关屏。

App 每 400 毫秒轮询，不随每次绘制读取结果。窗口恢复位置和尺寸统一使用原生物理坐标；placement 单独换算工作区偏移，工作区取目标显示器。重复显示请求在窗口已可见且几何正确时不再移动或抢前台，普通每帧同步不重复改样式或任务栏注册。

`events.jsonl` 增加拓扑写入、辅助输出清理、面板通知事件，detail 带操作编号。App 日志保留 helper 启动错误、取消、超时、退出码；超时进程未退出时不得启动第二个设备操作。磁盘协议仍为 v2。离线和机旁边界见 [唤醒闪屏修复验收](validation/resume-flicker.md)。

egui 窗口只承担展示与点击。关屏期间允许隐藏到托盘，后台要求仍由恢复进程维持。当前实现优先使用 wgpu，初始化失败尝试 glow。

- 单击托盘：打开/前置面板
- 右键：打开面板、恢复全部、退出
- 关闭或最小化面板：退回托盘，不退出进程；任务栏和 Alt-Tab 都没有
- 第二实例：不新开窗口，向已有进程发 `Local\Veil.ShowPanel`
- 面板：已连接物理屏列表、每块保持关闭/恢复、底部恢复全部、开机自启（默认关，写当前用户 Startup）、热键状态
- DPI：`PerMonitorV2`
- 退出：先 `release` 并等待恢复结果；超时或失败则报告，不默默退出

托盘图标用 Win32 `Shell_NotifyIcon`（`tray-icon`）。不引入浏览器控件。Windows 上关面板不调用 eframe 0.31 的 `Visible(false)`，也不 `SW_HIDE`：二者都会让 winit 停泵消息，托盘再显示会卡住。关面板时把窗口停到屏幕外并保持可见，每帧维持 `WS_EX_TOOLWINDOW` 与 `ITaskbarList::DeleteTab`，事件循环继续跑。恢复时不得把停泊坐标（约 `-32000,-32000`）当正常位置；记忆矩形只接受屏幕上足够大的窗口，打开时用 `SetWindowPlacement` 拉回工作区。这是实现对 Windows + eframe 0.31 的约束。

## 7. 安装、驱动与提权

### 7.1 安装包内容

引导 EXE 提权后：

1. 安装 `Veil.App.exe`、`Veil.Recovery.exe`、资源到 `%ProgramFiles%\Veil`
2. 展示驱动同意页：这是显示驱动，用于关光全部物理屏时留下活动路径。不同意装设备时，文件仍写入，以后可在面板安装
3. 将已签名 MTT VDD **始终**放到 `%ProgramFiles%\Veil\vdd`。默认用 INF 安装，**设备保持禁用**；本机已有同一硬件 ID 则接管，不新建第二块
4. 校验 CAT/DLL 签名与发布者指纹；不启用测试签名；不经确认不装设备
5. 写卸载信息。开机自启不在安装时打开

产品必须：

- 使用自己的安装目录
- 只认 MTT 硬件 ID（`Root\MttVDD`）与已记录发布者指纹；已有同一 ID 则接管
- 允许机器上存在 GameViewer / 向日葵等其它虚拟屏，但永不把它们当退路、不改装它们

具体文件哈希以安装时捆绑的版本为准，写入 [`installer/payload.manifest.json`](../installer/payload.manifest.json)。更换上游包必须重新核签名与指纹。捆绑 `MttVDD.dll` 的 UTF-16 字符串写死 `C:\VirtualDisplayDriver`；DriverHelper 安装时把 `vdd_settings.xml` 同时写到 `%ProgramFiles%\Veil\vdd` 与该目录。

### 7.2 按需启用

辅助输出是否已有设备，以 PnP 实例（`Root\MttVDD`）为准，不只看 INF 文件。查找已校验驱动包时，Program Files、exe 旁和仓库 `installer/payload` 都应算数，不能只认 Program Files。关光最后一块物理屏时：已有设备则按需启用；没有设备则面板说明将**安装并启用**（或接管已有 MTT）；用户确认后提权 `install-driver` 再 `enable`。取消或失败则物理屏不改动。只有找不到驱动包、也没有设备时，才拒绝最后一块。

`Veil.DriverHelper.exe` 清单要求管理员。用户从面板安装辅助输出，或要关光全部物理屏且当前没有活动辅助路径时启动。

1. 面板说明将打开辅助输出，拓扑可能闪断
2. 启用设备，等到活动虚拟路径出现（超时则禁用并失败，物理屏不动）
3. 再交给恢复进程停物理路径

退出或恢复全部物理屏后，尽力禁用辅助设备，避免留下一块用户没要的虚拟屏。禁用失败要可见，不能假装卸掉了。卸载顺序：恢复物理屏 → 禁用并删除所有权清单里的设备 → 删文件。

启用/禁用是否处处不需重启、睡眠后设备是否仍在，按本机结果为准，不得写成全平台可用。

## 8. 仓库布局

```text
doc/                      合同、架构、发布说明
src/Cargo.toml            Rust workspace
src/veil-engine/          CCD、角色、门禁、拓扑、VALIDATE、RecoverySession
src/veil-recovery/        独立恢复进程（发布名为 Veil.Recovery.exe）
src/veil-app/             egui 托盘面板（发布名为 Veil.App.exe）
src/veil-driver-helper/   安装/启用/禁用辅助 VDD（发布名为 Veil.DriverHelper.exe）
installer/Veil.Setup/     应用 MSI
installer/Veil.Bundle/    WiX Burn 引导 EXE
tools/show-session.ps1    打印最近一次产品会话记录
```

`src/` 为 Cargo workspace。安装器构建要求 `installer/payload/` 中的已核验文件；缺失则失败。

行为契约：查询标志、停 ACTIVE、原点调整、VALIDATE 后 APPLY、拓扑原始字节回放、ready/arm 握手、热键、父进程退出恢复、睡眠/待机中断后回放并打开面板、`execution-gap` 与漏掉的待机同样结束保持关闭且不自动再关、热插拔允许单次再关、内屏兜底。不要做：仅内屏按钮、`SC_MONITORPOWER` 产品入口、扩展桌面失败就改克隆（那只属于 VDD 退路）。

## 9. 测试与发布

| 层 | 做什么 | 不做什么 |
| --- | --- | --- |
| `cargo test --manifest-path src/Cargo.toml` | ABI、停路径、原点、角色、门禁、会话握手、Coordinator | 不调用真实 `SetDisplayConfig` APPLY |
| 日常使用 | 面板点选、托盘开关、关屏与恢复 | API 成功单独不算通过 |
| 发布 | 能力检测失败则禁用并说明原因 | 不把未测 GPU / 系统写入支持列表 |

安装包签名、驱动同意文案、卸载恢复，均属发布门禁；未做不得标「可公开安装」。私有仓库可用无签名预览包供协作者下载自用，流程见 [RELEASE.md](RELEASE.md)。
