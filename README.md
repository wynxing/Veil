# Veil

Veil 是 Windows 屏幕保持关闭工具：按物理屏决定关或开，直到主动恢复。

**正式版：1.0.1 · Windows 11 x64。** 核心关屏与恢复已由用户实际使用。当前支持与验证范围见 [支持与诊断](doc/validation/support-and-diagnostics.md)。

## 45 秒认识 Veil

**你玩你的。屏幕，由你决定。** 从宿舍里的小尴尬，到闲置副屏休息，看看 Veil 为什么存在。

https://github.com/user-attachments/assets/85ee4b5e-0ddb-4c41-909d-49f64f38d55b

[打开或下载视频](https://github.com/user-attachments/assets/85ee4b5e-0ddb-4c41-909d-49f64f38d55b) · 45 秒。点击播放器的音量按钮开启声音。

宣传片为原创动画，操作画面是情境示意，不作为硬件或远程软件兼容性实测。

## 下载与安装

从 [GitHub Releases](https://github.com/wynxing/Veil/releases/latest) 下载 `VeilSetup-1.0.1-x64.exe`，并核对同页的 `SHA256SUMS.txt`。安装包没有 Authenticode 签名，Windows 可能显示 SmartScreen 或「未知发布者」提示。

发布范围为 **Windows 11 x64**；Windows 10 与 ARM 不在本版支持范围内。辅助输出是已签名的第三方 MTT 显示驱动，安装页说明其用途并允许取消本次设备安装。

## 能做什么

- 让指定物理屏保持关闭，直到在面板里恢复或按紧急热键 `Ctrl+Alt+Shift+F10`。
- 按屏管理关闭与恢复。关光全部物理屏时，用 MTT 辅助输出留下活动路径；没有设备时可从面板安装，本机已有同一 MTT 则接管。
- 面板关闭或最小化后托盘继续运行；正常退出先恢复物理屏。
- 操作失败时说明原因，并保留恢复入口。黑色画面不是关屏成功。

默认不改电源计划。睡眠或待机中断后，设计行为是结束保持关闭并恢复显示，继续关屏须再次操作。具体测试覆盖范围见 [支持与诊断](doc/validation/support-and-diagnostics.md)。

面板打开时最多每 24 小时检查一次更新，有新版本时提示并打开发布页。正式版只提示正式版本；程序不下载、不启动安装包。

## 从源码构建

```powershell
cargo test --manifest-path src\Cargo.toml --workspace --locked
```

安装器需要已核验 payload，见 [安装器](installer/README.md)。缺文件时运行 `installer/FetchPayload.ps1`。

## 文档

- [产品需求](doc/PRD.md)
- [产品设计](doc/PRODUCT_DESIGN.md)
- [技术架构](doc/ARCHITECTURE.md)
- [支持、验证范围与故障诊断](doc/validation/support-and-diagnostics.md)
- [版本与发布流程](doc/RELEASE.md)
- [安装器](installer/README.md)
- [许可证](LICENSE)（MIT）。安装包再分发的驱动与 NefCon 见 [NOTICE](NOTICE)。
- [安全说明](SECURITY.md)
