# 验证结果：COLORFUL P15 24 上的 C# 产品短时只停内屏

状态：C# `Veil.App` / `Veil.Recovery` 短时只停内屏 + `release.json` 恢复，系统检查与机旁观察已记录。不是已发布产品，不能继承 Python 探针或 `app/` 的 10 分钟 / 循环 / 崩溃结论。  
日期：2026-09-18

第二组硬件上的**产品代码**复测。不得把 Python `display-probe` 或冻结 `app/` 的保持关闭写成 C# 已完成。

## 环境

| 项 | 记录 |
| --- | --- |
| 计算机 | COLORFUL P15 24，接电 |
| 系统 | Windows 11，内部版本 26200 |
| 拓扑 | 扩展桌面：内屏 `DISPLAY\CMN1540` `\\.\DISPLAY2`；外接 S24Q6-Q24G8 `DISPLAY\PDA0238` `\\.\DISPLAY1`（主屏） |
| 软件 | `src/` C# / WPF；APPLY 仅 `Veil.Recovery`；未启用自带 VDD；未调用 `install-vdd.ps1` |
| 会话 | `%LOCALAPPDATA%\Veil\session-86e587e9` |

## 本轮结论

- **短时只停内屏**：面板点内置「保持关闭」。`ready.json` `hotkeyRegistered=true`，intent 仅内屏，`vddAssist=false`。关屏中 CCD `activeInternal=0`、`activeAuxiliary=1`、`gdiMonitorCount=1`。`applyRc=0`。
- **恢复**：自动化未点到「恢复全部」（关屏后 UI 树变化），改为写 `release.json`。`result.json` `ok=true`，`reason=release`，`restoreRc=0`，`restoredTopology=true`，`adjustedClone=false`。恢复后 enumerate 与关屏前一致（两条活动路径）。
- **机旁观察（操作者）**：内屏是灭的；外屏能用；恢复时黑屏闪了一下。
- **产品缺陷（已修代码，见同日修复分支）**：恢复后面板仍显示「已关闭（保持关闭）」，因为 `Poll` 保留上一份 heartbeat。不得把该残留 UI 当成仍在关屏。

未做：C# 热键恢复、面板「恢复」按钮恢复、只停外屏、单屏恢复其余仍关、10 分钟、循环、父进程崩溃、睡醒再关、REDMI / 安装器 VDD。

## 结果表

| 步骤 | 机制 | API 结果 | 物理观察 | 结论 |
| --- | --- | --- | --- | --- |
| 1 | 枚举 | 内屏 + 外屏各 1 条活动路径 | 两块点亮 | 第二物理目标存在 |
| 2 | 面板只关内置 | apply=0；关屏中内屏路径 0、外屏 1 | 操作者：内屏灭；外屏能用 | 短时保持关闭成立；切换允许闪 |
| 3 | `release.json` 恢复全部 | restore=0，拓扑一致 | 操作者：黑屏闪一下后恢复 | 文件协议恢复成立；热键与按钮恢复未测 |
| 4 | 恢复后 UI | 心跳仍写「已关闭」 | 面板文案撒谎 | 缺陷；已改 `Poll` 在 `result.json` 后丢弃 heartbeat |

## 证据

原始文件在本机主仓库 `.git/veil-validation-20260918-csharp-p15/`，不随 Git 分发。误拷的旧 Python `run-95c66af9` 不是本次证据。

| 文件 | SHA-256 |
| --- | --- |
| `before/enumerate.txt` | `753baeddb51e44d0757f6926d6a8261ffec9017fb6bfae35fcc2972754cbe9a6` |
| `run-internal-short/enumerate-during.txt` | `d59021d97c5f88b8706f0b1cadd1f26a19ca590cf1d13c33af8575c35f225b66` |
| `run-internal-short/enumerate-after.txt` | `753baeddb51e44d0757f6926d6a8261ffec9017fb6bfae35fcc2972754cbe9a6` |
| `run-internal-short/ui-during.txt` | `4236009c323386d2297fdc7bffec0fc586d600505189f4a64e56bd4f78a977e1` |
| `run-internal-short/after/session-86e587e9/ready.json` | `dcd0c06d837b94e6d3700505f17a224cfe72422b58b465114459d343489abba9` |
| `run-internal-short/after/session-86e587e9/intent.json` | `e560414d674dc9ef920a7df19622f5446369c7c1c558cbbde37ef89a95b48a96` |
| `run-internal-short/after/session-86e587e9/heartbeat.json` | `c8e90867222736dce61ea1f4bc6b1941352acc8178a78f4b9a4cbdb2e786060d` |
| `run-internal-short/after/session-86e587e9/result.json` | `c1b20411079660125cbe6196f1ecb045f91ef4db161a90f6220cdbfac8913774` |
