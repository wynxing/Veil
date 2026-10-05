# Veil {{INFORMATIONAL}}（{{CHANNEL}}）

Veil 是 Windows 屏幕保持关闭工具：按物理屏决定关或开，直到主动恢复。本次发布范围为 **Windows 11 x64**。

## 本版本

- 无版本后缀使用正式发布渠道，有后缀使用预览渠道。
- 整理产品、架构与发布文档，将支持范围和故障排查集中到一份说明。
- 正式版更新提示只选择正式版本；预览版也能发现后续正式版本。

核心关屏与恢复已由用户实际使用；睡眠唤醒、安装升级及具体多屏／硬件组合尚未提供完整验收记录。详细证据与范围见 [支持与诊断](https://github.com/wynxing/Veil/blob/main/doc/validation/support-and-diagnostics.md)。

## 下载与安装

- 安装包：`{{SETUP_FILE}}`。下载后核对本 Release 的 `SHA256SUMS.txt`。
- 安装包没有 Authenticode 签名，Windows 可能显示 SmartScreen 或「未知发布者」提示。
- MTT 是已签名的第三方显示驱动，用于关光全部物理屏时留下活动路径。安装时可取消设备安装，驱动文件仍随应用保留，之后可从面板安装。
- Windows 10 与 ARM 不在本版支持范围内。应用不需要预装 .NET。
