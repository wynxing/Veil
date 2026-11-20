# Veil {{INFORMATIONAL}}（{{CHANNEL}}）

Veil 是 Windows 屏幕保持关闭工具：按物理屏决定关或开，直到主动恢复。本次发布范围为 **Windows 11 x64**。

## 本版本

- **小面板与安装器体验升级**：面板重构为三段式固定布局，顶栏常驻 `紧急恢复 Ctrl+Alt+Shift+F10` 按键胶囊与热键状态徽章，适配 Windows 浅色／深色系统主题并提升警告色对比度；底部常驻「恢复全部」与「退出」，折叠区新增「打开日志」「会话目录」「复制诊断摘要（脱敏）」入口；托盘悬浮提示补充紧急热键。WiX 安装向导补齐 `VeilTheme.zh-CN.wxl` 全中文界面。
- **后台降耗与运行可靠性优化**：面板退回托盘且无活跃关屏会话时停止 400ms 周期 CCD／PnP／磁盘轮询；修复再关路径激活未活动辅助虚拟输出时硬编码 `1920×1200` 分辨率的问题；开机自启改用原生注册表项 `HKCU\Software\Microsoft\Windows\CurrentVersion\Run\Veil`（自动清理旧版 `Startup\Veil.lnk`）；统一 App 与驱动助手日志为 UTC RFC3339 时间戳并按 2 MiB 轮转，应用启动时自动清理较早的已结束会话目录（保留最近 10 份）。
- **工程与文档治理**：枚举化屏幕领域状态（保持协议 v2 兼容）、拆分核心大文件、CI 接入 `rustfmt` 与 `clippy -D warnings` 零告警门禁，新增 `CHANGELOG.md`。
- **验证范围**：本版本通过全工作区 `fmt`、`clippy -D warnings`、发布预检、219 项 locked release 单元测试与 WiX MSI／Burn 0 警告构建；用户已实机试用新版面板界面并反馈完善热键常驻提示。多屏组合、睡眠唤醒、开机重启自启及安装／升级／卸载尚未提供完整机旁逐项验收记录。

详细变更见 [变更日志](https://github.com/wynxing/Veil/blob/main/CHANGELOG.md)，支持与验证范围见 [支持与诊断](https://github.com/wynxing/Veil/blob/main/doc/validation/support-and-diagnostics.md)。

## 下载与安装

- 安装包：`{{SETUP_FILE}}`。下载后核对本 Release 的 `SHA256SUMS.txt`。
- 安装包没有 Authenticode 签名，Windows 可能显示 SmartScreen 或「未知发布者」提示。
- MTT 是已签名的第三方显示驱动，用于关光全部物理屏时留下活动路径。安装时可取消设备安装，驱动文件仍随应用保留，之后可从面板安装。
- Windows 10 与 ARM 不在本版支持范围内。应用不需要预装 .NET。
