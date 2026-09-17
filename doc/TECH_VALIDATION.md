# Veil 技术验证协议

版本：0.2  
状态：仅内屏第一组失败后，虚拟屏辅助未获得第二目标  
日期：2026-09-17

本文是验证协议，不是已验证支持列表。产品要求见 [PRD.md](PRD.md)。没有实测证据时，不得把候选机制写成已完成能力。

## 1. 本轮范围

硬件：XIAOMI REDMI Book 14 2025(FHD+)，Windows 11 家庭中文版内部版本 26200，Intel UHD Graphics 驱动 32.0.101.6733，内置面板 TM140VDXP02（`DISPLAY\TMA0813`），1920×1200。当前无外接屏、无虚拟显示适配器。

要回答：

- 能否枚举这块内屏，并排除 `DEFAULT_MONITOR` 等占位设备
- 临时关闭候选（`SC_MONITORPOWER`）是否让屏幕不再显示桌面内容，以及普通输入会不会唤醒
- 保持关闭候选（CCD `SetDisplayConfig` 停用路径）在最后一块物理屏上是否允许、是否停住桌面输出、键鼠会不会自行恢复、主动恢复是否成立

本轮不做：最小应用、托盘、防睡眠、外接拓扑、虚拟显示器、20 次循环验收。不使用黑色覆盖层。

## 2. 安全规则

关唯一内屏前必须同时满足：

1. 当前显示拓扑已保存到文件
2. 独立看门狗进程已启动（父进程退出或崩溃后仍须能恢复）
3. 已在屏幕仍亮时证明「保存 → 再应用」可以跑通
4. 控制台已打印恢复时限，并使用 `--confirm off` 明确确认

第一次 CCD 关闭看门狗为 **20 秒**。看门狗恢复失败时的人工兜底（不是产品能力）：

1. `Win+Ctrl+Shift+B` 重启显卡驱动
2. 仍无画面则重启系统

任一机制无法可靠恢复时，立即停止后续关屏实验，只保留失败记录。

## 3. 候选机制

| 机制 | 接口 | 产品意图 | 已知限制 |
| --- | --- | --- | --- |
| 临时关闭 | `SendNotifyMessageW(HWND_BROADCAST, WM_SYSCOMMAND, SC_MONITORPOWER, 2)` | 一次熄屏，允许输入唤醒 | 通常作用于全部显示器，不是按屏保持关闭；`SendMessageTimeout` 广播会长时间阻塞 |
| 保持关闭 | CCD `QueryDisplayConfig` / `SetDisplayConfig` 清除指定路径的 `DISPLAYCONFIG_PATH_ACTIVE` | 停用桌面输出直到主动恢复 | 停用全部活动路径会被拒绝；只停内屏须留下辅助目标 |
| 虚拟/外接辅助 | 第二块活动目标 + 只停内屏 | 检验「非最后一块物理屏」时保持关闭 | 虚拟屏单独枚举，不计入物理支持；不自研驱动、不用 Parsec |
| 覆盖层 | 全屏黑窗 | 禁止 | 屏幕可能仍亮且仍参与桌面 |

虚拟显示器在仅内屏闭环失败后单独评估。本机当前仍无第二目标时，`disable-path --target internal` 必须拒绝 apply。

## 4. 判定

系统状态检查与物理观察必须同时记录。API 返回成功不能单独作为通过。

| 结论 | 条件 |
| --- | --- |
| 临时关闭候选成立 | 一次关闭后不显示桌面内容；普通输入可唤醒；探针不会立刻再次关闭 |
| 保持关闭候选成立 | 停用后持续不显示桌面内容；普通输入不解除；看门狗或显式 restore 后内屏重新可用 |
| 不支持 | API 拒绝、无法停住输出、输入会破坏保持关闭、或恢复不可靠 |
| 未完成 | 未执行，或缺少物理观察且系统状态无法排除失败 |

记录项：是否停止桌面输出、面板是否看起来熄灭或仍发光、是否短暂亮屏、是否影响其他显示器（本轮无外接则记「无其他物理屏」）。

## 5. 实验顺序

使用 [tools/display-probe](../tools/display-probe/README.md)。日志必须写文件。

1. **安全空跑（不关屏）**：`enumerate`、`save`、亮屏 `restore`、短时看门狗对已保存配置 `apply`
2. **临时关闭**：看门狗保护下 `temp-off`；观察画面与键鼠唤醒
3. **CCD 停用最后路径**：看门狗 20 秒；`disable-path`；记录 API、画面、键鼠、看门狗恢复
4. **仅当第 3 步确实停住桌面输出**：在看门狗窗口内主动打键鼠；若因此恢复，该机制不能承担保持关闭
5. **填写结果表**： [validation/redmi-book-14-2025-internal.md](validation/redmi-book-14-2025-internal.md)

本轮实测摘要见该结果表。`SC_MONITORPOWER` 在本机导致睡眠感黑屏，且可能把系统带进待机，不能当作后台继续运行的关屏。CCD 停用最后一块物理路径返回 87，保持关闭不支持。

## 5b. 虚拟屏辅助

假设：留下一块非内屏活动路径后，可以只停内屏。安全规则额外要求：`remainingActive >= 1`，否则不 VALIDATE/APPLY。第一次 apply 看门狗 **15 秒**。

结果见 [validation/redmi-book-14-2025-aux.md](validation/redmi-book-14-2025-aux.md)。2026-09-17：无外接、无假插头、无已装 VDD；探针退出码 3，未 apply。

## 6. 探针命令

工作目录为仓库根或 `tools/display-probe`。Python 3.12，仅标准库。

```text
python tools/display-probe/probe.py enumerate
python tools/display-probe/probe.py save --config <file>
python tools/display-probe/probe.py restore --config <file>
python tools/display-probe/probe.py watchdog --config <file> --seconds N --log <file>
python tools/display-probe/probe.py temp-off --config <file> --watchdog-seconds 20 --confirm off --log <file>
python tools/display-probe/probe.py disable-path --config <file> --watchdog-seconds 15 --confirm off --target internal --log <file>
python tools/display-probe/probe.py disable-path --config <file> --confirm off --target internal --validate-only --log <file>
```

关屏命令默认先拉起分离看门狗。`--confirm off` 是唯一接受的确认词。`temp-off` 使用 `SendNotifyMessageW`，避免广播 `SendMessageTimeout` 阻塞。`--target internal` 只停内屏；没有剩余活动路径时拒绝 apply。日志含墙钟与 monotonic，用于判断待机。
