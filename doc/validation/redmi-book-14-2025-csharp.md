# 验证结果：REDMI Book 14 2025 上的 C# 安装器 VDD 短时闭环

状态：本机已卸实验室 MTT，用无签名 MSI（`INSTALLVDD=1`）装上禁用态自带 VDD。面板确认后 `DriverHelper enable`，短时只停内屏有系统检查；操作者确认关屏期间**内屏灭了**。`release.json` 恢复后设备回到 `CM_PROB_DISABLED`。恢复闪屏、辅助输出是否可见未口头确认。不是已发布、不可公开安装。不得把 Python [redmi-book-14-2025-vdd.md](redmi-book-14-2025-vdd.md) 写成 C# 已过。  
日期：2026-09-19

## 环境

| 项 | 记录 |
| --- | --- |
| 计算机 | XIAOMI REDMI Book 14 2025(FHD+)，接电 |
| 系统 | Windows 11 家庭中文版，内部版本 26200 |
| 拓扑 | 不接外屏。装前仅内屏 `DISPLAY\TMA0813` `targetId=8388688` |
| 软件 | 工作树 `verify/csharp-redmi-vdd` 打出的 `Veil.msi` / `VeilSetup.exe`；APPLY 仅 `Veil.Recovery` |
| 安装方式 | `msiexec /i Veil.msi INSTALLVDD=1 /qn`。Burn 无单独驱动同意页；本轮未走交互 Burn UI |
| 会话 | `session-93d37ce1` |
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
| 7 | 亲手热键 / 10 分钟 / 循环 / 崩溃 / 睡醒 | 未跑 | 未做 | 未执行 |

## 未做

- 操作者口头确认辅助输出是否可见、恢复是否闪一下  
- 亲手按 `Ctrl+Alt+Shift+F10` 预检  
- Burn 交互同意页；`INSTALLVDD=0` 只装应用  
- C# 10 分钟、20 次循环、父进程崩溃  
- 睡醒再关  
- 代码签名、可公开安装

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
