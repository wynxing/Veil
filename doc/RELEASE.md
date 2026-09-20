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

优先使用「推送标签 → CI 发布」这一条入口。本机发布会在 GitHub 创建标签，可能同时触发标签 CI；不要再重复手动推送同名标签。

发布前会查询 GitHub：同名 Release 非草稿、远端标签指向当前提交，且安装包与校验文件均已上传、大小非零时，直接成功退出并保留已有附件；草稿、缺失附件、标签冲突或查询失败均明确报错，不覆盖安装包。此检查确认发布结构完整，不代替下载后的 SHA-256 校验。只读检查可运行 `./installer/release.ps1 -CheckOnly`。

CI 使用 Rust 依赖缓存，测试和打包统一使用 `--locked --release --target x86_64-pc-windows-msvc`；`pack.ps1` 负责构建，无单独的重复构建步骤。同一标签的发布串行执行，任务预算为 25 分钟，并为下载、测试、打包、上传设置步骤超时。执行机器失联时，GitHub 的故障检测仍可能晚于预算，超时配置不保证失联任务立即终止。

打包完成后先保存 14 天的 Actions artifact，再创建 Release。若上传阶段失败，可先取回 artifact 排查。执行机器失联且 Release 尚不存在时，重跑失败任务；若 Release 已存在但附件不完整，应人工检查并修复，脚本不会自动覆盖。旧标签重跑仍使用该标签中的旧流程，新的缓存与预检配置只对包含此次流程修改的标签生效。

发布预检回归验证：`pwsh -NoProfile -File installer/Test-ReleasePreflight.ps1`。构建或发布成功仍不代表安装、升级及屏幕控制已在实机验收。

Release 正文固定声明：无 Authenticode、SmartScreen 会拦截、仅 Windows 11 x64 预览、已测机器是 REDMI Book 14 2025 与 COLORFUL P15 24、不是可公开安装、不是全平台兼容。

## 下载与安装注意

- 私有仓库的 Release 只对协作者可见：[Releases](https://github.com/wynxing/Veil/releases)。
- 下载后核 `SHA256SUMS.txt`。
- SmartScreen /「未知发布者」是无签名预览的预期现象；这不是发布门禁已通过。
- MTT 是显示驱动。`INSTALLVDD=0` 只表示这次不创建设备，驱动文件仍随应用写入，以后可在面板安装。
- COLORFUL P15 过去的 Python 验收没装 MTT（当时测实体外接）。产品不再禁止在这台机器上装辅助输出；关光双物理屏的补装尚未机旁验证。见 [installer-payload.md](validation/installer-payload.md)。
- 装完仍按验证文档的范围使用；未测项不得当成已完成。Rust MSI 重装尚未机旁执行。

## 下一次预览

当前预览版本是 `0.1.8-preview.1`（`v0.1.8-preview.1`）。关最后一块物理屏时可从面板安装或接管辅助 MTT；安装器始终带上驱动文件，`INSTALLVDD=0` 只表示这次不创建设备。0.1.7 起升级先 `retire-old`，避免旧 `RestoreDisplays` 拦死 MajorUpgrade。这不是 P15 关光双屏或 Rust MSI 重装已机旁通过。再改代码：升高 `Version`、合并、再打新 tag。

## 明确延后

- 安装包 Authenticode、时间戳、SmartScreen 信誉。
- 公开仓库，或对外宣传「可公开安装」。
- Burn 许可页、正式 LICENSE。
- 把 Windows 10、ARM 或未测机器写入支持列表。
- 把 PRD / 产品设计里的「未发布」改成已完成。
