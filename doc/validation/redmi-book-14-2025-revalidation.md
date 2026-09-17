# REDMI Book 14 2025：仅内屏第二轮复核

日期：2026-09-17；状态：不关屏检查完成，机旁关屏验证待执行。**持续关闭能力尚未通过。**

## 环境与证据位置

Windows 11 家庭中文版 26200、64 位 Python 3.12、Intel UHD Graphics 32.0.101.6733、内屏 TM140VDXP02。WMI `PowerOnline=true`；电源方案为平衡；系统支持 S0 低电量待机，不支持 S3。仅有一个活动物理内屏，未安装虚拟显示驱动。开盖和物理画面须机旁确认，不能由 WMI 供电信息推断。

原始证据保存在本机主仓库 `.git/veil-validation-20260917/`，不随 Git 分发：`diagnostic.jsonl`、`before/`、`bright-timer-v2/`、`bright-synthetic-hotkey-v2/`、`bright-parent-crash/`、`final-bright-watchdog.jsonl`、`driver-signatures.json`。文件包含系统状态与恢复过程；物理观察尚未补齐。

## 原生方案复核

在 03:28:40 UTC 执行。ctypes 结构尺寸核对通过：LUID 8、source 20、target 48、path 72、video signal 48、mode 64 字节，mode 联合体偏移 16。

| 配套配置 | Query flags | Set VALIDATE flags | 原样拓扑 | 停用唯一内屏 |
| --- | --- | --- | --- | --- |
| basic | `0x2` | `0x460` | 0，通过 | 87，失败 |
| virtual | `0x12` | `0x8460` | 0，通过 | 87，失败 |
| refresh | `0x52` | `0x28460` | 0，通过 | 87，失败 |

每组重新查询，不复用另一组的模式索引。基础组查询到 2 个 mode，虚拟模式组为 3 个；虚拟联合索引以原始 32 位值保存并回传，停用只修改 ACTIVE 位。所有停用实验均只 VALIDATE，没有 apply。

结论：三组对照成功、三组零活动目标校验失败，当前 CCD 路线未实现纯原生仅内屏持续关闭。87 表示参数组合无效；这些结果不能证明所有硬件、驱动或其他机制均不可能，也不能证明外接实体显示器是唯一解决方式。

## 电源事件交叉确认

本轮读取 Windows System 日志，对照第一轮已有看门狗时间，未重新执行临时关屏。

| UTC | Kernel-Power ID | 系统事件正文 |
| --- | --- | --- |
| 02:27:15.774 | 506 | 进入新型待机；原因 `SC_MONITORPOWER` |
| 02:36:23.654 | 507 | 退出新型待机；原因 `Input Touchpad` |
| 02:36:41.248 | 506 | 进入新型待机；原因 `SC_MONITORPOWER` |
| 02:40:59.740 | 507 | 退出新型待机；原因 `Input Touchpad` |

两段分别约 548 秒和 258 秒，与原报告看门狗被延后的时间吻合。进入现代待机已获得系统事件证据；不能再只写“感觉像睡眠”。这些事件本身不能精确证明每个后台任务在整个区间的运行情况。新探针提供独立哈希任务进度，实际关屏期间的连续性仍待测。

## 恢复与软件检查

| 检查 | 结果 | 边界 |
| --- | --- | --- |
| ABI、路径变换、三组参数、拒绝不安全执行等 13 项单元测试 | 通过 | 使用模拟系统调用，不触发关屏 |
| 亮屏独立进程 2 秒定时恢复 | 通过 | restore=0，活动目标与完整拓扑一致；未关屏 |
| F10 全局快捷键注册及自动注入消息链 | 通过 | 软件注入，不能算实际物理按键验证；未签发预检凭证 |
| 父启动进程 `os._exit(17)` 后，独立进程 3 秒定时恢复 | 通过 | 亮屏实验，恢复返回 0，完整拓扑一致；不是关屏后崩溃验收 |
| 机旁实际快捷键与关屏观察 | 未执行 | 等待操作者接电开盖、按键和观察 |

首次自动注入发生在 worker 刚 arm、尚未进入实验时，安全取消，`ok=false`；等待其进入消息循环后第二次测试通过。两份记录均保留，不把首次取消算作关屏故障。

切换和恢复已去掉 `SDC_SAVE_TO_DATABASE`。独立恢复进程先注册快捷键并回报就绪，再执行切换；父进程不能迟到关屏。异常或结果未确认时预检凭证失效。快捷键由原 F12 候选改为 F10，因为微软文档明确 F12 保留给调试器。

## 虚拟屏候选与下一步

已下载但**未安装**：官方 `VirtualDrivers/Virtual-Display-Driver` 发布版 **25.7.23**，资产 `VirtualDisplayDriver-x86.Driver.Only.zip`。名称虽为 x86，INF 声明 `NTamd64`，适用于此处 x64 候选验证。INF 版本为 `12/24/2024,11.30.4.434`；配置默认一块虚拟屏。

本机 `Get-AuthenticodeSignature` 对 `MttVDD.dll` 和 `mttvdd.cat` 均返回 `Valid`，签名人为 SignPath Foundation。这不是 WHQL 或本机兼容性通过的证明。安装工具采用官方 NefCon **v1.20.0 x64**，其签名同样为 `Valid`，签名人为 Nefarius Software Solutions e.U.。

| 文件 | SHA-256 |
| --- | --- |
| `mttvdd.cat` | `08A0093FC9B2E32B287A6F8A77CA4DE0A31830D29FC33D2B13A918DC859468F6` |
| `MttVDD.dll` | `C9CA837F57A98FBD43BC416A7F535A95843626E7759EAF85CF0CD7CE334DBB05` |
| `MttVDD.inf` | `550D211FE481E74DFE3F9D724ED78BE48B3A9113405965D683D9373E8D672F5D` |
| `nefconc.exe`（x64） | `B65013F08BEF9D0DDCDEEF7501FC6BE346478B7B29B7730DE97C496408DDF9B4` |

安装前设备基线已保存，`C:\VirtualDisplayDriver` 当前不存在。安装需要管理员/UAC，官方流程还涉及 TrustedPublisher 证书：只添加已核验发布者所需证书，记录新增项，不导入到根证书库，不修改 Secure Boot 或测试签名。

接续步骤：机旁准备完成后，先做实际快捷键预检；然后在管理员权限下按官方方式部署驱动包与一屏配置，使用已核对帮助信息的 `nefconc.exe install <MttVDD.inf> "Root\MttVDD" --no-duplicates`。安装后记录新设备实例和发布的 INF 名，枚举活动路径；仍无第二活动目标则停止，不直接关屏。拓扑变化后重新预检。

卸载路径：先恢复内屏，再用设备管理器卸载本轮新增的 Virtual Display Driver 并删除其驱动包；也可依据安装后记录的确切实例 ID／OEM INF 使用 PnPUtil 定向移除。不能删除其他显示驱动，不能预先猜测 OEM INF 编号。仅清理本轮新增且不再被使用的配置文件与发布者证书，不删除原有证书。

本轮未发出 UAC 安装请求，也未安装驱动、修改证书或执行关屏。尚未收到机旁观察准备状态，因此以下验收保持未执行：15 秒虚拟屏辅助关闭、物理键鼠对抗、关屏下主动／崩溃恢复、10 分钟后台任务、20 次循环、虚拟目标消失、睡眠唤醒。不能据此交付产品应用。

## 技术来源

- [SetDisplayConfig 参数与返回码](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setdisplayconfig)
- [目标模式联合索引](https://learn.microsoft.com/en-us/windows/win32/api/wingdi/ns-wingdi-displayconfig_path_target_info)
- [RegisterHotKey 与 F12 保留规则](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-registerhotkey)
- [Virtual Display Driver 官方发布](https://github.com/VirtualDrivers/Virtual-Display-Driver/releases/tag/25.7.23)
- [官方安装与卸载说明](https://virtualdrivers-virtual-display-driver.mintlify.app/installation)
- [NefCon v1.20.0](https://github.com/nefarius/nefcon/releases/tag/v1.20.0)
