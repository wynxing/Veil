# 验证结果：REDMI Book 14 2025 上的 C# 安装器 VDD 短时闭环

状态：本机已卸实验室 MTT，用无签名 MSI（`INSTALLVDD=1`）装上禁用态自带 VDD。**第一次**短时只停内屏有系统检查；操作者确认关屏期间**内屏灭了**。**第二次**手动再关未 APPLY。**第三次**覆盖 `63e1057` 后短时 APPLY，热键恢复。**第四次**墙钟约 12 分钟，操作者确认稳定黑屏 10 分钟以上。**第五次** C# **20×15 秒**循环系统检查 20/20：关屏采样均为内 0 / 辅 1，`reason=release`，恢复后 VDD Code 22。循环时提权启动界面进程以避免每轮 UAC；未逐次口头记画面。**第六次**工作树构建合盖约 37 秒：没有 `reapplied`。**第七次**同一构建合盖两次：第一次醒后 `reapplied` applyRc=0（系统检查）；第二次合盖按单次再关额度结束。**第八次**单次合盖约 8 秒：`reapplied` applyRc=0，操作者确认开盖后内屏灭着，约 4 秒后热键恢复 `ok=true`。这是短时睡醒再关的系统检查 + 口头，不是长时、也不是全平台闭环。不是已发布、不可公开安装。不得把 Python [redmi-book-14-2025-vdd.md](redmi-book-14-2025-vdd.md) 写成 C# 已过。  
日期：2026-09-19

## 环境

| 项 | 记录 |
| --- | --- |
| 计算机 | XIAOMI REDMI Book 14 2025(FHD+)，接电 |
| 系统 | Windows 11 家庭中文版，内部版本 26200 |
| 拓扑 | 不接外屏。装前仅内屏 `DISPLAY\TMA0813` `targetId=8388688` |
| 软件 | 工作树 `verify/csharp-redmi-vdd` 打出的 `Veil.msi` / `VeilSetup.exe`；APPLY 仅 `Veil.Recovery` |
| 安装方式 | `msiexec /i Veil.msi INSTALLVDD=1 /qn`。Burn 无单独驱动同意页；本轮未走交互 Burn UI |
| 会话 | 首次 `session-93d37ce1`；第二次 `session-193371b1`；第三次 `session-4e437d38`；第四次 `session-139b5929` |
| 证据 | 本机主仓库 `.git/veil-validation-20260919-csharp-redmi/`，不随 Git 分发 |

## 本轮先做的实验室清理

装产品包之前，本机仍有实验室 MTT：`ROOT\DISPLAY\0000` 活动，`oem58.inf`，`C:\VirtualDisplayDriver` 中 DLL/INF/CAT 哈希与 [payload.manifest.json](../../installer/payload.manifest.json) 一致，`vdd_settings.xml` 与产品字节一致。未见 GameViewer / 向日葵。`%ProgramFiles%\Veil` 当时不存在。

未调用 `tools/display-probe/install-vdd.ps1`。提权后：`Disable-PnpDevice` → `pnputil /remove-device ROOT\DISPLAY\0000` → `pnputil /delete-driver oem58.inf /uninstall /force`，并把 `C:\VirtualDisplayDriver` 挪到证据目录。其后显示设备只剩 Intel UHD。不得把这次清理写成产品卸载已验收。

## 安装（系统检查）

本地 `pack.ps1` 打出无签名包：

| 文件 | 字节 | SHA-256 | Authenticode |
| --- | --- | --- | --- |
| `Veil.msi` | 712704 | `FC5DE95180E96FA0CE9EACD4A71A8E72DA3948381545DE58145FCB076D238D27` | NotSigned |
| `VeilSetup.exe` | 1752581 | `D3B7D4FF8CA4D8F77D4886E8D79B438B97C26735DE0CE2E60CD62990136119C7` | NotSigned |

`msiexec` 退出 0。装完：

- `%ProgramFiles%\Veil` 有 App / Recovery / DriverHelper 与 `vdd` / `nefcon`
- `ROOT\DISPLAY\0000` 为 `CM_PROB_DISABLED`（Code 22）
- `DriverHelper status`：`installed=true`，实例 `ROOT\DISPLAY\0000`
- 未出现开机自启快捷方式，未自动关屏
- `vdd_settings.xml` 同时在 `%ProgramFiles%\Veil\vdd` 与 `C:\VirtualDisplayDriver`（哈希 `EAD76E2AE5E8BC82AAF9A38DC21608DB6B2A42B024638CD42537E40E6B88BECE`）

这只证明本机 quiet 安装后设备保持禁用。没有 Authenticode，**不可公开安装**。

## 短时闭环（系统检查）

1. 装后枚举：`activeInternal=1`、`activeAuxiliary=0`。  
2. 启动已安装的 `Veil.App`。面板点内置「保持关闭」。出现启用说明（`Gate.EnableVddReason`），点确定。`Veil.DriverHelper enable` 经 UAC。  
3. `ready.json`：`pid=6524`，`hotkeyRegistered=true`。`intent.json`：只关内屏，`vddAssist=true`。  
4. 关屏中：`applyRc=0`；CCD `activeInternal=0`、`activeAuxiliary=1`；活动路径为 `VDD by MTT` / `DISPLAY\MTT1337` / `ROOT#DISPLAY#0000`。心跳「已关闭」。看门狗采样 `enumerate-0` 至 `enumerate-7` 与 `during/enumerate.txt` 均为内 0 / 辅 1。`adjustedClone=false`（本轮未改克隆）。操作者事后确认：**内屏灭了**。未单独说辅助输出是否可见。  
5. 约 25 秒后写入 `release.json`（看门狗 90 秒总时限先到期，由脚本补写；仍是文件协议恢复，不是热键）。`result.json`：`reason=release`，`ok=true`，`restoreRc=0`，`restoredTopology=true`。  
6. 恢复后枚举与装后基线同一哈希；`ROOT\DISPLAY\0000` 再次 `CM_PROB_DISABLED`。

## 结果表

| 步骤 | 机制 | API / CCD | 物理观察 | 结论 |
| --- | --- | --- | --- | --- |
| 1 | 卸实验室 MTT | 设备删除，`oem58.inf` 去掉 | 未口头记画面 | 产品安装前残留已清；不是产品卸载验收 |
| 2 | 安装器装禁用 VDD | 实例在、Code 22 | 未口头确认同意页（quiet MSI） | 本机禁用态安装成立；无同意页、无签名 |
| 3 | 面板确认后 enable | 设备变 OK，出现活动 `Root\MttVDD` 路径 | 未口头确认闪断 | 启用路径有系统检查 |
| 4 | 只停内屏 | apply=0；内 0 / 辅 1 | 操作者：内屏灭了 | 短时保持关闭：系统检查 + 内屏口头成立 |
| 5 | `release.json` 恢复 | restore=0，拓扑回到装后基线 | 未口头确认闪一下 | 文件协议恢复成立 |
| 6 | 恢复后 disable | 再次 Code 22 | 未口头确认虚拟屏消失 | 系统检查：未留下活动自带 VDD |
| 7 | 亲手热键预检 / 父进程崩溃 | 未跑 | 未做 | 未执行 |
| 12 | 第八次单次合盖再关 | `session-b6b9baa2`：`reapplied` apply=0，`reason=hotkey` ok | 操作者：开盖后内屏灭 | 短时睡醒再关成立；睡眠约 8 秒，再关后约 4 秒热键 |
| 8 | 第二次手动再关 | 无 APPLY、无 `result.json` | 操作者：无黑屏、无闪屏 | 关屏未落地；面板假死 |
| 9 | 覆盖 `63e1057` 后再关 | 采样 0–4 内 0 / 辅 1；`reason=hotkey` restore=0 | 未口头记画面 | 短时 APPLY 有系统检查；不是 15 秒 release 闭环 |
| 10 | 操作者自测约 12 分钟 | `session-139b5929` 墙钟 719 s；`applyRc=0`；`reason=hotkey` | 操作者：稳定黑屏 10 分钟以上 | 长时口头成立；无关屏中连续枚举 |
| 11 | 20×15 秒循环 | 20/20 `reason=release`；每轮 14 次采样内 0 / 辅 1；restore=0；VDD Code 22 | 未逐次口头 | 系统检查成立；提权启动界面；不是日常非提权 20 次 |

## 第二次手动再关（失败）

约 10:28–10:29，已安装的 `Veil.App` 仍在跑（pid 3384，10:12 启动）。新会话 `session-193371b1`：

- `ready.json` pid 1508，`hotkeyRegistered=true`；`arm.json` 已写
- 心跳一直 `armed=false`、文案「等待 arm。」
- `intent.json`：只关内屏，`vddAssist=true`
- 约 1 秒后有 `release.json`；**没有** `result.json`；留下 `heartbeat.json.tmp`
- `Veil.Recovery` 已不在。未 arm 时恢复进程不看 `release.json`，界面又只等 `result.json`，所以停在「等待 arm。」，保持关闭 / 恢复都灰
- 当时 CCD：`activeInternal=1`、`activeAuxiliary=1`，同一 `sourceId`，`gdiMonitorCount=1`（克隆）。内屏路径仍活动，所以没有黑屏、也没有闪屏
- `ROOT\DISPLAY\0000` 仍为 OK；产品该做的恢复后 disable 没做成

这与第一次 `session-93d37ce1`（关屏中内 0 / 辅 1）不是同一结果。不能把第二次写成保持关闭成立。

界面关掉后，把该会话拷到证据目录，并在内屏仍活动的前提下禁用留下的 VDD。代码侧补了：恢复进程死后由 App 写 `recovery-exit` 并结束会话；未 arm 也响应 `release.json`；心跳写入失败不再把恢复进程打挂。

## 第三次覆盖二进制后再关

13:55 用 `main` `63e1057` 的 Release 覆盖 `%ProgramFiles%\Veil` 的 App / Recovery / Engine / DriverHelper，**没有**重打 MSI。VDD 覆盖前已是 `CM_PROB_DISABLED`。

`session-4e437d38`：`ready.json` pid 5420，热键已注册；`intent.json` 只关内屏、`vddAssist=true`。脚本没点到启用确认框（本机可能已手动点过 UAC/确定）。

CCD：`enumerate-0` 至 `enumerate-4` 为 `activeInternal=0`、`activeAuxiliary=1`；`enumerate-5` 起回到内 1 / 辅 0。心跳「已保持关闭。」`result.json`：`reason=hotkey`，`ok=true`，`applyRc=0`，`restoreRc=0`，`adjustedClone=false`。约 8 秒内恢复进程已退出——这是热键正常结束，不是再次中途崩溃。脚本后写的 `release.json` 来晚了，不计入本次恢复机制。

恢复后：`ROOT\DISPLAY\0000` 再次 Code 22；枚举哈希与清理后基线相同；面板「保持关闭」可点。未复现「恢复进程死、无 result、面板假死」。未做满 15 秒、未口头确认内屏/闪屏。不可公开安装。

## 第四次操作者约 12 分钟

同一已覆盖构建、同一 `Veil.App` pid 6948。`session-139b5929`：`ready.json` 13:58:16，`result.json` 14:10:15，墙钟 719 秒。`intent.json` 只关内屏，`vddAssist=true`。`ready.pid=5524`，热键已注册。最后一份心跳仍是「已保持关闭。」（恢复前最后一次写入）。`result.json`：`reason=hotkey`，`ok=true`，`applyRc=0`，`restoreRc=0`，`adjustedClone=false`。无 `release.json`。

操作者事后确认：可以保持**稳定黑屏 10 分钟以上**。本轮没有关屏中的 `enumerate-*` 时间序列，不能写成与 Python 10 分钟后台采样同等。恢复后枚举回到内 1 / 辅 0，`ROOT\DISPLAY\0000` 为 Code 22。

## 第五次 20×15 秒循环（系统检查）

14:16–14:24（UTC 06:16:21–06:24:48），同一覆盖构建。为避免每轮 `DriverHelper` UAC，**提权启动** `Veil.App` 与循环脚本；每轮先 `enable` 出自带 VDD，再点「保持关闭」，15 秒后写 `release.json`。日常非提权每次关屏仍会要 UAC，本轮不证明那条。

`manifest.json`：`completed=true`，`okCount=20`。每一轮：

- 关屏中 14 次 CCD 采样均为 `activeInternal=0`、`activeAuxiliary=1`（全部 `enumerate-*.txt` 无内屏仍活动）
- `result.json`：`reason=release`，`ok=true`，`applyRc=0`，`restoreRc=0`
- 恢复后等待辅助路径消失，`ROOT\DISPLAY\0000` 为 `CM_PROB_DISABLED`

结束时枚举回到内 1 / 辅 0，与清理后基线同一哈希。未逐次口头确认黑屏/闪屏。不可公开安装。

## 未做

- 操作者口头确认辅助输出是否可见、恢复是否闪一下  
- 预检式亲手热键（第三次结果是热键恢复，但不是按预检脚本按的）  
- Burn 交互同意页；`INSTALLVDD=0` 只装应用  
- C# 父进程崩溃  
- 非提权界面下的 20 次循环（本轮为提权启动）  
- 关屏中连续 CCD 采样的 10 分钟（本次只有墙钟 + 口头）  
- 睡醒后再关的长时保持、P15 睡眠、第二次睡眠（额度用尽是设计）  
- 睡醒后连续 CCD 采样  
- 代码签名、可公开安装
- 用新构建复现「恢复进程死后面板解绑」

## 合盖约 3 秒（informal，不是睡醒验收）

15:09 操作者合盖。会话 `session-26315504`，`vddAssist=true`。

| 墙钟 | 文件 / 系统 | 含义 |
| --- | --- | --- |
| 15:09:40 | ready / arm / intent / topology | 开始只停内屏 |
| 15:09:49 | heartbeat `已再次保持关闭。` | **睡眠前**已经再关过一次 |
| 15:09:53 | Kernel-Power 506，原因 Lid | 进入现代待机 |
| 15:09:56 | Kernel-Power 507，原因 Lid；`result.json` | 退出待机。`reason=unexpected-topology`，`reapplyAttempted=true`，`restoreRc=0` |

当时构建没有 `events.jsonl`，heartbeat 被覆盖，面板把 `ok=false` 写成「恢复未完全成功」，即使拓扑已经拉回。再关额度是整段会话共用一次：VDD/拓扑抖动会先把它用掉，合盖醒来就不能再关。

这只说明本机发生过合盖与一次再关，**不得**写成 C# 睡醒闭环通过。没有关屏中连续枚举，也没有操作者口头记画面。原始副本在 `.git/veil-validation-20260919-csharp-redmi/lid-informal-26315504/`（拓扑字节不进 Git）。

| 文件 | SHA-256 |
| --- | --- |
| `result.json` | `6E6E57D610D6B138D6B6D1A0B4B0ED9D4B3399BFEE761F190B1CBD8B0AEA9527` |
| `heartbeat.json` | `5676D99C1D6F55F7FEA7EA974A8E750499193AD7DF706F5EBE004C3D7848EEEB` |
| `intent.json` | `277605327C54BF57230F59FEB260BD631B975C6C3EAB7808C0F7BEF9D9C055C6` |
| `ready.json` | `0F9D3AD709A8E395559BAA6A29F965E0DF4D41284E825663C800125CE5102E84` |
| `kernel-power.txt` | `94CE74247E12CB0A16BC00FEEF7438D66CC26AAF0F63733846E3747887EFF401` |

## 合盖约 37 秒（有 events.jsonl，再关未站住）

15:24 用工作树 `verify/csharp-sleep-wake` Debug 构建，非提权启动 `Veil.App`。会话 `session-488047d1`，`vddAssist=true`，热键已注册。

| 墙钟 | 来源 | 含义 |
| --- | --- | --- |
| 15:24:04 | ready / armed | 恢复进程就绪 |
| 15:24:05 | `applied` applyRc=0 | 只停内屏落地 |
| 15:24:11 | Kernel-Power 506，原因 Lid | 进入现代待机 |
| 15:24:17 | Kernel-Power 172 | Adaptive Connected Standby，Disconnected |
| 15:24:48 | Kernel-Power 507，原因 AC/DC Display Burst | 退出现代待机 |
| 15:24:48.162 | `interrupt` execution-gap | 调度间隙认出睡眠 |
| 15:24:48.196 | `reapply-attempt` | 按合同尝试再关一次 |
| 15:24:49.644 | `interrupt` unexpected-topology | **没有** `reapplied`；约 1.4 秒后拓扑再次对不上 |
| 15:24:49.733 | `finish` unexpected-topology | `reapplyAttempted=true`，`restoreRc=0`，`restoredTopology=true` |

醒后本机枚举：`activeInternal=1`、`activeAuxiliary=0`。`ROOT\DISPLAY\0000` 为 Code 22。界面进程仍在。保持关闭没有在醒后继续。

操作者口头：开盖后**闪了一下**。这与 `execution-gap` 先回放拓扑、再尝试一次再关、约 1.4 秒后因拓扑对不上而结束相符。闪一下不是醒后仍保持关闭，也不能单独证明再关 APPLY 落地（`events.jsonl` 没有 `reapplied`）。

这证明：日志能对上合盖睡眠；`execution-gap` 会触发单次再关。**不证明**醒后内屏能再次保持关闭。VDD 是否挺过现代待机、再关失败的 CCD 原因，本轮没有关屏中采样。原始副本在 `.git/veil-validation-20260919-csharp-redmi/sleep-wake-488047d1/`。

| 文件 | SHA-256 |
| --- | --- |
| `events.jsonl` | `137D05A3E4F9D7A83E3318C109F8EC0C69B49B3BA7C8F8F3793345A582C1B28E` |
| `result.json` | `FEAA35DB758625331AA3BD99A01E758037956B006C1EC83CDFDFFAA068688B10` |
| `heartbeat.json` | `7F1FD155CD5F495BA3F0B5A5EF9A98AB02C42112AC950DD71D086DEA4786C542` |
| `kernel-power.txt` | `0A092733F3CDF7165A87F0B10EA197E8D02BB7224F0015A1F84E8FECDBF104DE` |

### 再关未落地的代码原因（15:24，不是修复后的复测）

`events.jsonl` 有 `reapply-attempt`，没有 `reapplied` / `apply-blocked` / `already-off`。当时 `TryApply` 在「所选物理屏已经不活动且还有活动路径」时直接视为成功，**不写日志、不再 APPLY**。合盖前保存的拓扑是双路径（内屏+VDD）。醒来 `RestoreSaved` 会先把内屏拉亮（操作者见到闪一下），紧接着的 CCD 查询仍可能停在睡眠中的「内屏已关」。于是再关被当成已经关上，约 1.4 秒后内屏真正亮起，对不上期望目标，走 `unexpected-topology`。恢复进程也不能自己提权启用 VDD；若现代待机丢掉虚拟路径，再关只能停。

工作树随后补了：再关前等待所选物理屏重新出现；该静默分支写入 `already-off`；再关若需要 VDD 则写 `vdd-request.json`，由界面 enable 一次。15:43 复测见下，不得把 15:24 写成已修好。

## 合盖两次（15:43，第一次再关有 APPLY）

同一工作树 Debug 构建，会话 `session-01393423`，`vddAssist=true`。Kernel-Power 记录**两次**合盖。没有 `vdd-request.json`。

| 墙钟 | 来源 | 含义 |
| --- | --- | --- |
| 15:43:35 | `applied` applyRc=0 | 只停内屏落地 |
| 15:43:44 | Kernel-Power 506，原因 Lid | 第一次进入现代待机 |
| 15:43:54 | Kernel-Power 507，原因 Lid | 第一次开盖，约 10 秒 |
| 15:43:54.292 | `interrupt` execution-gap | 认出第一次睡眠 |
| 15:43:54.682 | `reapply-settle` attempt 1 | 所选内屏已重新活动 |
| 15:43:55.688 | `reapplied` applyRc=0 | **第一次醒后 APPLY 再关落地（系统检查）** |
| 15:44:01 | Kernel-Power 506，原因 Lid | 第二次合盖 |
| 15:44:16 | Kernel-Power 507，原因 Lid | 第二次开盖，约 15 秒 |
| 15:44:16.207 | `interrupt` execution-gap，`reapply=true` | 额度已用，不再关 |
| 15:44:16.765 | `finish` execution-gap | `restoreRc=0`，`restoredTopology=true` |

醒后枚举：内 1 / 辅 0。VDD Code 22。合同是整段会话只再关一次，第二次合盖结束保持关闭是预期，不是回归。

系统检查：第一次醒后 `reapplied` 成立。第一次开盖后内屏是否再次灭着、灭了多久，**口头未录入**，因此还不能写「睡醒闭环通过」。原始副本在 `.git/veil-validation-20260919-csharp-redmi/sleep-wake-01393423/`。

| 文件 | SHA-256 |
| --- | --- |
| `events.jsonl` | `99395672CCC115A876D7349CBE2FF94E5A7D5A844E3ADA10D7BDEF995A99ECB0` |
| `result.json` | `2C8DADB48E4A8DA0B1EDEA87F70A35B94637BC2AFCA6AA5A358E473B175B61B4` |
| `kernel-power.txt` | `30EE046B3C6E3270BBEABB966002F9CB7F67466F2D1E43F5562F8137EB4F937D` |

## 单次合盖约 8 秒（开盖后灭，热键恢复）

15:48 同一 Debug 构建。会话 `session-b6b9baa2`。只合盖一次。

| 墙钟 | 来源 | 含义 |
| --- | --- | --- |
| 15:48:59 | `applied` applyRc=0 | 只停内屏落地 |
| 15:49:11 | Kernel-Power 506，原因 Lid | 进入现代待机 |
| 15:49:19 | Kernel-Power 507，原因 Lid | 开盖，约 8 秒 |
| 15:49:19.215 | `interrupt` execution-gap | 认出睡眠 |
| 15:49:19.525 | `reapply-settle` attempt 1 | 内屏已重新活动 |
| 15:49:20.682 | `reapplied` applyRc=0 | 再关 APPLY 落地 |
| 15:49:24.318 | `finish` hotkey | `ok=true`，`restoreRc=0` |

操作者口头：合盖后再打开，**内屏是灭的**。没有第二次合盖。恢复是热键，不是 `release.json`。无 `vdd-request`。再关后到热键约 4 秒，没有醒后长时采样。不得写成 10 分钟睡醒保持，也不得写成全 Windows 兼容。原始副本在 `.git/veil-validation-20260919-csharp-redmi/sleep-wake-b6b9baa2/`。

| 文件 | SHA-256 |
| --- | --- |
| `events.jsonl` | `1B9732B0950AF3DB33DC4CCCB3BFDB3CBFFFC7B576B32BE3E8B6CB95B288B4F3` |
| `result.json` | `2C0B83D490645886BF0F954E7624D2248AE8599C8E3D7F5C110067448D0A9542` |
| `kernel-power.txt` | `02765E7B0FE780F707B513E7E6470DDB0BFE0FE7381EC9069FBB06BEC23F0085` |

## 证据

原始文件在本机主仓库 `.git/veil-validation-20260919-csharp-redmi/`。

| 文件 | SHA-256 |
| --- | --- |
| `lab-baseline.json` | `EACA69392B43632498832122B2032A00678116A7275F6DFAE44CB54832612A2A` |
| `lab-uninstall.log` | `E5A042EC9530192BA9B1502B8FBEB2F3F15FB62A14DE6F62AE162FD9832D74E2` |
| `before/enumerate.txt` | `9294DADAB4ABC4DEE34E79A074DF836A4F350CF67D8E054FFDBB3E67EF59E2D2` |
| `during/enumerate.txt` | `57A4E1DA4412B5CA21F0FDDA3342B36C1EC5435DDD0645588A575F0FC3BF9B2B` |
| `during/intent.json` | `AD03D72858FC818B1D454A9AF2E9E9533845BF2190CD8F51A069B8F4C35B40F3` |
| `during/ready.json` | `C0F778C5CC28F57A26C80B63E26A56E1F5B85B8E88082B6BA6CFB756C5210736` |
| `during/heartbeat.json` | `076EEABACF6F48F23DD0CA033B3DA9B72136B6968BE7378C9E992B11300FDFA1` |
| `after/enumerate.txt` | `9294DADAB4ABC4DEE34E79A074DF836A4F350CF67D8E054FFDBB3E67EF59E2D2` |
| `after/session-93d37ce1/result.json` | `C1B20411079660125CBE6196F1ECB045F91EF4DB161A90F6220CDBFAC8913774` |
| `after/session-93d37ce1/release.json` | `708077B6E1634D2A52BE81C1B74568053AB4F96BDE5AE6F6F9F622AB9600F2D3` |
| `driver-status.json` | `89E784812A71E8CFBFC6E3C0993729FC680913F4988285229B5A0803D8789412` |
| `manual-orphan-193371b1/before-cleanup-enumerate.txt` | `22B9E9B088BF3E6F0E463C53262C3C1BDCD65CD405954898C2ED5A011348A8A7` |
| `manual-orphan-193371b1/session-193371b1/intent.json` | `AD03D72858FC818B1D454A9AF2E9E9533845BF2190CD8F51A069B8F4C35B40F3` |
| `manual-orphan-193371b1/session-193371b1/heartbeat.json` | `E3658C5F6DF7C5B063098FA7609F14317248EAE71153C9F158C00B066D822A00` |
| `manual-orphan-193371b1/session-193371b1/release.json` | `813C03CE27824A5495208AD986F0866AC53C26AA4C13D4431B402CB071FED2B3` |
| `manual-orphan-193371b1/after-cleanup-enumerate.txt` | `BFA179380F28EDAE11E5B0944A47AFD2072A12B8ACFEA62C6C39FB5195C3694C` |
| `manual-orphan-193371b1/after-pnp.txt` | `AC937B3BFE72DD1EF1E70DB7925A5694DE6CC330224BDCB22AE28ACC83C73264` |
| `retest-orphan-fix/before-enumerate.txt` | `BFA179380F28EDAE11E5B0944A47AFD2072A12B8ACFEA62C6C39FB5195C3694C` |
| `retest-orphan-fix/during/enumerate-0.txt` | `0EF4A07BF5124C2275B0D2134CEE5D489F6A813BD70A90975D6B2F7459EA05F8` |
| `retest-orphan-fix/during/enumerate-5.txt` | `BFA179380F28EDAE11E5B0944A47AFD2072A12B8ACFEA62C6C39FB5195C3694C` |
| `retest-orphan-fix/session-after/result.json` | `C6355DDD6F99A86FA21254523EF12C2BEBD178AF2A69BD2C095C5C77D55705FF` |
| `retest-orphan-fix/session-after/intent.json` | `277605327C54BF57230F59FEB260BD631B975C6C3EAB7808C0F7BEF9D9C055C6` |
| `retest-orphan-fix/after-enumerate.txt` | `BFA179380F28EDAE11E5B0944A47AFD2072A12B8ACFEA62C6C39FB5195C3694C` |
| `retest-orphan-fix/after-pnp.txt` | `AC937B3BFE72DD1EF1E70DB7925A5694DE6CC330224BDCB22AE28ACC83C73264` |
| `operator-10min-139b5929/session-139b5929/result.json` | `C6355DDD6F99A86FA21254523EF12C2BEBD178AF2A69BD2C095C5C77D55705FF` |
| `operator-10min-139b5929/session-139b5929/intent.json` | `277605327C54BF57230F59FEB260BD631B975C6C3EAB7808C0F7BEF9D9C055C6` |
| `operator-10min-139b5929/timing.txt` | `BDBC4C4760AC5460D384EC0EADF456C02E50B5EC8ACFBDF8498D907991B50D5A` |
| `operator-10min-139b5929/after-enumerate.txt` | `BFA179380F28EDAE11E5B0944A47AFD2072A12B8ACFEA62C6C39FB5195C3694C` |
| `operator-10min-139b5929/after-pnp.txt` | `AC937B3BFE72DD1EF1E70DB7925A5694DE6CC330224BDCB22AE28ACC83C73264` |
| `cycles-20/manifest.json` | `890D8D747A12F6DB9088F31DFF9C499C719B4673B0B5C266B1D22C4780D1E1EC` |
| `cycles-20/cycles.log` | `BF8F3C4E9FFDE892ADDE97D1AA573619EFBF08422303D19DD493C469B710242D` |
| `cycles-20/cycle-01/enumerate-0.txt` | `4F53326DAD2B3DD42C313A03AD22BF25B2FA6E85366583E047446EB31780A549` |
| `cycles-20/cycle-01/session/result.json` | `C1B20411079660125CBE6196F1ECB045F91EF4DB161A90F6220CDBFAC8913774` |
| `cycles-20/cycle-20/enumerate-0.txt` | `C43505AC3F2F94DB8A239A5482E4F7032431E3131C783DAA3E2E1D68F94F7418` |
| `cycles-20/cycle-20/session/result.json` | `C1B20411079660125CBE6196F1ECB045F91EF4DB161A90F6220CDBFAC8913774` |
| `cycles-20/after-enumerate.txt` | `BFA179380F28EDAE11E5B0944A47AFD2072A12B8ACFEA62C6C39FB5195C3694C` |
