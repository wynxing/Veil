# Veil

Veil 是可公开设计中的 Windows 屏幕保持关闭工具：按物理屏决定关或开，直到主动恢复。它不是全局熄屏快捷方式。首版不做临时关闭。

**项目状态：公开产品 C# 代码已开始，不是已发布安装包。** 机制可行性已在两台机器上收口。C# 在 COLORFUL P15 上：只停内屏的 `release.json` 与热键恢复有机旁观察；面板「恢复」/「恢复全部」与只停外屏（含原点调整）有系统检查，外屏口头机旁未做。本地可打出无签名 Burn EXE；`MttVDD.dll` 写死 `C:\VirtualDisplayDriver`。REDMI 安装器 VDD 与睡醒再关**未执行**。仓库 `app/` 仍冻结。设计见 [产品设计](doc/PRODUCT_DESIGN.md)，实现栈见 [技术架构](doc/ARCHITECTURE.md)，合同见 [PRD](doc/PRD.md)。

本地构建（x64）：

```powershell
dotnet test src\Veil.sln -p:Platform=x64
```

安装器需要已核验 payload，见 [installer/README.md](installer/README.md)。无 payload 时安装器构建应失败。安装包未代码签名，不得标「可公开安装」。

## 希望解决的问题

- 让指定物理屏保持关闭，直到用户主动恢复。
- 可同时管理多块物理屏；没有外接时，用安装器自带、默认禁用的已签名虚拟输出作为隐藏退路，以便关光全部物理屏。
- 失败时说明原因，不用黑窗或系统熄屏冒充保持关闭。

是否阻止系统睡眠由以后的独立选项决定，默认不改电源计划。

## 首版能力（设计，非已发布）

物理屏列表、按屏保持关闭与单独恢复、恢复全部、紧急热键、按需隐藏虚拟输出。虚拟屏不对用户提供开关。跨重启不自动再关。睡醒后尝试再关，失败则结束并说明——C# 睡醒闭环尚未机旁执行。

验收以被选物理屏持续不显示桌面内容为准；面板断电按设备记录。黑色画面不是关屏成功。

## 已验证的机制（不是全平台兼容）

第一组：XIAOMI REDMI Book 14 2025，仅笔记本内屏。原生停最后一条路径校验 87。已签名 Virtual Display Driver 25.7.23 作为第二目标后，可保持关闭内屏（短时、10 分钟、循环、崩溃恢复）。睡醒后内屏亮起，保持关闭结束且当时未自动再关。详见 [虚拟屏辅助结果](doc/validation/redmi-book-14-2025-vdd.md)。

第二组：COLORFUL P15 24，内屏加实体外接 S24Q6-Q24G8，不额外安装虚拟屏。Python 探针只停内屏已有短时、10 分钟、循环与崩溃恢复；只停外屏有短时闭环。C# 产品见 [P15 C#](doc/validation/colorful-p15-24-csharp.md)。C# 安装器 payload 见 [安装器核对](doc/validation/installer-payload-csharp.md)。C# REDMI 路径见 [C# REDMI 未执行](doc/validation/redmi-book-14-2025-csharp.md)。

## 文档

- [产品需求](doc/PRD.md)：公开产品合同。
- [产品设计](doc/PRODUCT_DESIGN.md)：形态、进程、按需路径引擎、安装与验收边界。
- [技术架构](doc/ARCHITECTURE.md)：.NET / WPF / WiX、双进程切分、现存 Python 代码处置。
- [技术验证协议](doc/TECH_VALIDATION.md)：实验规则；可行性调研已收口。
- [最小应用](app/README.md)：已冻结的调研原型，仅本机已验证配置。
- [产品工程](src/)：C# / WPF。P15 短时只停内屏与热键恢复已观察；面板按钮与只停外屏仅有系统检查。不是已发布。
- [安装器](installer/README.md)：WiX 5；payload 缺失则构建失败。本地包无签名，不可公开安装。
- [协作规范](AGENTS.md)：工作树开发与清理要求。
