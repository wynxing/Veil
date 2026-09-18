"""保持关闭门禁与结束态文案：只根据快照和 worker 结果判断，不改变显示。"""

DEFAULT_HOTKEY = "Ctrl+Alt+Shift+F10"
INTERRUPT_REASONS = ("execution-gap", "unexpected-topology")


def active_roles(status):
    return [row["role"] for row in status.get("paths", []) if row.get("active")]


def keep_off_block_reason(status):
    roles = active_roles(status)
    if roles.count("internal") < 1:
        return "当前没有活动的内屏路径。"
    if roles.count("virtual") + roles.count("external") < 1:
        return "没有第二活动目标（虚拟屏或实体外接），无法停用最后一条物理路径。"
    return None


def physical_paths(status):
    return [row for row in status.get("paths", []) if row.get("role") in ("internal", "external")]


def virtual_paths(status):
    return [row for row in status.get("paths", []) if row.get("role") == "virtual"]


def auxiliary_listing_note(status):
    virtual = virtual_paths(status)
    if virtual:
        return f"虚拟辅助目标 {len(virtual)} 个（不列入物理屏，不能对它关屏）"
    if "external" in active_roles(status):
        return "当前第二目标是实体外接，未使用虚拟屏"
    return "未检测到活动虚拟目标或实体外接"


def describe_hold_result(result, status=None, hotkey=DEFAULT_HOTKEY):
    """worker 结束后的界面状态。保持关闭要求已经结束，不跨睡眠自动再关。"""
    status = status or result.get("statusAfter") or {}
    reason = result.get("reason")
    internal_on = (status.get("activeInternal") or 0) >= 1
    wanted = "开启"
    if result.get("ok"):
        detail = {
            "release": "已恢复关屏前拓扑。",
            "hotkey": f"已由 {hotkey} 恢复。",
            "parent-exit": "主进程退出后已恢复显示。",
        }.get(reason, "已恢复显示。")
        return wanted, "已确认", detail
    if reason in INTERRUPT_REASONS:
        if internal_on:
            if reason == "execution-gap":
                detail = "保持关闭已结束：会话中断（常见于睡眠唤醒）。已重新识别屏幕，未自动再关。"
            else:
                detail = "保持关闭已结束：显示拓扑变化。已重新识别屏幕，未自动再关。"
            if result.get("fallbackRc") == 0 and not result.get("restoredTopology"):
                detail += " 已点亮内屏。"
            return wanted, "已确认", detail
        return wanted, "未知", (
            f"保持关闭已结束（{reason}）。请检查画面，必要时 Win+Ctrl+Shift+B。"
        )
    detail = f"保持关闭结束（{reason}）。"
    if result.get("fallbackRc") == 0:
        detail += " 已点亮内屏。"
    else:
        detail += " 请检查画面，必要时 Win+Ctrl+Shift+B。"
    return wanted, "失败", detail
