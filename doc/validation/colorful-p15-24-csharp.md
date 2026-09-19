# 验证结果：COLORFUL P15 24 上的 C# 产品短时保持关闭

状态：C# `Veil.App` / `Veil.Recovery` 在本机双物理屏、未装 MTT VDD 的条件下，短时只停内屏或只停外屏均有系统检查。只停内屏的 `release.json` 与热键恢复另有操作者口头确认。不是已发布产品，不能继承 Python 探针或 `app/` 的 10 分钟 / 循环 / 崩溃结论。  
日期：2026-09-18

第二组硬件上的**产品代码**复测。不得把 Python `display-probe` 或冻结 `app/` 的保持关闭写成 C# 已完成。本机禁止安装自带 MTT VDD。

## 环境

| 项 | 记录 |
| --- | --- |
| 计算机 | COLORFUL P15 24，接电 |
| 系统 | Windows 11，内部版本 26200 |
| 拓扑 | 扩展桌面：内屏 `DISPLAY\CMN1540` `\\.\DISPLAY2`；外接 S24Q6-Q24G8 `DISPLAY\PDA0238` `\\.\DISPLAY1`（主屏，Cursor 在这块） |
| 软件 | `src/` C# / WPF；APPLY 仅 `Veil.Recovery`；未启用自带 VDD；未调用 `install-vdd.ps1` |
| 会话 | `session-86e587e9`（release 只停内屏）；`session-7a25f486`（热键）；`session-d104ebda`（面板「恢复」）；`session-3ddfc434`（「恢复全部」只停内屏）；`session-bd7a5381`（只停外屏 +「恢复全部」） |

## 本轮结论

- **短时只停内屏**：面板点内置「保持关闭」。`hotkeyRegistered=true`，intent 仅内屏，`vddAssist=false`。关屏中 CCD `activeInternal=0`、`activeAuxiliary=1`、`gdiMonitorCount=1`。`applyRc=0`。
- **`release.json` 恢复**：`reason=release`，`restoreRc=0`，拓扑拉回双屏。操作者：内屏灭、外屏能用、恢复时黑屏闪一下。
- **热键恢复**：关屏中注入 `Ctrl+Alt+Shift+F10`（证明消息链，不是操作者亲手按键）。`result.json` `reason=hotkey`，`restoreRc=0`，`restoredTopology=true`。面板显示「已由 Ctrl+Alt+Shift+F10 恢复。」，两块「已显示（开启）」。操作者确认：内屏灭着、外屏能用、热键后闪一下恢复。
- **面板「恢复」**：只关内屏后点该行「恢复」（UIA `RestoreButton`，不是热键、不是手写 `release.json`）。intent 目标 `CMN1540` / `8388688`。关屏中 `activeInternal=0`、`activeAuxiliary=1`。`reason=release`，`restoreRc=0`。第一次用按钮 **Name**「恢复全部」找不到控件，已给 `RestoreAllButton` / `RestoreButton` 固定 AutomationId 后再测点得到。本轮为自动化点击，**没有**操作者口头确认画面，不得写成机旁通过。
- **面板「恢复全部」**：再关内屏后点底部 `RestoreAllButton`。intent 仅内屏。关屏中同样内 0 / 外 1。`reason=release`。同样是 UIA，不是口头机旁。
- **只关外屏（主屏）**：`KeepOffButton` 外接行；intent `PDA0238` / `8257`。关屏中 `activeInternal=1`、`activeAuxiliary=0`、`gdiMonitorCount=1`。`result.json` `adjustedOrigin=true`，`reason=release`，`restoreRc=0`，恢复后双屏活动。系统检查成立。外接是 Cursor 所在主屏，关屏会黑掉工作屏；**本轮没有操作者口头确认「外屏灭、内屏可用」**，不得把系统检查写成物理通过。
- **产品缺陷（已修）**：第一次恢复后 `Poll` 仍拿上一份 heartbeat，面板撒谎「已关闭」。热键这轮修复后的构建不再撒谎。关屏后按 Name 找不到「恢复全部」：已加 AutomationId。
- **「单屏恢复、其余仍关」**：代码从 `keepOff` 去掉一块再 APPLY。P15 双物理且不启 VDD 时不能同时关两块物理屏（会变成最后一条路径）。本轮**未测**并发多关；记「代码有、当前拓扑测不了」。

未做：10 分钟、循环、父进程崩溃、睡醒再关（见下）、REDMI / 安装器启用 VDD、操作者亲手按热键的预检、只关外屏的口头机旁。

## 睡醒单次再关（本机，未执行）

优先配置本应是 P15 只关内屏。本轮**没有**让机器进入睡眠：无人值守睡眠会中断会话，且无法做机旁观察。不得把 Python REDMI 醒后亮屏、未再关写成 C# 已测。

单元测试 `ExecutionGapRestoresThenReappliesOnce` 锁住 `HandleInterrupt`：先回放拓扑，意图仍在则 `TryApply` 一次（`_reapplyAttempted`），第二次间隙 `Finish`，禁止循环 APPLY。这只证明代码路径，**不是**睡醒机旁证据。

## 结果表

| 步骤 | 机制 | API / CCD | 物理观察 | 结论 |
| --- | --- | --- | --- | --- |
| 1 | 枚举 | 内屏 + 外屏各 1 条活动路径 | 两块点亮（先前轮次） | 第二物理目标存在 |
| 2 | 面板只关内置 | apply=0；关屏中内屏路径 0、外屏 1 | 操作者：内屏灭；外屏能用 | 短时保持关闭成立 |
| 3 | `release.json` 恢复全部 | restore=0，拓扑一致 | 操作者：黑屏闪一下后恢复 | 文件协议恢复成立 |
| 4 | 恢复后 UI（修复前） | 心跳仍写「已关闭」 | 面板文案撒谎 | 缺陷；已改 `Poll` |
| 5 | 注入热键恢复 | `reason=hotkey`，restore=0 | 操作者：内屏灭、外屏能用、热键后闪一下恢复 | 热键消息链 + 机旁画面成立；非亲手按键预检 |
| 6 | 面板行内「恢复」 | `session-d104ebda`，`reason=release`，关屏中内 0 外 1 | 未口头确认 | 系统检查通过；不是机旁通过 |
| 7 | 面板「恢复全部」 | `session-3ddfc434`，`reason=release` | 未口头确认 | 系统检查通过；AutomationId 可点 |
| 8 | 只关外屏 + 原点 | `session-bd7a5381`，关屏中内 1 外 0，`adjustedOrigin=true` | 未口头确认 | 系统检查通过；物理未点头 |
| 9 | 睡醒再关 | 未跑 | 未做 | 未执行 |

## 证据

原始文件在本机主仓库 `.git/veil-validation-20260918-csharp-p15/`，不随 Git 分发。误拷的旧 Python `run-95c66af9` 不是本次证据。

| 文件 | SHA-256 |
| --- | --- |
| `before/enumerate.txt` | `753baeddb51e44d0757f6926d6a8261ffec9017fb6bfae35fcc2972754cbe9a6` |
| `run-internal-short/enumerate-during.txt` | `d59021d97c5f88b8706f0b1cadd1f26a19ca590cf1d13c33af8575c35f225b66` |
| `run-internal-short/enumerate-after.txt` | `753baeddb51e44d0757f6926d6a8261ffec9017fb6bfae35fcc2972754cbe9a6` |
| `run-internal-short/ui-during.txt` | `4236009c323386d2297fdc7bffec0fc586d600505189f4a64e56bd4f78a977e1` |
| `run-internal-short/after/session-86e587e9/result.json` | `c1b20411079660125cbe6196f1ecb045f91ef4db161a90f6220cdbfac8913774` |
| `run-internal-hotkey/enumerate-during.txt` | `d59021d97c5f88b8706f0b1cadd1f26a19ca590cf1d13c33af8575c35f225b66` |
| `run-internal-hotkey/enumerate-after.txt` | `753baeddb51e44d0757f6926d6a8261ffec9017fb6bfae35fcc2972754cbe9a6` |
| `run-internal-hotkey/ui-after.txt` | `ed59c6afc5755f8fa1d238067d606e9cc7a9620f831cc727e4c5e3df8a0743f6` |
| `run-internal-hotkey/session-copy/session-7a25f486/ready.json` | `35e02738a0ee08dc5b0b73601ebc6eebd641431a7c9ba5efd082d1710034213a` |
| `run-internal-hotkey/session-copy/session-7a25f486/result.json` | `c6355ddd6f99a86fa21254523ef12c2bebd178af2a69bd2c095c5c77d55705ff` |
| `run-internal-restore-button/enumerate-during2.txt` | `d59021d97c5f88b8706f0b1cadd1f26a19ca590cf1d13c33af8575c35f225b66` |
| `run-internal-restore-button/enumerate-after2.txt` | `753baeddb51e44d0757f6926d6a8261ffec9017fb6bfae35fcc2972754cbe9a6` |
| `run-internal-restore-button/session-during/intent.json` | `e560414d674dc9ef920a7df19622f5446369c7c1c558cbbde37ef89a95b48a96` |
| `run-internal-restore-button/session-after/result.json` | `c1b20411079660125cbe6196f1ecb045f91ef4db161a90f6220cdbfac8913774` |
| `run-internal-restore-all/enumerate-during.txt` | `d59021d97c5f88b8706f0b1cadd1f26a19ca590cf1d13c33af8575c35f225b66` |
| `run-internal-restore-all/session-during/intent.json` | `e560414d674dc9ef920a7df19622f5446369c7c1c558cbbde37ef89a95b48a96` |
| `run-internal-restore-all/session-after/result.json` | `c1b20411079660125cbe6196f1ecb045f91ef4db161a90f6220cdbfac8913774` |
| `run-external-restore-all/enumerate-during.txt` | `f3f72251da533b6d94920dbf0837643b4eea4c03ec13b5a5d3c35e34d9ac36d0` |
| `run-external-restore-all/session-during/intent.json` | `cb4f5b846e4f540715b054dc1ae0d0506e25c6d165b230dd6a79a9d89a993a21` |
| `run-external-restore-all/session-after/result.json` | `8f22370693c1d5cdc40ee60f30bffaa500ebbcf4859ab55bd669d6b2d4cb587d` |
