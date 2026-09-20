# 可靠性修复验收（协议 v2）

日期：2026-09-20。状态：代码与离线验证；本轮没有运行真实关屏、安装或卸载。不继承此前 Rust 短时记录为本版本验收。

## 行为与协议

- `session.json` 必须包含 `protocolVersion=2`、启用前活动物理屏身份、VDD 启用责任；`baseline.json` 是启用前恢复基线，`topology.json` 是启用后的握手快照。
- `intent.json`、`release.json` 增加递增 `requestId`；心跳包含 `processedRequestId` 与 `waiting/holding/restoring/restore-failed/finished` 状态。同一请求不重复 APPLY。控制文件写入失败向上传播，JSON 同目录临时文件通过 Windows 原子替换提交，最多尝试 8 次。
- `result.json` 增加 `protocolVersion` 和 `restoreState`：`complete/partial/unknown/not-needed`。`ok` 保留为操作诊断；正常退出、卸载使用恢复状态，不依赖提示文字。无法辨认的旧结果保持未知，不能据此移除驱动。
- 恢复先回放基线，有界枚举；必要时内屏兜底后再次枚举。轮询暂停总预算不超过 3 秒（系统 API 自身耗时不在暂停预算内）。所有仍连接的原活动物理屏输出恢复且至少存在一个活动物理屏才确认完成；布局差异只警告。连接枚举失败保持未知。
- 失败后清除关闭要求；恢复失败时不再自动 APPLY，保留热键与恢复全部。原恢复进程已死时，通过同一会话目录的恢复专用进程重试，不消费关闭意图。
- VDD 清理只处理本次启用责任；必须再次确认存在活动物理屏。清理失败保留上下文，轮询不重复弹 UAC，用户明确重试才再尝试。
- 卸载先开启维护门禁，再检查用户会话和所有待恢复会话；任何失败阻止驱动移除。门禁在提交或回滚阶段释放。

## 离线验证

运行 `cargo test --manifest-path src/Cargo.toml --workspace` 和 `cargo check --manifest-path src/Cargo.toml --workspace --all-targets`。测试使用 Fake CCD，不调用真实 `SetDisplayConfig`。

覆盖：失败请求多轮去重；首次/追加/单屏恢复/克隆/醒后再关失败；恢复失败后热键和新请求重试；父进程退出；恢复专用入口；延迟枚举、拔出和布局差异；部分 APPLY 失败；损坏/旧协议结果；原子写入锁冲突；恢复超时；VDD 所有结束原因的最后输出保护；预先活动 VDD 不自动认领；清理失败不循环提权；维护状态的保守门禁。

当前本地：96 项离线测试通过，全目标编译通过。安装包构建通过（WiX MSI 与 Bundle 均为 0 警告、0 错误）；没有执行安装器。远端 CI 尚未运行。

CI 在 Windows PR 和 main 提交上运行离线测试、全目标编译与安装包构建；PR 不发布 Release。安装包构建验证 payload 哈希、签名、WiX 编译，不启动安装器。

## 机旁待验收

以下均未在本轮执行，必须记录系统检查与物理观察：

| 环境/路径 | 待执行 |
| --- | --- |
| REDMI | 新安装的设备所有权、面板点选、短时恢复/热键、10 分钟、20 次循环、睡醒 |
| P15 | 不装 VDD，内外屏分别关闭/恢复、面板、循环、睡醒 |
| 故障 | App 崩溃、Recovery 崩溃、恢复失败后的显式重试、显示布局变化 |
| 安装/卸载 | 默认禁用失败、恢复失败/超时阻止卸载、提交/回滚门禁、旧安装升级、其它用户登录 |

本轮不宣称全硬件兼容、无闪屏、安装升级闭环或可公开安装。

## 安装执行上下文

MSI 的系统动作不直接把 Session 0 当成交互桌面。嵌入助手确认只有一个登录用户后，用该会话的主令牌和用户环境启动固定安装路径的 `Veil.App --restore-and-exit`，等待退出码，并再次检查会话集合。无用户、多用户、令牌/环境创建失败均阻止卸载；不接收任意可执行文件参数。`--restore-and-exit` 忽略已结束的历史会话（含缺 `protocolVersion` 的 v1 `result.json`）和没有存活恢复进程的目录；只等待仍在跑的恢复进程。升级时 `sweep-sessions` 在拆旧产品前清掉这类目录，避免 0.1.5 及更早的恢复入口被历史文件拦住。此实现已编译，仍需机旁验证 SYSTEM 到用户会话的真实调用。

接口依据：[WTSQueryUserToken](https://learn.microsoft.com/en-us/windows/win32/api/wtsapi32/nf-wtsapi32-wtsqueryusertoken)、[CreateProcessAsUserW](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-createprocessasuserw)。前者要求 LocalSystem 及相应权限，后者使用指定用户令牌及会话。
