# 验证结果：安装器 payload 与本地打包

状态：本机已放入与 [payload.manifest.json](../../installer/payload.manifest.json) 一致的 Virtual Display Driver 25.7.23 / NefCon v1.20.0。缺文件或哈希不符时构建必须失败。安装包无 Authenticode，可本地打、**不可公开安装**。Rust MSI / Burn 尚未在 REDMI 上机旁重装。COLORFUL P15 **不得**用该包安装 MTT VDD。  
日期：2026-09-19

未调用实验室脚本 `tools/display-probe/install-vdd.ps1`。二进制不进 Git。

## 缺 payload 时必须失败

拷入文件之前，`installer/ValidatePayload.ps1` 退出码 1，报缺 `vdd/mttvdd.cat`、`vdd/MttVDD.dll`、`vdd/MttVDD.inf`、`nefcon/x64/nefconc.exe`。未改哈希凑合。

## 文件来源与哈希

按 manifest 的 `sources` 从上游已核验发布包取出同字节文件：

| 资产 | URL |
| --- | --- |
| Virtual Display Driver 25.7.23 | `https://github.com/VirtualDrivers/Virtual-Display-Driver/releases/download/25.7.23/VirtualDisplayDriver-x86.Driver.Only.zip` |
| NefCon v1.20.0 x64 | `https://github.com/nefarius/nefcon/releases/download/v1.20.0/nefcon_v1.20.0.zip` 内 `x64/nefconc.exe` |

拷入 `installer/payload/` 后 SHA-256 与指纹与 manifest 一致：

| 文件 | SHA-256 / 指纹 |
| --- | --- |
| `vdd/mttvdd.cat` | `08A0093FC9B2E32B287A6F8A77CA4DE0A31830D29FC33D2B13A918DC859468F6` |
| `vdd/MttVDD.dll` | `C9CA837F57A98FBD43BC416A7F535A95843626E7759EAF85CF0CD7CE334DBB05` |
| `vdd/MttVDD.inf` | `550D211FE481E74DFE3F9D724ED78BE48B3A9113405965D683D9373E8D672F5D` |
| `nefcon/x64/nefconc.exe` | `B65013F08BEF9D0DDCDEEF7501FC6BE346478B7B29B7730DE97C496408DDF9B4` |
| CAT/DLL 发布者 | SignPath Foundation，`3CF8CF26D8BA266C3A483AB7D26D4A818E317D76`，Authenticode `Valid` |
| nefconc | Nefarius Software Solutions e.U.，Authenticode `Valid` |

## `vdd_settings.xml` 路径（已用捆绑 DLL 只读核验）

对 `MttVDD.dll` 做 ASCII / UTF-16 字符串检查（未加载驱动、未在 P15 安装）：

- UTF-16 含完整路径 **`C:\VirtualDisplayDriver`**
- UTF-16 另有 `\vdd_settings.xml`
- ASCII 有 `Using vdd_settings.xml` / `Loading GPU from vdd_settings.xml`
- 未见 `%ProgramFiles%\Veil`

结论：此版本驱动把配置目录写死为 `C:\VirtualDisplayDriver`。产品仍把 INF/DLL 装到 `%ProgramFiles%\Veil\vdd`，但 `Veil.DriverHelper` 安装时把同一份 `vdd_settings.xml` **同时**写到安装目录与 `C:\VirtualDisplayDriver`。若该路径已有他人配置则拒绝覆盖；nefcon 安装失败则回滚本次新写的文件。这只说明读取路径已核对，**不是** Rust 安装器已在 REDMI 上验收，也不是公开安装许可。

P15 上不要创建该目录，也不要跑 `install-driver`。

## 本地包

`installer/pack.ps1` 用 `cargo build --release` 产出三个 exe（拷成 `Veil.*.exe`）并编译 MSI / Bundle。缺 payload 时失败。产物在 `installer/dist/`（不进 Git）。无 Authenticode，**不可公开安装**。
