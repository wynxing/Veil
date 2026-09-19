这是 Veil {{INFORMATIONAL}} **无签名预览包**，供私有仓库协作者本机自用。

- 安装包没有 Authenticode。Windows SmartScreen / 未知发布者会拦截，这是预期，不是已通过发布门禁。
- **不得标为可公开安装**，也不是公开产品已发布。
- 仅面向 **Windows 11 x64**。Windows 10、ARM 与其它机器未测，不得写入支持列表。
- 已记录过机旁/系统检查的机器：XIAOMI REDMI Book 14 2025（安装器自带 VDD）、COLORFUL P15 24（实体外接）。不是全平台兼容。
- 自带 MTT 是显示驱动，用于没有外接屏时关掉笔记本屏幕。安装时不同意则不要装 VDD。
- 产物：`{{SETUP_FILE}}`（Rust x64 MSVC + WiX，无 .NET 运行时）。请核对本 Release 的 `SHA256SUMS.txt`。

流程与延后项见仓库 `doc/RELEASE.md`。
