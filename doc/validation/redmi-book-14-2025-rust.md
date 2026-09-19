# 验证结果：REDMI Book 14 2025 上的 Rust 短时只停内屏

状态：本机沿用已装禁用态自带 VDD（`ROOT\DISPLAY\0000` Code 22），用工作树 Rust Debug 三个 exe 跑产品协调器路径。**第一次**短时只停内屏有系统检查，操作者当时没看盖。**第二次** `release.json` 恢复：系统检查成立，操作者确认**内屏灭了大约十几秒，然后亮回来**。**第三次**约 6 秒后热键恢复：`reason=hotkey`，操作者确认**灭着，按热键后亮回来**。不是 Rust MSI 重装，不是 egui 面板点选，不是长时 / 循环 / 睡醒。不可公开安装。不得把 Python [redmi-book-14-2025-vdd.md](redmi-book-14-2025-vdd.md) 写成 Rust 已过。  
日期：2026-09-19

## 环境

| 项 | 记录 |
| --- | --- |
| 计算机 | XIAOMI REDMI Book 14 2025(FHD+)，接电（BatteryStatus=2，电量 100%） |
| 系统 | Windows 11 家庭中文版，内部版本 26200 |
| 拓扑 | 不接外屏。基线仅内屏 `DISPLAY\TMA0813` `targetId=8388688`，`activeInternal=1` `activeAuxiliary=0` |
| 软件 | 工作树 `product/rust-v1` Debug：`Veil.App.exe` / `Veil.Recovery.exe` / `Veil.DriverHelper.exe`。APPLY 仅 Recovery |
| 入口 | `Veil.App --validate-keep-off-internal --seconds 15`（自动确认启用 VDD，不点 egui） |
| VDD | 未重打 Rust 安装包。沿用本机 `%ProgramFiles%\Veil` 已装禁用态 `Root\MttVDD` |
| 会话 | 第一次 `session-5ed435d3`（只系统检查）；第二次 `session-bd16c162`（release + 口头）；第三次 `session-ca4f3e68`（热键 + 口头） |
| 证据 | 本机主仓库 `.git/veil-validation-20260919-rust-redmi/`，不随 Git 分发 |

原生三组 VALIDATE 停内屏仍为 87、`remainingActive=0`（`out/rust-redmi-diagnose.jsonl`）。没有第二活动目标时产品不得 APPLY。

## 短时闭环（第二次，口头入档）

1. 跑前枚举：内 1 / 辅 0。VDD Code 22。  
2. 提权启动校验进程。协调器 enable 自带 VDD 后出现活动 `VDD by MTT` / `DISPLAY\MTT1337` / `ROOT#DISPLAY#0000`。  
3. `ready.json`：`pid=17260`，`hotkeyRegistered=true`。`arm.json` 对上。`intent.json`：只关内屏，`vddAssist=true`。  
4. `events.jsonl`：`applied` `applyRc=0`（12:09:33Z），约 14.4 秒后 `finish` `reason=release`。  
5. 关屏中 `enumerate-1` 至 `enumerate-13` 均为 `activeInternal=0`、`activeAuxiliary=1`。`enumerate-0` 为 enable 后、APPLY 前（内 1 / 辅 1）。`adjustedClone=false`。  
6. 操作者口头：内屏灭了大约十几秒，然后亮回来。  
7. `result.json`：`ok=true`，`reason=release`，`restoreRc=0`，`restoredTopology=true`。  
8. 跑后枚举与跑前同一哈希；VDD 再次 Code 22。

第一次 `session-5ed435d3` 的 CCD / result 与第二次同类，但当时操作者没看盖，只作文中对照，不单独当口头验收。

## 热键恢复（第三次）

`short-201115` / `session-ca4f3e68`。`--wait-hotkey`，25 秒窗口。

- `ready.json`：`pid=12464`，热键已注册；`intent` 只关内屏，`vddAssist=true`
- `applied` applyRc=0（12:11:24Z）；`finish` `reason=hotkey`（12:11:31Z，约 6.3 秒）
- `enumerate-1` 至 `enumerate-6`：内 0 / 辅 1；`enumerate-7` 与跑后：内 1 / 辅 0
- `result.json`：`ok=true`，`restoreRc=0`，`restoredTopology=true`，`adjustedClone=false`
- 跑后 VDD Code 22
- 操作者口头：灭着，按 `Ctrl+Alt+Shift+F10` 后亮回来

这是机旁热键，不是注入按键。

## 结果表

| 步骤 | 机制 | API / CCD | 物理观察 | 结论 |
| --- | --- | --- | --- | --- |
| 1 | 沿用已装禁用 VDD | Code 22，未重打 MSI | 无 | 安装器路径的禁用态仍在；不是 Rust 安装器验收 |
| 2 | 协调器 enable | 出现活动 `Root\MttVDD` | 未单独记闪断 | 启用有系统检查；自动确认，不是面板点选 |
| 3 | 只停内屏 | apply=0；采样 1–13 内 0 / 辅 1 | 操作者：内屏灭了约十几秒 | 短时保持关闭：系统检查 + 口头成立 |
| 4 | `release.json` 恢复 | restore=0，拓扑回到基线 | 操作者：随后亮回来 | 文件协议恢复成立 |
| 5 | 热键恢复 | `reason=hotkey` restore=0 | 操作者：灭着，按热键后亮 | 短时热键恢复成立 |
| 6 | 恢复后 disable | 再次 Code 22 | 未口头确认虚拟屏消失 | 系统检查：未留下活动自带 VDD |

## 未做

- Rust MSI / Burn 重装；`INSTALLVDD=0`  
- egui 面板点「保持关闭」  
- 10 分钟连续采样；20 次循环  
- 睡醒再关；父进程崩溃  
- 恢复闪屏单独口头  
- 代码签名、可公开安装  

P15 上的 Rust 机旁观察仍未执行。

## 证据

原始文件在 `.git/veil-validation-20260919-rust-redmi/`。拓扑原始字节不进 Git。

第二次（口头入档）`short-200924`：

| 文件 | SHA-256 |
| --- | --- |
| `before/enumerate.txt` | `BFA179380F28EDAE11E5B0944A47AFD2072A12B8ACFEA62C6C39FB5195C3694C` |
| `during/enumerate-1.txt` | `7D68982CC4761FFF2AFD1FB6B263ECD7B07D0976F5CFCC1353BC8630C40D898D` |
| `during/enumerate-13.txt` | `7D68982CC4761FFF2AFD1FB6B263ECD7B07D0976F5CFCC1353BC8630C40D898D` |
| `after/enumerate.txt` | `BFA179380F28EDAE11E5B0944A47AFD2072A12B8ACFEA62C6C39FB5195C3694C` |
| `session-bd16c162/result.json` | `4027475D858088B652AB09A84849081A7641C9A5EBFE13244D6036EC790048EF` |
| `session-bd16c162/intent.json` | `37A5B31B8C9A6B4DDCA459EFE3EE0962C0EA567CFBE52ACFB247F7CEEACA1244` |
| `session-bd16c162/ready.json` | `E0E57C08B5937D5243517DF39E00127FD649B895614554F74789602B936E114E` |
| `session-bd16c162/events.jsonl` | `BB163EA490C106C5D0F5C7EE06E4AA654A923AB67BB952F01AD3CC981A5757BB` |
| `validate.log` | `59C462754B1EA40247D411F40DB3A7B462597852BEAC43E75575C1F37FE0E1D2` |

第三次（热键）`short-201115`：

| 文件 | SHA-256 |
| --- | --- |
| `before/enumerate.txt` | `BFA179380F28EDAE11E5B0944A47AFD2072A12B8ACFEA62C6C39FB5195C3694C` |
| `during/enumerate-1.txt` | `FEE655A3655ECAC8649B8C252C35B59D54E740B15755359F90EE783873D0765C` |
| `during/enumerate-6.txt` | `FEE655A3655ECAC8649B8C252C35B59D54E740B15755359F90EE783873D0765C` |
| `after/enumerate.txt` | `BFA179380F28EDAE11E5B0944A47AFD2072A12B8ACFEA62C6C39FB5195C3694C` |
| `session-ca4f3e68/result.json` | `CF8AFE5F2974EAAF5A2FB196962E46B7DECAA8429D5D729A53872CBB2D68EC0A` |
| `session-ca4f3e68/events.jsonl` | `3DEAE1AF922FCD6F73B4E9D19EC9DBD8EC5F35B34377CCD8A694FC1F5C48A234` |
| `validate.log` | `BB2AE2EB1D7C46ED9EE02E3DC58EF2D7418F202F0907D2B0ADF6E2A96AF69081` |

第一次（只系统检查）`short-195340` / `session-5ed435d3` 另存同目录。
