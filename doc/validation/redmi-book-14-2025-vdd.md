# REDMI Book 14 2025：虚拟屏辅助保持关闭

日期：2026-09-18；状态：短时闭环、父进程崩溃恢复、10 分钟、20 次循环、虚拟目标消失、当时最小应用（已不在仓库）两次关屏恢复，以及关屏中睡眠唤醒已记录。唤醒后内屏亮起，不自动再关；文案修正后操作者确认界面正常。**不能写成不依赖虚拟屏。** 当前产品在 `src/`，见 [REDMI Rust](redmi-book-14-2025-rust.md)。

本机原始证据在主仓库 `.git/veil-validation-20260917/`（安装与首次关屏）和 `.git/veil-validation-20260918/`（今日预检、崩溃恢复、20 次循环与虚拟目标消失），不随 Git 分发。仓库只记录可复核摘要。这与「系统设置 → 仅第二屏幕」不是同一机制：本轮使用 CCD 只清除内屏 `DISPLAYCONFIG_PATH_ACTIVE`，虚拟目标保持活动，并由独立恢复进程回放关屏前拓扑。

## 环境

| 项 | 记录 |
| --- | --- |
| 计算机 | XIAOMI REDMI Book 14 2025(FHD+) |
| 系统 | Windows 11 家庭中文版，64 位，内部版本 26200 |
| GPU | Intel UHD Graphics，驱动 32.0.101.6733（`oem17.inf`，Microsoft WHCP） |
| 物理显示器 | 内置面板 TM140VDXP02（`DISPLAY\TMA0813`），1920×1200 |
| 虚拟驱动 | Virtual Display Driver 25.7.23 / INF `12/24/2024,11.30.4.434`，发布名 `oem58.inf`，实例 `ROOT\DISPLAY\0000` |
| 虚拟监视器 | Generic Monitor (VDD by MTT)，`DISPLAY\MTT1337` |
| 安装工具 | NefCon v1.20.0 x64 `nefconc.exe` |
| 配置 | `C:\VirtualDisplayDriver\vdd_settings.xml`，1 块虚拟屏，1920×1200@60 |
| 外接屏 | 无 |

签名：`mttvdd.cat` / `MttVDD.dll` 的 Authenticode 为 `Valid`，签名人 SignPath Foundation，指纹 `3CF8CF26D8BA266C3A483AB7D26D4A818E317D76`。PnPUtil 显示 Signer Name 为 SignPath Foundation。WMI `Win32_PnPSignedDriver.IsSigned` 对该实例返回 `false`。这不是 WHQL，也不是本机兼容性认证。

卸载线索（尚未执行卸载）：设备管理器卸载 Virtual Display Driver 并删除驱动包；或定向移除 `oem58.inf` / `ROOT\DISPLAY\0000`。不得删除 Intel 显示驱动。

## 与「仅第二屏幕」的区别

此前人工用虚拟屏把 Windows 设成「仅第二屏幕」后，过一段时间会退回主屏。本轮不调用该投影模式。

对照点：关屏期间 CCD 采样必须看到 `activeInternal=0` 且 `activeAuxiliary=1`；到期或快捷键后必须回到两条活动路径。若系统自行把内屏抢回来，应记失败，而不是再包一层投影设置。扩展桌面（GDI 两块独立桌面）下只停内屏 VALIDATE 为 87；克隆/共用源（GDI `gdiMonitorCount=1`、两监视器挂在同一 `DISPLAY1`）下 VALIDATE 为 0。保持关闭必须落在后一种拓扑。不得使用 `SDC_TOPOLOGY_EXTERNAL`（仅第二屏幕）。

## 安装后不关屏复核（2026-09-18）

当前仍能枚举内屏 + `VDD by MTT`。`validation.py diagnose` 在 basic / virtual / refresh 三组上，原样拓扑与「只停内屏」VALIDATE 均为 0（`post-vdd-diagnostic.jsonl`）。这只说明现在有第二活动目标后参数组合可接受，不等于保持关闭已通过。

## 已执行步骤

安装脚本为工作区 `tools/display-probe/install-vdd.ps1`，证据目录 `vdd-install-state.json`：安装成功，TrustedPublisher 新增上述指纹，未改 Secure Boot / 测试签名。安装后枚举 `activeInternal=1`、`activeAuxiliary=1`。安装后电源事件采集未看到 Kernel-Power 进出待机记录。

机旁预检（亮屏，不关屏）：定时 2 秒恢复与实际 `Ctrl+Alt+Shift+F10` 均 `ok=true`，凭证 `vdd-preflight.json`。

| 运行 | 目录 | 计划 | 系统检查 | 关屏期间采样 | 后台哈希 | 机旁观察 |
| --- | --- | --- | --- | --- | --- | --- |
| 15 秒 + 合成输入 + 快捷键恢复 | `veil-validation-20260917/run-d1fed47b` | apply=0，13.4 秒后 `reason=hotkey`，restore=0，拓扑一致 | 通过 | 14 次 `state`：`activeInternal` 仅 0，`activeAuxiliary` 仅 1 | 16 个样本，计数递增，最大间隔 1.01 秒 | 已记录：内屏熄灭，普通输入不唤醒，快捷键恢复正常；背光/闪烁/布局未单独说明 |
| 15 秒 + 合成输入 + 定时恢复 | `veil-validation-20260917/run-21d69658` | apply=0，15.0 秒后 `reason=timer`，restore=0，拓扑一致 | 通过 | 15 次 `state`：内屏仅 0，辅助仅 1 | 17 个样本，递增，最大间隔 1.01 秒 | 未单独填写；不得用系统检查代替 |
| 600 秒 + 合成输入 + 定时恢复 | `veil-validation-20260917/run-6ff40cdf` | apply=0，600.0 秒后 `reason=timer`，restore=0，拓扑一致 | 通过 | 593 次 `state`：内屏仅 0，辅助仅 1 | 598 个样本，递增至 5 980 000，最大间隔 1.02 秒 | 未单独填写 |
| 15 秒关屏 + 父进程 `os._exit(17)` | `veil-validation-20260918/run-5d0a6ed3` | 父进程在 worker arm 后立即退出；worker 仍 restore=0，拓扑一致 | 通过 | 7 次 `state`：内屏仅 0，辅助仅 1 | 9 个样本，递增，最大间隔 1.01 秒 | 操作者 6.5 秒后按快捷键恢复（`reason=hotkey`）；证明恢复进程不依赖父进程存活 |
| 20×15 秒循环 | `veil-validation-20260918/cycles-20` | 20/20 `reason=timer`，apply/restore=0，拓扑一致；`summary.json` | 通过 | 每次关屏期间内屏采样均为 0、辅助均为 1 | 每次进度递增，组内最大间隔 1.02 秒 | 操作者在场并允许继续；未逐次填写背光/闪烁/布局 |
| 关屏中禁用 `ROOT\DISPLAY\0000` | `veil-validation-20260918/vdd-disappear/run-5d6c31b3` | apply=0，约 3.1 秒后 `reason=unexpected-topology`；保存拓扑 restore=87，`fallbackRc=0` | 按协议：优先恢复内屏，不保持关闭 | 关屏采样内屏 0 / 辅助 1；禁用后变为内屏 1 / 辅助 0；采样未见零活动路径 | 进度递增，间隔约 1.02 秒 | 操作者允许 UAC；未单独填写背光/闪烁 |
| 最小应用关屏（两次） | `%LOCALAPPDATA%\Veil\run-e6ebf237`、`run-b0900840` | hold `--seconds 0`，apply=0，约 3.6s / 3.1s 后 `reason=hotkey`，restore=0，拓扑一致 | 通过 | 关屏采样内屏仅 0、辅助仅 1；克隆拓扑 `gdiMonitorCount=1` | 未跑进度哈希 | 操作者确认可以；热键恢复。此前扩展桌面校验 87、会话目录 183、错误路径 `SDC_TOPOLOGY_INTERNAL` 曾拆掉虚拟屏，已修 |
| 关屏中睡眠唤醒 | `%LOCALAPPDATA%\Veil\run-4de6c611`；证据 `veil-validation-20260918/sleep-wake/` | hold `--seconds 0`，apply=0，约 11.9s 后 `reason=execution-gap`，restore=0，拓扑一致；`ok=false` | 通过：保持关闭结束并回放拓扑 | 关屏采样内屏仅 0、辅助仅 1；最后一次采样与恢复之间 monotonic 间隔约 10.9s | 未跑进度哈希 | 操作者确认唤醒后屏幕正常亮。Kernel-Power 506（06:43:54Z 进入低功率）与 507（06:44:05Z 退出），Event 172 为 Adaptive Connected Standby。应用当时显示未知 |
| 睡眠唤醒界面复测 | `%LOCALAPPDATA%\Veil\run-2ff8eb14` | hold `--seconds 0`，apply=0，约 13.7s 后 `reason=execution-gap`，restore=0，拓扑一致；`ok=false` | 通过：保持关闭结束并回放拓扑 | 关屏采样内屏仅 0、辅助仅 1；约 3.1s 采样后至 13.7s 恢复 | 未跑进度哈希 | 操作者确认测试没问题；内屏已活动时界面不再显示未知 |

三次长证据运行与 20 次循环均注入了鼠标移动与 Shift。合成输入不是物理键鼠验收；但采样显示注入后内屏路径仍保持停用。600 秒运行的墙钟跨度与 monotonic 一致，未见看门狗被待机拉长。20 次循环总墙钟约 456 秒。

虚拟目标消失（`Disable-PnpDevice`，不是卸载驱动包）：关屏约 1 秒后发出禁用；`CM_PROB_DISABLED` 约 2 秒。CCD 在 worker 回放保存拓扑之前已变成仅内屏。回放双路径返回 87（此时虚拟适配器不可用）。`SDC_TOPOLOGY_INTERNAL` 返回 0。随后 `Enable-PnpDevice` 成功，虚拟目标再次活动，但适配器 LUID 从 `12cfa0d9` 变为 `1ed5e2a9`，GDI 源名从 `DISPLAY21` 变为 `DISPLAY22`，因此旧拓扑文件再次 APPLY 仍为 87。当前枚举已是 `activeInternal=1`、`activeAuxiliary=1`。这证明辅助目标消失后**不能**维持仅内屏保持关闭，安全路径是点亮内屏。

## 未执行 / 证据不足

| 项 | 状态 |
| --- | --- |
| 虚拟目标消失后的内屏命运 | 已测：关屏期间禁用 VDD 后保持关闭结束，内屏被点亮；保存的双路径拓扑无法回放（87）。不承诺自动重建保持关闭 |
| 睡眠唤醒 | 已测两次：关屏中进入现代待机后内屏亮起，worker 因执行间隙结束保持关闭并回放拓扑，未自动再关。第一次界面显示未知；文案修正后第二次操作者确认没问题。唤醒闪屏未单独记录 |
| 10 分钟机旁画面/背光连续观察 | 证据不足 |
| 20 次循环的逐次物理观察表 | 未填；仅有操作者在场 |
| 纯原生仅内屏保持关闭 | 仍不支持，见[第二轮复核](redmi-book-14-2025-revalidation.md) |

## 当前结论

- **纯原生仅内屏保持关闭**：不支持（历史结果，本轮未推翻）。
- **仅物理内屏 + 已签名 VDD 第二目标**：在本机、本驱动版本上，15 秒系统检查与一次机旁观察通过；父进程崩溃后独立恢复进程仍能把拓扑拉回；10 分钟与 20×15 秒循环的系统检查通过，关屏期间采样均为 `activeInternal=0`。关屏期间禁用虚拟适配器后，保持关闭结束，内屏被点亮，不能靠已保存的双路径拓扑维持关闭。这是第一组硬件选定的保持关闭路线，不是全 Windows 默认方案。
- **不是**「仅第二屏幕」投影模式的封装；若 VDD 仍在时系统自行把内屏抢回来，应记失败。
- 虚拟屏单独枚举，不计入物理屏支持范围。关屏中睡眠唤醒后不得宣称仍保持关闭；本机一次实测为内屏亮起且未自动再关。
- 当时的最小应用（已不在仓库）按该路线做过两次短时保持关闭并用热键恢复；关屏中睡眠后画面正常亮起。第一次界面显示未知，文案修正后复测通过。扩展桌面与会话目录冲突导致的失败已记录并修复。当前产品在 `src/`，不是本页探针，也不是安装包。
