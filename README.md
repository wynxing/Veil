# Veil

Veil keeps selected physical displays off on Windows until you turn them back on. It is not a global screen-off hotkey.

Veil 是 Windows 屏幕保持关闭工具：按物理屏决定关或开，直到主动恢复。它不是全局熄屏快捷方式。

**状态：** 源码公开。安装包是无签名预览，不是已签名的公开发布，也不宣称全平台兼容。能力检测失败则禁用并说明。

## 下载预览

仅面向 **Windows 11 x64**。打 `v*` 标签后，[GitHub Releases](https://github.com/wynxing/Veil/releases) 提供 `VeilSetup-*-x64.exe` 与 `SHA256SUMS.txt`。下载后核对校验和。安装包没有 Authenticode，SmartScreen /「未知发布者」会拦截，这是无签名预览的预期。

Windows 10、ARM 与未测 GPU 不在支持范围内。睡眠或待机后的机旁矩阵尚未通过，见 [唤醒闪屏验收](doc/validation/resume-flicker.md)。

已安装的预览在面板打开时检查更新，最多每 24 小时一次。有新版本时提示，并打开发布页。程序不下载、不启动安装包。

## 能做什么

- 让指定物理屏保持关闭，直到在面板里恢复或按紧急热键 `Ctrl+Alt+Shift+F10`。
- 可同时管理多块物理屏。关光全部物理屏时，用已签名 MTT 辅助输出作隐藏退路。安装时默认一起装设备（禁用）；没有设备可在面板安装；本机已有同一 MTT 则接管。
- 失败时说明原因。黑色画面不是关屏成功。

是否阻止系统睡眠由以后的独立选项决定，默认不改电源计划。界面是 egui 小面板和原生托盘。关屏 APPLY 只发生在 `Veil.Recovery`。

## 从源码构建

```powershell
cargo test --manifest-path src\Cargo.toml
```

安装器需要已核验 payload，见 [installer/README.md](installer/README.md)。缺文件时运行 `installer/FetchPayload.ps1`。无 payload 时安装器构建会失败。

## 文档

- [产品需求](doc/PRD.md)
- [产品设计](doc/PRODUCT_DESIGN.md)
- [技术架构](doc/ARCHITECTURE.md)
- [等待过程、日志与机旁验收](doc/validation/responsive-operations.md)
- [预览发布](doc/RELEASE.md)
- [安装器](installer/README.md)
- [许可证](LICENSE)（MIT）。安装包再分发的驱动与 NefCon 见 [NOTICE](NOTICE)。
- [安全说明](SECURITY.md)
