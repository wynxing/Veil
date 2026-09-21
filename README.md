# Veil

Veil 是 Windows 屏幕保持关闭工具：按物理屏决定关或开，直到主动恢复。它不是全局熄屏快捷方式。首版不做临时关闭。

**项目状态：日常预览可用。** 无签名预览包可下载，**不是已发布、不得标可公开安装**，也不宣称全平台兼容。能力检测失败则禁用并说明。设计见 [产品设计](doc/PRODUCT_DESIGN.md)，实现栈见 [技术架构](doc/ARCHITECTURE.md)，合同见 [PRD](doc/PRD.md)。

本地构建（x64）：

```powershell
cargo test --manifest-path src\Cargo.toml
```

安装器需要已核验 payload，见 [installer/README.md](installer/README.md)。缺文件时可先跑 `installer/FetchPayload.ps1`；无 payload 时安装器构建应失败。安装包未代码签名，不得标「可公开安装」。

## 预览下载（不是可公开安装）

私有仓库在打 `v*` 标签后，会把无签名预览包 `VeilSetup-*-x64.exe` 挂到 [GitHub Releases](https://github.com/wynxing/Veil/releases)。这是协作者自用预览：SmartScreen 会警告，**不得标可公开安装**，也不是公开产品已发布。流程见 [预览发布](doc/RELEASE.md)。

## 希望解决的问题

- 让指定物理屏保持关闭，直到用户主动恢复。
- 可同时管理多块物理屏；关光全部物理屏时，用已签名 MTT 辅助输出作隐藏退路。安装应用时默认一起装（禁用）；没有设备可在面板安装；本机已有同一 MTT 则接管。
- 失败时说明原因，不用黑窗或系统熄屏冒充保持关闭。

是否阻止系统睡眠由以后的独立选项决定，默认不改电源计划。

## 首版能力

物理屏列表、按屏保持关闭与单独恢复、恢复全部、紧急热键、按需隐藏虚拟输出。虚拟屏不对用户提供开关。跨重启不自动再关。睡醒后尝试再关，失败则结束并说明。这些已在 `src/` 落地并有离线测试。未测 GPU、Windows 10 与 ARM 不得写入支持列表。

验收以被选物理屏持续不显示桌面内容为准；面板断电按设备记录。黑色画面不是关屏成功。

## 文档

- [产品需求](doc/PRD.md)：公开产品合同。
- [产品设计](doc/PRODUCT_DESIGN.md)：形态、进程、按需路径引擎、安装与验收边界。
- [技术架构](doc/ARCHITECTURE.md)：Rust / egui / WiX、双进程切分。
- [产品工程](src/)：Rust / egui。不是已发布。
- [安装器](installer/README.md)：WiX 5；payload 缺失则构建失败。本地包无签名，不可公开安装。
- [预览发布](doc/RELEASE.md)：私有 Release、版本、fetch/pack/tag。不是可公开安装。
- [协作规范](AGENTS.md)：工作树开发与清理要求。
