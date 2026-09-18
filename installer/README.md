# Veil 安装器

传统 WiX 5 Burn 引导 EXE + 应用 MSI。安装包代码签名未做，**不得标为可公开安装**。

## 构建

1. 按 [payload/README.md](payload/README.md) 放入已核验的 VDD 与 NefCon。
2. 运行 `installer/ValidatePayload.ps1`：缺失或哈希/签名不符会失败。
3. 运行 `installer/pack.ps1` 发布应用并编译 MSI/Bundle。

无 payload 时不得打出缺驱动的包。实验室脚本 `tools/display-probe/install-vdd.ps1` 不得被调用。

## 安装行为

- 提权后写入 `%ProgramFiles%\Veil`。
- 驱动同意：自带 MTT 是显示驱动，用于没有外接屏时关掉笔记本屏幕。不同意则 `INSTALLVDD=0`，只装应用。
- INF 安装后设备保持禁用。
- 安装时不开机自启、不关屏。
- 卸载：先对仍打开的会话写 `release.json`，再禁用并移除自带 `Root\MttVDD`，不改装其它虚拟屏。

## vdd_settings.xml 路径

产品把配置写到 `%ProgramFiles%\Veil\vdd\vdd_settings.xml`。MTT 驱动是否只认该目录、或仍写死 `C:\VirtualDisplayDriver`，**尚未用捆绑 DLL 核对**。未核对该路径前，不得宣称按需 VDD 安装路径已验证。
