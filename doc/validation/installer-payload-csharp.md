# 验证结果：C# 安装器 payload 与本地 Burn 包

状态：本机已放入与 manifest 一致的 25.7.23 / NefCon v1.20.0 文件并尝试打本地包。安装包无 Authenticode，**可本地装、不可公开安装**。未在任何机器上用该包完成驱动同意与关屏。  
日期：2026-09-18

未调用实验室脚本 `tools/display-probe/install-vdd.ps1`。二进制不进 Git。

## 缺 payload 时必须失败

在拷入文件之前，`installer/ValidatePayload.ps1` 退出码 1，报缺 `vdd/mttvdd.cat`、`vdd/MttVDD.dll`、`vdd/MttVDD.inf`、`nefcon/x64/nefconc.exe`。未改哈希凑合。

## 文件来源与哈希

本机原先没有实验室解包目录。按 [payload.manifest.json](../../installer/payload.manifest.json) 的哈希，从上游已核验发布包取出同字节文件（不是改哈希）：

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

结论：此版本驱动把配置目录写死为 `C:\VirtualDisplayDriver`。产品仍把 INF/DLL 装到 `%ProgramFiles%\Veil\vdd`，但 `Veil.DriverHelper` 安装时把同一份 `vdd_settings.xml` **同时**写到安装目录与 `C:\VirtualDisplayDriver`。若该路径已有他人配置则拒绝覆盖；nefcon 安装失败则回滚本次新写的文件。这只说明读取路径已核对，**不是**「安装路径已在 REDMI 上验证可用」，也不是公开安装许可。

P15 上 `C:\VirtualDisplayDriver` 本不存在；本轮没有创建该目录、没有跑 `install-driver`。

## 本地包

`installer/pack.ps1` 已成功。产物在 `installer/Veil.Setup/bin/Release/`（不进 Git）：

| 文件 | 大小 | SHA-256 | Authenticode |
| --- | --- | --- | --- |
| `Veil.msi` | 712704 | `E76D965D66B4E29C090779C0698E5E6CCD5DBCE35FD182FF15E04B0C5F9E6077` | NotSigned |
| `VeilSetup.exe`（Burn） | 1751713 | `A155AA95F36868DC377F991AC29837E4515B4D2E4117FBA85D6E1FF7F53D4472` | NotSigned |

第一次 `pack.ps1` 因 WiX v5 不接受 Feature 内嵌 `Condition`、默认编译把 `Bundle.wxs` 打进 MSI、以及多文件 Component 的 `Guid='*'` 失败；已改为 `Level`、分项目编译、一文件一组件，并让 pack 同时 publish Recovery / DriverHelper。缺 payload 时仍失败。

COLORFUL P15 **不得**用该包安装 MTT VDD。REDMI 本机已用后续无签名 MSI 做过禁用态安装与短时系统检查，见 [redmi-book-14-2025-csharp.md](redmi-book-14-2025-csharp.md)。仍无 Authenticode，不可公开安装。
