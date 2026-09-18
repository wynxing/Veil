"""门禁与列表规则；不调用真实显示 API。"""
import unittest

from capability import (
    describe_hold_result,
    keep_off_block_reason,
    physical_paths,
    virtual_paths,
)


def status(*rows, auxiliary=None):
    roles = [row["role"] for row in rows if row.get("active")]
    aux = auxiliary if auxiliary is not None else roles.count("virtual") + roles.count("external")
    return {
        "activeInternal": roles.count("internal"),
        "activeAuxiliary": aux,
        "paths": list(rows),
    }


class CapabilityTests(unittest.TestCase):
    def test_missing_virtual_blocks_keep_off(self):
        data = status({"role": "internal", "active": True, "monitorName": "Panel"})
        self.assertIn("虚拟目标", keep_off_block_reason(data))
        self.assertEqual(len(physical_paths(data)), 1)
        self.assertEqual(virtual_paths(data), [])

    def test_virtual_present_allows_internal_keep_off(self):
        data = status(
            {"role": "internal", "active": True, "monitorName": "Panel"},
            {"role": "virtual", "active": True, "monitorName": "VDD"},
        )
        self.assertIsNone(keep_off_block_reason(data))
        self.assertEqual([row["role"] for row in physical_paths(data)], ["internal"])
        self.assertEqual(len(virtual_paths(data)), 1)

    def test_virtual_not_listed_as_physical(self):
        data = status(
            {"role": "internal", "active": True},
            {"role": "virtual", "active": True},
            {"role": "external", "active": True},
        )
        self.assertEqual([row["role"] for row in physical_paths(data)], ["internal", "external"])

    def test_inactive_virtual_does_not_count(self):
        data = status(
            {"role": "internal", "active": True},
            {"role": "virtual", "active": False},
        )
        self.assertIsNotNone(keep_off_block_reason(data))

    def test_execution_gap_with_internal_on_is_confirmed_not_unknown(self):
        result = {
            "ok": False,
            "reason": "execution-gap",
            "restoreRc": 0,
            "restoredTargets": True,
            "restoredTopology": True,
        }
        on = status(
            {"role": "internal", "active": True},
            {"role": "virtual", "active": True},
        )
        wanted, confirmed, detail = describe_hold_result(result, on)
        self.assertEqual(wanted, "开启")
        self.assertEqual(confirmed, "已确认")
        self.assertIn("未自动再关", detail)
        self.assertNotEqual(confirmed, "未知")

    def test_execution_gap_without_internal_stays_unknown(self):
        result = {"ok": False, "reason": "execution-gap"}
        off = status({"role": "virtual", "active": True})
        wanted, confirmed, detail = describe_hold_result(result, off)
        self.assertEqual(wanted, "开启")
        self.assertEqual(confirmed, "未知")
        self.assertIn("execution-gap", detail)

    def test_hotkey_success_still_confirmed(self):
        result = {"ok": True, "reason": "hotkey"}
        wanted, confirmed, detail = describe_hold_result(result, status(
            {"role": "internal", "active": True},
            {"role": "virtual", "active": True},
        ))
        self.assertEqual((wanted, confirmed), ("开启", "已确认"))
        self.assertIn("F10", detail)

    def test_unexpected_topology_with_fallback_internal_on(self):
        result = {
            "ok": False,
            "reason": "unexpected-topology",
            "restoreRc": 87,
            "restoredTopology": False,
            "fallbackRc": 0,
        }
        on = status({"role": "internal", "active": True})
        wanted, confirmed, detail = describe_hold_result(result, on)
        self.assertEqual((wanted, confirmed), ("开启", "已确认"))
        self.assertIn("拓扑变化", detail)
        self.assertIn("已点亮内屏", detail)


if __name__ == "__main__":
    unittest.main()
