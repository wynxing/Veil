# Veil 预览发布

这是公开仓库的无签名预览打包与分发说明，不是产品合同，也不是已签名的公开发布。产品要求仍见 [PRD.md](PRD.md) 与 [PRODUCT_DESIGN.md](PRODUCT_DESIGN.md)。安装包没有 Authenticode。

## 定位

| 项 | 状态 |
| --- | --- |
| 产品要求 | 能力检测失败则禁用并说明；不宣称全平台兼容 |
| 本次流程 | 公开仓库 GitHub Release 挂无签名的 Windows x64 预览包（Rust MSVC 三个 exe + WiX） |
| 已验证 | payload 哈希门禁成立；本机可用 `pack.ps1` 打出无签名 Burn EXE |
| 不是 | 代码签名、SmartScreen 信誉、已签名的公开发布、全平台兼容 |

预览包能装、能跑，不等于公开产品已发布，也不等于硬件兼容已过。

## 版本

单一来源：[src/version.props](../src/version.props) 的 `Version` 与可选 `VersionSuffix`。

- MSI / Burn 只用数字版本，例如 `0.1.1`。下一包必须升高（`0.1.2`），否则 `MajorUpgrade` 拒装。
- 标签：`v` + 数字版本 + 可选 suffix，例如 `v0.1.1-preview.1`。
- 文件名：`VeilSetup-0.1.1-preview.1-x64.exe`。

改版本只改 props，不要在 WiX 里手写另一套数字。`src/Cargo.toml` 的 `version` 与 `workspace.metadata.veil.suffix` 必须和这份 props 一致，发布预检会核对，不一致则失败。应用内更新比较的是 props 编出来的信息版本（例如 `0.1.16-preview.1`），不是丢掉后缀的文件版本 `0.1.16.0`。

## 本机打包

在仓库根：

```powershell
cargo test --manifest-path src\Cargo.toml
.\installer\FetchPayload.ps1
.\installer\pack.ps1
```

`pack.ps1` 在 payload 缺文件时会自己调用 `FetchPayload.ps1`。已有文件但哈希/签名不符时必须失败，不得改哈希凑合。

`FetchPayload.ps1` 从 [payload.manifest.json](../installer/payload.manifest.json) 的 `sources` 下载已核验上游包，抽出 4 个文件后再跑 [ValidatePayload.ps1](../installer/ValidatePayload.ps1)。二进制不进 Git。

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

发布预检回归验证：`pwsh -NoProfile -File installer/Test-ReleasePreflight.ps1`。构建或发布成功仍不代表安装、升级及屏幕控制已在每台机器上实测。

Release 正文固定声明：无 Authenticode、SmartScreen 会拦截、仅 Windows 11 x64 预览、不是已签名的公开发布、不是全平台兼容。应用内更新只提示并打开发布页，不下载安装包。

## 下载与安装注意

- 公开仓库的 Release：[Releases](https://github.com/wynxing/Veil/releases)。
- 下载后核 `SHA256SUMS.txt`。
- SmartScreen /「未知发布者」是无签名预览的预期现象；这不是发布门禁已通过。
- 已安装的预览在面板打开时最多每 24 小时检查一次更新。有新版本只提示并打开发布页，不下载、不启动安装包。
- MTT 是显示驱动。`INSTALLVDD=0` 只表示这次不创建设备，驱动文件仍随应用写入，以后可在面板安装。
- 未测 GPU / 系统不得当成已完成。这不是已签名的公开发布。

## 下一次预览

当前预览版本是 `0.1.16-preview.1`（`v0.1.16-preview.1`）。数字版本从 `0.1.15` 升高，满足 MajorUpgrade 的版本递增要求，实际升级仍待安装验收。本版加入统一产品图标、首次安装路径选择、默认关闭的公共桌面快捷方式和明确的辅助设备选项；升级沿用原目录。安装、升级、修复、卸载及屏幕恢复尚未在隔离 Windows 环境完成验收。睡眠回路与唤醒闪屏的机旁矩阵仍未通过。0.1.7 起升级先 `retire-old`。再改代码：升高 `Version`、合并、再打新 tag。

## 明确延后

- 安装包 Authenticode、时间戳、SmartScreen 信誉。有签名之前，不做下载并启动安装包的自更新。
- 对外把无签名预览说成已签名的公开发布。
- Burn 许可页。根目录 `LICENSE` 与 `NOTICE` 已经写明 MIT 和再分发组件。
- 把 Windows 10、ARM 或未测机器写入支持列表。
- 把 PRD / 产品设计改成功能已完成或全平台兼容。
