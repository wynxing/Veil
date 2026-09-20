# Veil 技术架构

版本：1.7  
状态：实现栈为 Rust（x64 MSVC）+ egui 小面板 + 原生托盘；`src/` 为 Cargo workspace。首版代码已齐，预览包可下载。Rust REDMI 短时只停内屏已有系统检查 + 口头；P15 Rust 尚未执行。公开产品未发布。私有预览打包流程见 [RELEASE.md](RELEASE.md)，不是可公开安装。  
日期：2026-09-19

本文是公开产品的实现架构，不是实验室日记。产品合同见 [PRD.md](PRD.md)，形态与运行时合同见 [PRODUCT_DESIGN.md](PRODUCT_DESIGN.md)，实验规则见 [TECH_VALIDATION.md](TECH_VALIDATION.md)。没有实测证据的条目不得写成已完成或已兼容。

## 1. 文档分工

| 文档 | 回答的问题 |
| --- | --- |
| PRD | 用户要什么、什么算成功 |
| PRODUCT_DESIGN | 托盘、按需路径、双进程、安装同意、验收边界 |
| **本文** | 用什么技术做、进程怎么切、仓库怎么摆 |
| TECH_VALIDATION / `validation/` | 某台机器上实际跑过什么 |

产品要求已经锁在前两份里。本文不再重开临时关闭、黑窗、自研驱动、跨重启再关、MSIX、Electron。

## 2. 已锁定的实现选择

| 项 | 选择 | 说明 |
| --- | --- | --- |
| 语言与运行时 | Rust，x64 MSVC（`x86_64-pc-windows-msvc`） | 结构体尺寸与探针 ABI 单测对齐；首版不发布 x86 / ARM |
| 界面 | egui 小面板 + 原生托盘（优先 wgpu，初始化失败尝试 glow） | 单击托盘打开；不是设置中心；不引入浏览器控件 |
| 安装 | 传统安装包：WiX 5 引导 EXE（Burn）+ 应用 MSI | 便于提权安装已签名驱动；不用 MSIX |
| 首版范围 | 含按需自带 VDD | 无第二物理屏时，允许关光全部物理屏 |
| 关屏机制 | CCD `SetDisplayConfig` 停路径 | 不写 `SDC_SAVE_TO_DATABASE` |
| 进程 | 界面进程 + 独立恢复进程；关屏 APPLY 只发生在恢复进程 | 已在探针中验证父进程退出仍能恢复 |
| 驱动协助 | 仅在启用/禁用自带 VDD 时拉起提权助手 | 日常关一块物理屏不要求管理员 |
| 实验室 | 保留 Python 3.12 探针 | 不作为安装包，不作为发布 UI |

否决项：把实验室探针打成安装包、PyInstaller 当发布物、WinUI 3 首版、Tauri/Electron、用浏览器控件、重写 CCD 结构但不锁 ABI、借用机器上已有的向日葵 / GameViewer 等虚拟屏。

目标系统按已测机器写：**Windows 11 x64**。Windows 10 与其它 GPU 组合未测，不得写入支持列表。

## 3. 运行时切分

```text
安装器 (elevated, 一次性)
  └─ 写入 Veil.App / Veil.Recovery / 自带 VDD 文件
  └─ 安装 INF，设备默认禁用
  └─ 不自动开机自启、不自动关屏

Veil.App.exe (用户会话, 不提权)
  ├─ 单实例互斥 Local\Veil
  ├─ 托盘与 egui 面板
  ├─ 调用 Veil.Engine：枚举、门禁、VALIDATE
  ├─ 需要自带 VDD 时启动 Veil.DriverHelper（UAC）
  └─ 启动 Veil.Recovery，只发「关这些 / 恢复」意图
        │
        │ 会话目录 %LOCALAPPDATA%\Veil\session-<id>\
        │ ready.json / arm.json / release.json / result.json
        ▼
Veil.Recovery.exe (同一用户会话, 脱离 Job, 无窗口)
  ├─ 注册 Ctrl+Alt+Shift+F10
  ├─ ready 之后才允许 arm
  ├─ VALIDATE 通过才 APPLY 停路径
  ├─ 监视热键、release.json、父进程、拓扑、执行间隙
  └─ 回放保存拓扑；失败则 SDC_TOPOLOGY_INTERNAL 兜底
```

**硬规则（来自已验证探针，产品必须保持）：**

1. 界面进程可以 VALIDATE，**不得**自己 APPLY 保持关闭。APPLY 与恢复必须在同一个恢复进程里，避免父进程在恢复之后再次关屏。
2. 关屏前恢复进程必须 `ready.json.hotkeyRegistered=true`，且 `arm.json.pid` 与该进程一致。
3. 停路径后必须仍有至少一条活动输出路径。`remainingActive=0` 或 VALIDATE 非 0 则不 APPLY。
4. 不把 `SC_MONITORPOWER` 当作保持关闭。探针里的 `temp-off` 只用于电源诊断，产品不实现。

`Veil.Engine` 是类库，App 与 Recovery 都引用它。App 用它做列表和门禁；Recovery 用它做 VALIDATE / APPLY / 回放。

## 4. 能力引擎

每次操作重新枚举活动路径，不缓存过期拓扑。

### 4.1 路径角色

沿用探针 `classify_role`：

- `internal`：内嵌输出技术（Internal / eDP / UDI embedded）
- `virtual`：适配器或监视器路径命中 IddCx / MTT / ROOT\DISPLAY 等特征
- `external`：其余已连接目标
- `placeholder`：`DEFAULT_MONITOR`，不当作可关物理屏

界面只列出 `internal` 与 `external`。第三方虚拟屏既不出现在列表，也不作为第二目标。自带 VDD 同样不出现在列表；它只在「将关光全部物理屏」时作为隐藏退路。

物理屏身份优先用适配器 LUID + 目标 ID + 监视器设备路径；热插拔后对不上则视为新设备，默认开启。

### 4.2 关屏决策

按 [PRODUCT_DESIGN.md](PRODUCT_DESIGN.md) 第 2 节：

| 用户意图 | 条件 | 动作 |
| --- | --- | --- |
| 关一块或多块，关完仍有活动物理屏 | VALIDATE 0 且剩余活动路径 ≥ 1 | 清除目标路径 ACTIVE。若被关的是当前主屏（留下的源不在原点），把留下的源模式挪到 `(0,0)` |
| 关完后物理路径变为 0 | 用户已同意安装自带 VDD | 提权启用自带设备 → 确认出现活动虚拟路径 → 再停全部物理路径 |
| 任一前一步失败 | — | 结束本轮关闭要求，执行一次恢复；恢复未确认则保留恢复入口 |

已验证事实（不是全平台保证）：

- COLORFUL P15 扩展桌面：只停内屏 VALIDATE 0，不必改克隆；只停外屏须 `adjustedOrigin`。
- REDMI 仅内屏：原生停最后路径 VALIDATE 87。已签名 MTT VDD 作第二目标后，**已验证的是共用源/克隆**；扩展桌面下只停内屏仍会 87。产品若走 VDD 退路，允许先改到该拓扑，但必须在面板说明「显示拓扑可能变化」，不得写成无感。

产品引擎**不要**把探针里「只关内屏」写死。目标集合是用户选中的物理路径列表。克隆调整只留给「物理路径将变为 0、必须靠自带 VDD」这一支。双物理屏扩展桌面已通过的，禁止无故改克隆。

### 4.3 状态

每块物理屏：

- 用户要求：`开启` / `保持关闭`
- 已确认：`已显示` / `已关闭` / `处理中` / `失败` / `未知`

失败不得显示为已关闭。睡眠、拔插、自带 VDD 消失后重新枚举。重启后要求清空。

睡醒后再关是产品要求。Python 探针在 REDMI 上醒后内屏亮起且未自动再关。Rust 睡醒尚未机旁执行。实现必须：尝试一次；VALIDATE/APPLY 失败则结束要求并说明；禁止循环 APPLY。

## 5. 恢复进程协议

会话目录沿用探针已跑通的文件握手，由 Rust 实现，不引入 RPC。

| 文件 | 写入方 | 含义 |
| --- | --- | --- |
| 拓扑 JSON（path/mode 原始字节） | App，在 arm 前 | 启用 VDD 后的握手快照；恢复使用启用前 baseline.json；不含设备实例隐私以外的额外字段 |
| `ready.json` | Recovery | `pid`、`hotkeyRegistered` |
| `arm.json` | App | 必须等于 Recovery 的 pid，之后才允许 APPLY |
| `release.json` | App | 用户恢复全部 / 退出 |
| `result.json` | Recovery；进程已死时也可由 App 补写 | 结束原因与恢复结果 |
| `events.jsonl` | Recovery（App 在补写 result 时也可追加） | 追加时间线：ready / apply / settle / interrupt / reapply / finish。给操作者复盘，不是心跳替代 |
| `vdd-request.json` | Recovery 再关需要自带 VDD 时 | 界面 Poll 后提权 enable 一次；VDD 出现后再 APPLY。未完成不得循环 APPLY |

结束原因需能区分：`hotkey`、`release`、`parent-exit`、`execution-gap`（调度间隙，常见于睡眠）、`unexpected-topology`、`error`、`recovery-exit`（恢复进程已死、未写结果）。未 arm 时也要响应 `release.json`，不得一直停在「等待 arm」。协议 v2 的编号、结构化结果及卸载门禁见 [可靠性修复验收](validation/reliability-v2.md)。

创建恢复进程时使用 `CREATE_BREAKAWAY_FROM_JOB | CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW`。父进程崩溃不得带走恢复进程。恢复进程自身崩溃仍不保证回放拓扑；界面保留失败上下文，用户再次恢复时启动 `--restore-only` 进程。只有重新枚举确认活动物理屏，且本次操作承担启用责任时才尝试 disable；状态未知则保留辅助输出。人工兜底仍是 `Win+Ctrl+Shift+B`，再不行重启。该解绑路径有单元测试；机旁仍未验证解绑。

热键：`MOD_NOREPEAT | MOD_SHIFT | MOD_CONTROL | MOD_ALT` + `VK_F10`（与探针 `0x4007, 0x79` 相同）。注册失败则拒绝关屏，面板必须可见地写「不可用」。

## 6. 界面进程

egui 窗口只承担展示与点击。关屏期间允许隐藏到托盘，后台要求仍由恢复进程维持。当前实现优先使用 wgpu，初始化失败尝试 glow。拓扑切换时的窗口与渲染稳定性仍待机旁验证。

- 单击托盘：打开/前置面板
- 右键：打开面板、恢复全部、退出
- 面板：已连接物理屏列表、每块保持关闭/恢复、底部恢复全部、开机自启（默认关，写当前用户 Startup）、热键状态
- DPI：`PerMonitorV2`
- 退出：先 `release` 并等待恢复结果；超时或失败则报告，不默默退出

托盘图标用 Win32 `Shell_NotifyIcon`（`tray-icon`）。不引入浏览器控件。Windows 上关面板不调用 eframe 0.31 的 `Visible(false)`，也不 `SW_HIDE`：二者都会让 winit 停泵消息，托盘再 `ShowWindow` 会卡住。关面板时把窗口停到屏幕外并保持可见，事件循环继续跑。这是实现对 Windows + eframe 0.31 的约束，不是机旁已验证结论。

## 7. 安装、驱动与提权

### 7.1 安装包内容

引导 EXE 提权后：

1. 安装 `Veil.App.exe`、`Veil.Recovery.exe`、资源到 `%ProgramFiles%\Veil`
2. 展示驱动同意页：自带的是显示驱动，用于没有外接屏时关掉笔记本屏幕
3. 将已签名 MTT VDD 放到 `%ProgramFiles%\Veil\vdd`，用 INF 安装，**设备保持禁用**
4. 校验 CAT/DLL 签名与发布者指纹；不启用测试签名；不静默装
5. 写卸载信息。开机自启不在安装时打开

实验室脚本 `tools/display-probe/install-vdd.ps1` **不得**被安装器直接调用。它拒绝机器上任何已有虚拟适配器，并固定解包到 `C:\VirtualDisplayDriver`，只适合当时那台调研机。产品必须：

- 使用自己的安装目录
- 只认自带硬件 ID（调研包为 `Root\MttVDD`）与已记录发布者指纹
- 允许机器上存在 GameViewer / 向日葵等其它虚拟屏，但永不把它们当退路、不改装它们

具体文件哈希以安装时捆绑的版本为准，写入安装器校验表；更换上游包必须重新核签名与指纹，并在验证文档记一笔。当前文件哈希与 `vdd_settings.xml` 路径见 [installer-payload.md](validation/installer-payload.md)。

### 7.2 按需启用

自带 VDD 是否已安装，以 PnP 设备实例（`Root\MttVDD`）为准，不只看 `%ProgramFiles%\Veil\vdd\MttVDD.inf`。关光最后一块物理屏时：已有设备则按需启用；仅有已校验安装包、设备未出现时，面板说明将**安装并启用**隐藏辅助输出，用户确认后提权 `install-driver` 再 `enable`。取消或失败则物理屏不改动。没有安装包则按最后路径拒绝。这不是本机已装 VDD、也不是 P15 关光双屏已验证的结论。

`Veil.DriverHelper.exe` 清单要求管理员。仅当用户要关光全部物理屏、且当前没有活动的自带虚拟路径时启动。

1. 面板说明将打开隐藏辅助输出，拓扑可能闪断
2. 启用自带设备，等到活动虚拟路径出现（超时则禁用并失败，物理屏不动）
3. 再交给恢复进程停物理路径

退出或恢复全部物理屏后，尽力禁用自带设备，避免留下一块用户没要的虚拟屏。禁用失败要可见，不能假装卸掉了。卸载顺序：恢复物理屏 → 禁用并删除自带设备 → 删文件。

REDMI 上 Rust 产品沿用本机已装禁用态 VDD，做过短时只停内屏（系统检查 + 口头），见 [redmi-book-14-2025-rust.md](validation/redmi-book-14-2025-rust.md)。**不是** Rust MSI / Burn 重装。启用/禁用是否处处不需重启、睡眠后设备是否仍在，仍未验证。不可公开安装，也不得写成全平台可用。

## 8. 仓库布局

### 8.1 目标树

```text
doc/                      合同、架构、验证协议与摘要
src/Cargo.toml            Rust workspace
src/veil-engine/          CCD、角色、门禁、拓扑、VALIDATE、RecoverySession
src/veil-recovery/        独立恢复进程（发布名为 Veil.Recovery.exe）
src/veil-app/             egui 托盘面板（发布名为 Veil.App.exe）
src/veil-driver-helper/   仅启用/禁用自带 VDD（发布名为 Veil.DriverHelper.exe）
installer/Veil.Setup/     应用 MSI
installer/Veil.Bundle/    WiX Burn 引导 EXE
tools/display-probe/      长期保留的 Python 实验室
```

`src/` 为 Cargo workspace。安装器构建要求 `installer/payload/` 中的已核验文件；缺失则失败。捆绑 `MttVDD.dll` 的 UTF-16 字符串写死 `C:\VirtualDisplayDriver`；DriverHelper 安装时把 `vdd_settings.xml` 同时写到 `%ProgramFiles%\Veil\vdd` 与该目录。

### 8.2 现存路径

| 路径 | 性质 | 处置 |
| --- | --- | --- |
| `tools/display-probe/` | 已验证实验室：CCD 结构、VALIDATE 三组、独立 worker、循环、证据收集 | **长期保留**。继续用于机旁对照。可修门禁与记录缺陷，不在其中做产品 UI、安装器或按屏产品状态机 |
| `tools/display-probe/install-vdd.ps1` | 单机调研安装 | 保留作历史步骤；产品安装器重写，不调用它 |
| `src/` | Rust / egui 产品 | 当前实现。不是已发布 |
| `installer/` | WiX 5 + payload 门禁 | 当前实现。无签名，不可公开安装 |
| `doc/validation/` | 可复核摘要 | 机制证据与 Rust 产品附录。原始拓扑字节仍只放主仓库 `.git/veil-validation-*`，不进 Git |

移植的是**行为契约**，不是 Python 文件本身：

- 必须对齐：查询标志、停 ACTIVE、原点调整、VALIDATE 后 APPLY、拓扑原始字节回放、ready/arm 握手、热键、父进程退出恢复、`execution-gap` / 拓扑变化则结束保持关闭、内屏兜底
- 不要对齐：Tk 窗口、仅内屏按钮、实验室 `progress` 哈希负载、预检蜂鸣窗、`SC_MONITORPOWER` 产品入口、扩展桌面失败就改克隆（那只属于 VDD 退路）

Rust 引擎在 P15 与 REDMI 上关屏时，必须重新做机旁观察。移植成功不等于自动继承 Python 报告里的「已验证」。Rust REDMI 短时只停内屏已有系统检查 + 口头，见 [redmi-book-14-2025-rust.md](validation/redmi-book-14-2025-rust.md)。P15 Rust 仍未跑，见 [colorful-p15-24-rust.md](validation/colorful-p15-24-rust.md)。

### 8.3 实现状态

只记依赖与现状，不代替详细计划：

1. `Veil.Engine` + 离线测试：`cargo test --manifest-path src/Cargo.toml` 已有  
2. `Veil.Recovery` 与探针对照的握手：代码已在；P15 机旁未跑  
3. egui 列物理屏、按屏开关、托盘、退出恢复：代码已在；面板点选未机旁  
4. 安装器安装禁用状态的自带 VDD：打包脚本已有；Rust MSI 未机旁  
5. DriverHelper 按需启用 + REDMI 关光内屏：工作树 exe 沿用已装 VDD 做过短时；不是 MSI 重装  
6. 睡醒单次再关（失败即停）：代码已有；尚未机旁  
7. 单屏恢复：缩小 intent 时从保存拓扑一次 APPLY；离线测试已有，机旁未做  
8. 卸载恢复：`--restore-and-exit` 只等待仍有恢复进程的会话。升级由 Burn `retire-old` 先改缓存 MSI、关掉旧 `RestoreDisplays` 再卸旧版；新包遇 `UPGRADINGPRODUCTCODE` 不再跑恢复动作。单独卸载仍检查恢复结果；机旁未做

第 4–5 步未通过前，不得把「无外接关笔记本」写成已发布能力；双物理屏路径可以按已测范围单独验收，但不能因此把 VDD 从首版设计里拿掉。

## 9. 测试与发布

| 层 | 做什么 | 不做什么 |
| --- | --- | --- |
| `cargo test --manifest-path src/Cargo.toml` | ABI、停路径、原点、角色、门禁、会话握手、Coordinator | 不调用真实 `SetDisplayConfig` APPLY |
| Python `display-probe` 单测 | 继续保护实验室 ABI 与 worker 协议 | 不替代产品测试 |
| 机旁 | 按 TECH_VALIDATION：系统检查 + 物理观察 | API 成功单独不算通过 |
| 发布 | 只包含已验证配置上的行为 | 能力检测失败则禁用并说明原因 |

安装包签名、驱动同意文案、卸载恢复，均属发布门禁；未做不得标「可公开安装」。私有仓库可用无签名预览包供协作者下载自用，流程见 [RELEASE.md](RELEASE.md)；预览能装不等于本表门禁已通过。
