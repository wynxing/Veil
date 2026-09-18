# 验证结果：COLORFUL P15 24 上的 C# 产品短时只停内屏

状态：C# `Veil.App` / `Veil.Recovery` 短时只停内屏，`release.json` 与热键恢复均有系统检查和机旁观察。不是已发布产品，不能继承 Python 探针或 `app/` 的 10 分钟 / 循环 / 崩溃结论。  
日期：2026-09-18

第二组硬件上的**产品代码**复测。不得把 Python `display-probe` 或冻结 `app/` 的保持关闭写成 C# 已完成。

## 环境

| 项 | 记录 |
| --- | --- |
| 计算机 | COLORFUL P15 24，接电 |
| 系统 | Windows 11，内部版本 26200 |
| 拓扑 | 扩展桌面：内屏 `DISPLAY\CMN1540` `\\.\DISPLAY2`；外接 S24Q6-Q24G8 `DISPLAY\PDA0238` `\\.\DISPLAY1`（主屏） |
| 软件 | `src/` C# / WPF；APPLY 仅 `Veil.Recovery`；未启用自带 VDD；未调用 `install-vdd.ps1` |
| 会话 | `session-86e587e9`（release）；`session-7a25f486`（热键） |

## 本轮结论

- **短时只停内屏**：面板点内置「保持关闭」。`hotkeyRegistered=true`，intent 仅内屏，`vddAssist=false`。关屏中 CCD `activeInternal=0`、`activeAuxiliary=1`、`gdiMonitorCount=1`。`applyRc=0`。
- **`release.json` 恢复**：`reason=release`，`restoreRc=0`，拓扑拉回双屏。操作者：内屏灭、外屏能用、恢复时黑屏闪一下。
- **热键恢复**：关屏中注入 `Ctrl+Alt+Shift+F10`（证明消息链，不是操作者亲手按键）。`result.json` `reason=hotkey`，`restoreRc=0`，`restoredTopology=true`。面板显示「已由 Ctrl+Alt+Shift+F10 恢复。」，两块「已显示（开启）」。操作者确认：内屏灭着、外屏能用、热键后闪一下恢复。
- **产品缺陷（已修）**：第一次恢复后 `Poll` 仍拿上一份 heartbeat，面板撒谎「已关闭」。热键这轮修复后的构建不再撒谎。

未做：面板「恢复」/「恢复全部」按钮、只停外屏、单屏恢复其余仍关、10 分钟、循环、父进程崩溃、睡醒再关、REDMI / 安装器 VDD。操作者亲手按热键的预检未单独做。

## 结果表

| 步骤 | 机制 | API 结果 | 物理观察 | 结论 |
| --- | --- | --- | --- | --- |
| 1 | 枚举 | 内屏 + 外屏各 1 条活动路径 | 两块点亮 | 第二物理目标存在 |
| 2 | 面板只关内置 | apply=0；关屏中内屏路径 0、外屏 1 | 操作者：内屏灭；外屏能用 | 短时保持关闭成立 |
| 3 | `release.json` 恢复全部 | restore=0，拓扑一致 | 操作者：黑屏闪一下后恢复 | 文件协议恢复成立 |
| 4 | 恢复后 UI（修复前） | 心跳仍写「已关闭」 | 面板文案撒谎 | 缺陷；已改 `Poll` |
| 5 | 注入热键恢复 | `reason=hotkey`，restore=0；面板「已由 Ctrl+Alt+Shift+F10 恢复。」 | 操作者：内屏灭、外屏能用、热键后闪一下恢复 | 热键消息链 + 机旁画面成立；非亲手按键预检 |

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
