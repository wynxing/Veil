# Veil 版本与发布

当前正式版本为 **1.0.0**，发布范围为 Windows 11 x64。安装包没有 Authenticode 签名；Windows 可能显示 SmartScreen 或「未知发布者」提示。支持与验证范围集中见 [支持与诊断](validation/support-and-diagnostics.md)。

## 版本来源与渠道

数字版本和可选后缀由 `src/version.props` 定义；`src/Cargo.toml` 的 workspace 版本与后缀、`src/Cargo.lock` 的本地包版本须保持一致。

| 版本 | 标签与安装包 | GitHub 渠道 |
| --- | --- | --- |
| 无后缀，例如 `1.0.0` | `v1.0.0`、`VeilSetup-1.0.0-x64.exe` | 正式 Release，设为 Latest |
| 有后缀，例如 `1.1.0-preview.1` | `v1.1.0-preview.1`、`VeilSetup-1.1.0-preview.1-x64.exe` | prerelease，不设为 Latest |

MSI / Burn 使用数字版本，每次更新安装包必须升高数字版本，否则 MajorUpgrade 会拒绝同号版本。应用更新比较完整信息版本；正式版只提示后续正式版，预览版可以发现正式版本。面板打开时最多每 24 小时检查一次，只提示并打开发布页。

## 本机验证与打包

```powershell
pwsh -NoProfile -File installer/Test-ReleasePreflight.ps1
cargo test --manifest-path src/Cargo.toml --workspace --locked --release --target x86_64-pc-windows-msvc
cargo check --manifest-path src/Cargo.toml --workspace --all-targets --locked --release --target x86_64-pc-windows-msvc
.\installer\pack.ps1
```

`pack.ps1` 在 payload 缺失时调用 `FetchPayload.ps1`，按 [清单](../installer/payload.manifest.json) 下载并核验哈希、签名和发布者指纹；已有文件校验不符时失败，不替换预期哈希。二进制不进 Git。

产物位于 `installer/dist/`：`VeilSetup-<完整版本>-x64.exe` 与 `SHA256SUMS.txt`。应用为 Rust x64 MSVC 静态链接发布，目标机不需要 .NET；构建 WiX 安装器需要 .NET SDK。安装后的程序名为 `Veil.App.exe`、`Veil.Recovery.exe`、`Veil.DriverHelper.exe`。

## PR、标签与发布工作流

1. 从最新远端 `main` 新建工作树和分支，修改版本、实现及发布说明，完成本机验证后提交 PR。
2. 等待必需的 `windows` 检查成功并合并，确认目标提交已进入远端 `main`。
3. 在合并提交上创建对应标签，推送该标签；例如 `git tag v1.0.0`、`git push origin v1.0.0`。
4. `.github/workflows/release.yml` 执行预检、payload 获取、测试、打包及发布，按版本后缀选择渠道。
5. 等待最终成功，检查 Release 非草稿、渠道正确、标签指向预期提交、附件大小非零。下载公开安装包，按同页 `SHA256SUMS.txt` 核验 SHA-256。
6. 整合回本地并保留已有成果；只清理本次已整合的工作树和分支。

优先使用「推送标签 → CI 发布」入口。本机 `installer/release.ps1` 也能测试、打包和上传，但可能由 GitHub 创建远端标签并触发标签 CI，不要再重复推送同名标签。

只读结构预检：`./installer/release.ps1 -CheckOnly -Tag <标签>`。同名 Release 完整、渠道一致、远端标签指向当前提交时直接成功退出，保留已发布附件；草稿、渠道不符、缺失附件、标签冲突或查询失败均报错，不自动覆盖。结构预检不替代下载后的哈希校验。

工作流使用 Rust 缓存、locked release 构建和分步超时。同一标签串行发布，打包产物另存为保留 14 天的 Actions artifact。失败时先检查实际 Release 状态；不存在则可重跑，附件不完整时先排查，不自动覆盖。旧标签重跑仍使用该标签中的旧流程。

## 安装与维护

MTT 是关光全部物理屏时的辅助显示驱动。`INSTALLVDD=0` 只取消本次设备安装，驱动文件仍随应用写入，以后可从面板安装。升级沿用原安装目录；退出、升级维护及卸载遵守屏幕恢复门禁，详见 [安装器](../installer/README.md)。

代码签名、时间戳、SmartScreen 信誉与未来自动下载安装更新单独处理，不作为版本后缀的定义。当前程序只打开发布页，不自动下载安装包。
