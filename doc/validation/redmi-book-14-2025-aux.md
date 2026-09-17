# 验证结果：REDMI Book 14 2025 虚拟屏辅助

> 历史实验记录。文中“不支持”仅指当时已测调用，不能泛化为硬件或全部机制不可能。当前结论以[第二轮复核](redmi-book-14-2025-revalidation.md)为准。

状态：本轮已记录；未获得第二目标，未 apply  
日期：2026-09-17

## 环境

与 [redmi-book-14-2025-internal.md](redmi-book-14-2025-internal.md) 相同。额外检查：

| 项 | 记录 |
| --- | --- |
| 活动 CCD 路径 | 1（内屏 TM140VDXP02，`role=internal`） |
| activeInternal / activeAuxiliary | 1 / 0 |
| 外接屏 / HDMI 假插头 | 未接入 |
| 虚拟显示适配器 | 未检测到（无 VDD / IddCx / Parsec） |
| 探针 | `disable-path --target internal`，拒绝在剩余活动路径为 0 时 VALIDATE/APPLY |

未安装第三方虚拟显示驱动。探针不负责装驱动；本轮也未启用测试签名。

## 本轮结论

- **第二目标**：不存在。CCD 假设（留下一块辅助目标后再停内屏）无法在本机当前库存上检验。
- **只停内屏**：探针在 `remainingActive=0` 时退出码 3，未调用 `SetDisplayConfig`。这是安全门，不是保持关闭成立。
- **apply / 键鼠对抗 / 待机观察**：未执行。未通过「第二路径存在且 VALIDATE 成功」的门槛。
- **保持关闭（仅内屏、依赖辅助屏）**：未验证。不得写成支持，也不得用黑窗或 `SC_MONITORPOWER` 冒充。
- **最小应用**：不进入。

要继续这条候选，需要机旁接入假插头/外接屏，或由用户在 UAC 下安装已签名的 Virtual Display Driver，然后再跑 VALIDATE 与 15 秒看门狗 apply。

## 结果表

| 步骤 | 机制 | API 结果 | 物理观察 | 输入是否唤醒 | 恢复是否成功 | 结论 |
| --- | --- | --- | --- | --- | --- | --- |
| 0 | 枚举（含 role） | 1 条活动内屏；`activeAuxiliary=0` | 仅内屏点亮 | 不适用 | 亮屏 restore 返回 0 | 第二目标不存在 |
| 1 | `disable-path --target internal --validate-only` | 未 VALIDATE；`disabledCount=1`，`remainingActive=0`，退出 3 | 无关屏 | 不适用 | 不适用 | 安全拒绝，未 apply |
| 2 | 看门狗下 apply | 未执行 | 未执行 | 未执行 | 未执行 | 未执行 |
| 3 | 键鼠 vs 保持关闭 | 未执行 | 未执行 | 未执行 | 未执行 | 未执行 |

## 摘录

```text
event=save pathCount=1
event=restore rc=0
event=disable_path_plan target=internal disabledCount=1 remainingActive=0 validateOnly=true
event=disable_path_skipped_apply reason=no remaining active path after disabling internal; auxiliary target required
```

原始 JSONL 与拓扑字节不提交仓库。
