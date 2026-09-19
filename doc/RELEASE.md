# Veil 预览发布

这是私有预览的打包与分发说明，不是公开产品合同。产品要求仍见 [PRD.md](PRD.md) 与 [PRODUCT_DESIGN.md](PRODUCT_DESIGN.md)。安装包签名未做，**不得标为可公开安装**。

## 定位

| 项 | 状态 |
| --- | --- |
| 产品要求 | 只发布已验证配置上的行为；能力检测失败则禁用并说明 |
| 本次流程 | 私有仓库 GitHub Release 挂无签名的 Windows x64 预览包（Rust MSVC 三个 exe + WiX），供协作者下载自用 |
| 已验证 | payload 哈希门禁成立；本机可用 `pack.ps1` 打出无签名 Burn EXE |
| 不是 | 代码签名、SmartScreen 信誉、公开仓库、全平台兼容、Rust MSI 已在 REDMI 机旁重装 |

预览包能装、能跑，不等于公开产品已发布，也不等于硬件兼容已过。

## 版本

单一来源：[src/version.props](../src/version.props) 的 `Version` 与可选 `VersionSuffix`。

- MSI / Burn 只用数字版本，例如 `0.1.1`。下一包必须升高（`0.1.2`），否则 `MajorUpgrade` 拒装。
- 标签：`v` + 数字版本 + 可选 suffix，例如 `v0.1.1-preview.1`。
- 文件名：`VeilSetup-0.1.1-preview.1-x64.exe`。

改版本只改 props，不要在 WiX 里手写另一套数字。

## 本机打包

在仓库根：

```powershell
cargo test --manifest-path src\Cargo.toml
.\installer\FetchPayload.ps1
.\installer\pack.ps1
```

`pack.ps1` 在 payload 缺文件时会自己调用 `FetchPayload.ps1`。已有文件但哈希/签名不符时必须失败，不得改哈希凑合。

`FetchPayload.ps1` 从 [payload.manifest.json](../installer/payload.manifest.json) 的 `sources` 下载已核验上游包，抽出 4 个文件后再跑 [ValidatePayload.ps1](../installer/ValidatePayload.ps1)。二进制不进 Git。不要调用实验室脚本 `tools/display-probe/install-vdd.ps1`。

产物在 `installer/dist/`（不进 Git）：

- `VeilSetup-<informational>-x64.exe`
- `SHA256SUMS.txt`

应用按 Rust `x86_64-pc-windows-msvc` release 静态链接发布，目标机不必先装 .NET。WiX 仍用本机 `dotnet` 编译安装器工程。安装目录里的 exe 名保持 `Veil.App.exe` / `Veil.Recovery.exe` / `Veil.DriverHelper.exe`。

## 打 tag 与 CI

1. 把流程变更合并进 `main`。
2. 确认 props 版本与即将打的 tag 一致。
3. 推送标签，例如 `git tag v0.1.0-preview.1` 后 `git push origin v0.1.0-preview.1`。
4. `.github/workflows/release.yml` 在 `windows-latest` 上：拉取 payload、跑测试、打包、以 **prerelease** 创建 GitHub Release。

本机也可以在干净工作区跑 `.\installer\release.ps1`：测试、打包、用 `gh release create --prerelease` 上传。它可能在本地创建 tag，**不会**执行 `git push --tags`。CI 传入的 tag 必须与 props 算出的 tag 一致。

Release 正文固定声明：无 Authenticode、SmartScreen 会拦截、仅 Windows 11 x64 预览、已测机器是 REDMI Book 14 2025 与 COLORFUL P15 24、不是可公开安装、不是全平台兼容。

## 下载与安装注意

- 私有仓库的 Release 只对协作者可见：[Releases](https://github.com/wynxing/Veil/releases)。
- 下载后核 `SHA256SUMS.txt`。
- SmartScreen /「未知发布者」是无签名预览的预期现象；这不是发布门禁已通过。
- 自带 MTT 是显示驱动。不同意则安装时不要装 VDD（`INSTALLVDD=0` / 引导程序选项）。
- COLORFUL P15 **不要**用该包安装 MTT VDD（已有实体外接）。见 [installer-payload.md](validation/installer-payload.md)。
- 装完仍按验证文档的范围使用；未测项不得当成已完成。Rust MSI 重装尚未机旁执行。

## 下一次预览

当前预览版本是 `0.1.4-preview.1`（`v0.1.4-preview.1`）。0.1.3 面板能打开，但 egui 默认字体没有中文，按钮和说明显示为空框；这版启动时加载系统雅黑/黑体。再改代码：升高 `Version`、合并、再打新 tag。不要复用同一个 MSI 版本号。

## 明确延后

- 安装包 Authenticode、时间戳、SmartScreen 信誉。
- 公开仓库，或对外宣传「可公开安装」。
- Burn 许可页、正式 LICENSE。
- 把 Windows 10、ARM 或未测机器写入支持列表。
- 把 PRD / 产品设计里的「未发布」改成已完成。
