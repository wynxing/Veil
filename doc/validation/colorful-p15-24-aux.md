# 验证结果：COLORFUL P15 24 内屏加实体外接

状态：不关屏 VALIDATE、探针只停内屏闭环、当时最小应用（已不在仓库）只停内屏，以及只停外屏短时闭环已记录。只停外屏在把内屏挪到桌面原点后 apply=0，约 14 秒热键恢复。操作者确认外屏灭了、内屏可以用，只在开关时闪。当前产品在 `src/`，见 [P15 Rust](colorful-p15-24-rust.md)。  
日期：2026-09-18

第二组硬件。不能把第一组 REDMI + VDD 的保持关闭结论搬到这台机器。VALIDATE 通过不等于保持关闭成立。

## 环境

| 项 | 记录 |
| --- | --- |
| 计算机 | COLORFUL P15 24，笔记本 |
| 系统 | Windows 11 家庭中文版，64 位，内部版本 26200 |
| GPU | Intel UHD Graphics 32.0.101.7085（两条活动路径均在此适配器）；NVIDIA GeForce RTX 4050 Laptop GPU 32.0.15.8129（当前无活动输出） |
| 物理内屏 | Integrated Monitor，`DISPLAY\CMN1540`，CCD `role=internal`，`\\.\DISPLAY2`，非主屏 |
| 物理外屏 | S24Q6-Q24G8，`DISPLAY\PDA0238`，CCD `role=external`，`outputTechnology=10`（DisplayPort 外接），`\\.\DISPLAY1`，主屏 |
| 拓扑 | 扩展桌面：`gdiMonitorCount=2`，两条路径 `sourceId` 不同（内屏 1 / 外屏 0） |
| 活动路径 | `activeInternal=1`，`activeAuxiliary=1`（辅助是实体外屏，不是虚拟目标） |
| 虚拟适配器 | GameViewer `ROOT\DISPLAY\0000`、OrayIddDriver `ROOT\DISPLAY\0001` 已安装但 GDI `stateFlags=0`，未进入活动 CCD 路径 |
| 电源 | 接电 `PowerOnline=true`；固件支持 S3，不支持 S0 低电量待机（与第一组现代待机不同） |
| 探针 | `tools/display-probe`，Python 3.12，ctypes；单元测试 20+8 通过 |

PnP 中还有大量状态为 Unknown 的历史监视器（如 S2719DGF），未出现在活动 CCD 路径中。

## 本轮结论

- **枚举**：能区分内屏与实体外屏；两块都活动；虚拟适配器未活动。
- **只停内屏、留下外屏**：basic / virtual / refresh 三组 `SDC_VALIDATE` 均为 0，`disabledCount=1`，`remainingActive=1`。扩展桌面下参数组合可接受。这与第一组「仅内屏」或「扩展桌面 + VDD」的 87 不同，也**不是**保持关闭已通过。
- **只停外屏、留下内屏**：仅清除外屏 ACTIVE、外屏仍是主屏时三组均为 87。把留下的内屏源模式挪到 (0,0) 后三组 VALIDATE 为 0（`adjustedOrigin=true`）。这不是 `SDC_TOPOLOGY_INTERNAL`（仅电脑屏幕）。探针 `run-79b325a2`：apply=0，约 13.9 秒后 `reason=hotkey`，restore=0，拓扑一致，`ok=true`。14 次采样内屏仅 1、外屏仅 0，`gdiMonitorCount=1`。操作者确认：**外屏灭了、内屏可以用，开关会闪烁下而已。** 当时最小应用只提供内屏保持关闭。未做只停外屏的 10 分钟、循环或父进程崩溃。
- **停掉全部路径**：三组均为 87，`remainingActive=0`。按协议不得 apply。
- **当时最小应用（已不在仓库）**：门禁已改为：活动内屏且（活动虚拟屏或活动实体外接）即可；仅内屏仍拒绝。`--check` 的 `blockReason` 为空。崩溃后重新预检：`min-app-preflight/preflight.json`，timer 与热键均 `ok=true`。随后调用与按钮同一路径的 `Session.begin_keep_off()`（未开托盘窗口）：VALIDATE `rc=0`，`adjustedClone=false`，worker `--seconds 0`，`run-95c66af9` apply=0，约 14.1 秒后 `reason=hotkey`，restore=0，拓扑一致，`ok=true`。关屏期间 worker 14 次采样内屏仅 0、外屏仅 1。会话文案「已由 Ctrl+Alt+Shift+F10 恢复」。操作者确认：**内屏整段灭着、外屏一直可用。**
- **亮屏预检**：2 秒定时恢复与实际 `Ctrl+Alt+Shift+F10` 均 `ok=true`，凭证 `preflight/preflight.json`。
- **短时只停内屏（热键）**：`run-430dac5c`，apply=0，约 8.1 秒后 `reason=hotkey`，restore=0，拓扑一致。关屏期间 8 次采样均为 `activeInternal=0`、`activeAuxiliary=1`，`gdiMonitorCount` 从 2 变为 1。操作者确认内屏熄灭，外屏闪了一会后重新点亮。
- **满 15 秒只停内屏（定时）**：`run-1236f6d1`，apply=0，15.0 秒后 `reason=timer`，restore=0，拓扑一致。15 次采样内屏仅 0、外屏仅 1。操作者确认开关瞬间闪，内屏整段都灭着。
- **10 分钟只停内屏（定时）**：`run-17834102`，apply=0，600.0 秒后 `reason=timer`，restore=0，拓扑一致。590 次采样内屏仅 0、外屏仅 1。操作者确认：外屏在内屏关闭时闪一次后稳定；内屏重新打开时外屏再闪一次。
- **20×15 秒循环**：`cycles-20`，20/20 `reason=timer`。操作者确认每次只在内屏开关时外屏闪。
- **父进程崩溃恢复**：`run-c8e53c41`。父进程在 worker 就绪后 `os._exit(17)`；独立进程 `reason=parent-exit`，apply=0，restore=0，拓扑一致，约 16ms 后拉回双屏。操作者确认：**内屏短暂关掉后自己亮回。** 探针因 15 秒看门狗分类记 `ok=false`，恢复本身成立。预检凭证已作废。

不要在这台机器上安装 MTT VDD，也不要激活 GameViewer / 向日葵来凑虚拟目标。

## 结果表

| 步骤 | 机制 | API 结果 | 物理观察 | 输入是否唤醒 | 恢复是否成功 | 结论 |
| --- | --- | --- | --- | --- | --- | --- |
| 0 | 枚举 | 内屏 + 外屏各 1 条活动路径；`activeAuxiliary=1` | 两块屏均点亮（机旁当前布局） | 不适用 | 不适用 | 第二物理目标存在 |
| 1 | 三组原样拓扑 VALIDATE | 均为 0 | 无关屏 | 不适用 | 不适用 | 对照通过 |
| 2 | 三组只停内屏 VALIDATE | 均为 0；剩余 1 条外屏 | 无关屏 | 不适用 | 不适用 | 参数可接受；未 apply |
| 3 | 三组只停外屏 VALIDATE | 仅清 ACTIVE 均为 87；内屏挪到原点后均为 0 | 无关屏 | 不适用 | 不适用 | 主屏外接上须调整原点 |
| 4 | 三组停全部 VALIDATE | 均为 87；剩余 0 | 无关屏 | 不适用 | 不适用 | 安全上不得 apply |
| 5 | 最小应用 `--check` | 改门禁后 `blockReason` 为空；`virtualCount=0` | 无关屏 | 不适用 | 不适用 | 门禁与本机拓扑匹配 |
| 6 | 亮屏预检 timer + 热键 | 两次 restore=0，`ok=true` | 无关屏 | 不适用 | 热键约 1s 命中 | 预检通过（崩溃前） |
| 6b | 最小应用前重新预检 | timer + 热键均 `ok=true` | 无关屏 | 不适用 | 热键约 15s 命中 | 新凭证 `min-app-preflight` |
| 7 | 只停内屏短时 apply | apply=0；8 次采样内屏仅 0、外屏仅 1；8.1s 热键 restore=0 | 操作者：内屏灭了；外屏闪了一会亮了 | 合成输入后路径仍停用；非物理键鼠验收 | 热键恢复成功 | 目标屏关闭成立；外屏有切换闪烁 |
| 8 | 只停内屏满 15 秒 | apply=0；15 次采样内屏仅 0、外屏仅 1；15.0s timer restore=0 | 操作者：开关瞬间闪；内屏整段都灭着 | 合成输入后路径仍停用 | 定时恢复成功 | 短时闭环成立；外屏仅切换闪烁 |
| 9 | 只停内屏 10 分钟 | apply=0；590 次采样内屏仅 0、外屏仅 1；600s timer restore=0 | 关闭时闪一次后稳定；内屏开时外屏再闪一次 | 合成输入后路径仍停用 | 定时恢复成功 | 10 分钟闭环成立；外屏仅开关边缘闪 |
| 10 | 20×15 秒循环 | 20/20 timer，apply/restore=0；每次采样内屏 0 / 外屏 1 | 操作者：每次只在内屏开关时外屏闪 | 每次含合成输入 | 20 次定时恢复 | 循环闭环成立 |
| 11 | 父进程崩溃 | 父进程退出 17；worker `parent-exit`，restore=0，拓扑一致，约 16ms | 操作者：内屏短暂关掉后自己亮回 | 不适用 | 独立进程拉回双屏 | 崩溃恢复成立 |
| 12 | 最小应用 `Session.begin_keep_off` | apply=0；14 次采样内屏仅 0、外屏仅 1；14.1s 热键 restore=0；未改克隆；界面「已确认」 | 操作者：内屏整段灭着、外屏一直可用 | 未做合成输入 | 热键恢复成功 | 最小应用短时闭环成立 |
| 13 | 只停外屏短时 apply | apply=0；原点调整；14 次采样内屏仅 1、外屏仅 0；13.9s 热键 restore=0 | 操作者：外屏灭了；内屏可以用；开关会闪烁下而已 | 合成输入后外屏仍停用 | 热键恢复成功 | 短时闭环成立；内屏仅切换闪烁 |

## 证据

原始文件在本机主仓库 `.git/veil-validation-20260918-p15/`，不随 Git 分发。

| 文件 | SHA-256 |
| --- | --- |
| `before/diagnostic.jsonl` | `e1bdd858ce845287d2ee0242a10a72f2ccd23847ca082c14153356ee9a48db94` |
| `before/diagnostic-extra.jsonl` | `7b20bdf61d7cebbddc95e54677d31d44d5a75eb1c70e609938e049bed50f1a03` |
| `before/enumerate.json` | `753baeddb51e44d0757f6926d6a8261ffec9017fb6bfae35fcc2972754cbe9a6` |
| `before/baseline.json` | `5b1d7a5507386c7c787dcd149d6425b959cccd51a29764dac1bf1c2102e01da4` |
| `before/veil-check.json` | `661f9e171dab007e35694e5ca6a6af7606c87afda85e28805124ae295b04eb02` |
| `after-gate/veil-check.json` | `86a0d88eee9edc50f4bead66b8e2ee59a39a67bede42eb2d5583f5b32990112e` |
| `before/sleep-states.txt` | `64c319bad1c19c5eaba47394c0390af3c8ce0f68564aee84fc3e77e1c72ca724` |
| `run-15s-internal/run-430dac5c/summary.json` | `325d920683b9e3e0c64ffe12e392328a93672b6812a6b0b87f3412ee096388a5` |
| `run-15s-internal/run-430dac5c/recovery.jsonl` | `516e6ffdef7b40e62912159ddc5bc6214c0b807ccfcac07a5f428e6a87071389` |
| `run-15s-internal-timer/run-1236f6d1/summary.json` | `2604edb4e4108d7220a50ccceaff9ddc5602d1491b24ecf4f58f49368b2c9d18` |
| `run-15s-internal-timer/run-1236f6d1/recovery.jsonl` | `339c8bd158bb645c64ff561a89906a166b7d19002cbd658e0991cfcd366fd6cb` |
| `run-600s-internal/run-17834102/summary.json` | `90d58d281bfa7d3cd266acc525f828bb92ce80a508d89e651ae7883652c2d237` |
| `cycles-20/manifest.json` | `92d154b9ead5bccaf0953e3b539c179402ddf75c6bb692726f028661d06cb239` |
| `cycles-20/group-summary.json` | `ced7659751150eece06f007b9e2b13233ea9b4f72e4b3e191f238beaf503a6bf` |
| `run-parent-crash/run-c8e53c41/result.json` | `47a8261bff66b559c66b04cd4bd5ca52455d8b16fc62c884f017ed9b597d88a5` |
| `min-app-preflight/preflight.json` | `5789609d4fe7451ffdcfdfc5f814aaed33c48ae90aaaef19fbca75ae11a151b0` |
| `min-app-keep-off/validate.json` | `2cf9809094c165dead4fe78cd61fba2c8797df271b600e763b369ead5f279b77` |
| `min-app-keep-off/run-95c66af9/result.json` | `d4684f4341696f500a121820db16f6cf75b6752216823a971918c58f5af44886` |
| `min-app-keep-off/run-95c66af9/recovery.jsonl` | `23d5fde44e84e3c73f2dbfc62b5d2927f203f8dbb235373a986450264fc0b0c5` |
| `disable-external/validate.jsonl` | `75fc57f3509d50d56abc46e44a72d353205433f9a49ab758c6fe97d3c0ac6644` |
| `disable-external/run-15s/run-79b325a2/result.json` | `5f608ae49eebfb597db33b335184885819d2501ed8947f83e9ebb582a9be6d7b` |
| `disable-external/run-15s/run-79b325a2/recovery.jsonl` | `742ce008d03ddd19d50b7517a881a0e744fd3af1618f106f4c99c20bc12f366a` |

## 下一步

1. Rust 产品机旁见 [P15 Rust](colorful-p15-24-rust.md)，尚未执行。
2. Python 只停外屏短时闭环已通过。未做只停外屏的 10 分钟、循环或父进程崩溃。
3. 睡眠唤醒尚未测。
