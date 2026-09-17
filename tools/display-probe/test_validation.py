"""只使用模拟系统调用；运行测试不会改变屏幕状态。"""
import argparse
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch, Mock

import probe as p
import validation as v


def path(internal=True, active=True, ident=1):
    item = p.DISPLAYCONFIG_PATH_INFO()
    item.flags = (p.DISPLAYCONFIG_PATH_ACTIVE if active else 0) | 8
    item.targetInfo.outputTechnology = p.DISPLAYCONFIG_OUTPUT_TECHNOLOGY_INTERNAL if internal else 5
    item.targetInfo.id = ident
    item.sourceInfo.modeInfoIdx = 0x0001FFFF
    item.targetInfo.modeInfoIdx = 0x00020003
    return item


class ValidationTests(unittest.TestCase):
    def test_abi(self):
        self.assertEqual(v.check_layout()["DISPLAYCONFIG_PATH_INFO"], 72)

    def test_deactivation_preserves_auxiliary_union_and_original(self):
        original = [path(), path(False, ident=2), path(active=False, ident=3)]
        before = [bytes(x) for x in original]
        changed, count, remaining = p.deactivated_paths(original, "internal")
        self.assertEqual((count, remaining), (1, 1))
        self.assertEqual([bytes(x) for x in original], before)
        self.assertFalse(changed[0].flags & 1)
        self.assertEqual(changed[0].flags, 8)
        self.assertEqual(changed[0].sourceInfo.modeInfoIdx, 0x0001FFFF)
        self.assertEqual(bytes(changed[1]), before[1])

    def test_profiles_query_fresh_and_only_validate(self):
        with patch.object(p, "query_topology", return_value=([path()], [])) as query, \
             patch.object(p, "set_display_config", return_value=0) as setter, \
             patch.object(p, "log_event"):
            self.assertEqual(v.diagnostic(None), 0)
        self.assertEqual([x.args[0] for x in query.call_args_list], [2, 18, 82])
        self.assertEqual([x.args[2] for x in setter.call_args_list],
                         [1120, 1120, 33888, 33888, 164960, 164960])
        for call in setter.call_args_list:
            self.assertFalse(call.args[2] & p.SDC_APPLY)

    def args(self, **kw):
        return argparse.Namespace(confirm="off", target="internal", validate_only=kw.get("validate_only", False),
                                  config="unused", log=None, receipt=None, watchdog_seconds=15)

    def test_failed_validation_never_starts_worker(self):
        with patch.object(p, "query_topology", return_value=([path()], [])), \
             patch.object(p, "set_display_config", return_value=87) as setter, \
             patch.object(v, "start_run") as start, patch.object(p, "log_event"):
            self.assertEqual(v.guarded_experiment(self.args(), "disable-path"), 2)
            start.assert_not_called()
            self.assertEqual(setter.call_count, 1)

    def test_zero_paths_can_validate_but_not_apply(self):
        with patch.object(p, "query_topology", return_value=([path()], [])), \
             patch.object(p, "set_display_config", return_value=0), \
             patch.object(v, "start_run") as start, patch.object(p, "log_event"):
            self.assertEqual(v.guarded_experiment(self.args(validate_only=True), "disable-path"), 0)
            self.assertEqual(v.guarded_experiment(self.args(), "disable-path"), 3)
            start.assert_not_called()

    def test_missing_preflight_refuses_temp_off(self):
        with patch.object(p, "query_topology", return_value=([path()], [])), \
             patch.object(p, "monitor_power") as power:
            with self.assertRaisesRegex(RuntimeError, "requires"):
                v.guarded_experiment(self.args(), "temp-off")
            power.assert_not_called()

    def test_unready_worker_never_armed(self):
        with tempfile.TemporaryDirectory() as tmp:
            directory = Path(tmp) / "run"
            with patch.object(v, "spawn", return_value=Mock()), \
                 patch.object(v, "wait_ready", side_effect=RuntimeError("not ready")):
                with self.assertRaises(RuntimeError):
                    v.start_run("unused", directory, "disable-path", 15)
            self.assertFalse((directory / "arm.json").exists())

    def test_hotkey_failure_never_applies(self):
        with tempfile.TemporaryDirectory() as tmp:
            args = argparse.Namespace(directory=tmp, config="unused", kind="disable-path")
            paths = [path()]
            with patch.object(p, "load_topology", return_value=(paths, [])), \
                 patch.object(v, "current_fingerprint", return_value=v.fingerprint(paths, [])), \
                 patch.object(p.user32, "RegisterHotKey", return_value=0), \
                 patch.object(p, "set_display_config") as setter, patch.object(p, "log_event"):
                self.assertEqual(v.worker(args), 1)
                setter.assert_not_called()
            self.assertFalse((Path(tmp) / "ready.json").exists())

    def test_restore_does_not_persist(self):
        with patch.object(p, "load_topology", return_value=([], [])), \
             patch.object(p, "set_display_config", return_value=0) as setter:
            p.restore_topology("unused")
            self.assertTrue(setter.call_args.args[2] & p.SDC_APPLY)
            self.assertFalse(setter.call_args.args[2] & p.SDC_SAVE_TO_DATABASE)

    def test_receipt_expiry_and_topology_change(self):
        with tempfile.TemporaryDirectory() as tmp:
            receipt = Path(tmp) / "receipt.json"
            for created, fingerprint in ((0, "current"), (v.time.time(), "old")):
                v.write_json(receipt, {"ok": True, "created": created, "fingerprint": fingerprint,
                                       "hotkey": v.HOTKEY})
                with patch.object(v, "current_fingerprint", return_value="current"):
                    with self.assertRaises(RuntimeError):
                        v.verify_receipt(receipt)

    def test_invalid_duration_never_spawns(self):
        with patch.object(v, "spawn") as start:
            with self.assertRaises(ValueError):
                v.start_run("unused", "unused", "timer", 0)
            start.assert_not_called()

    def test_apply_failure_always_restores_and_reports_failure(self):
        with tempfile.TemporaryDirectory() as tmp:
            directory = Path(tmp)
            v.write_json(directory / "arm.json", {"pid": v.os.getpid()})
            paths = [path(), path(False, ident=2)]
            args = argparse.Namespace(directory=tmp, config="unused", kind="disable-path",
                                      target="internal", seconds=15, input_test=False)
            with patch.object(p, "load_topology", return_value=(paths, [])), \
                 patch.object(v, "current_fingerprint", return_value=v.fingerprint(paths, [])), \
                 patch.object(p.user32, "RegisterHotKey", return_value=1), \
                 patch.object(p.user32, "UnregisterHotKey"), patch.object(v, "hotkey_received", return_value=False), \
                 patch.object(v, "spawn", return_value=Mock()), \
                 patch.object(v, "wait_progress_ready"), \
                 patch.object(p, "set_display_config", side_effect=[0, 31]) as setter, \
                 patch.object(p, "restore_topology", return_value=0) as restore, \
                 patch.object(p, "monitor_power"), patch.object(p, "query_topology", return_value=(paths, [])), \
                 patch.object(p, "snapshot_status", return_value={}), patch.object(p, "log_event"), \
                 patch.object(v.time, "sleep"):
                self.assertEqual(v.worker(args), 1)
                restore.assert_called_once_with("unused")
                self.assertEqual(setter.call_count, 2)
            result = v.read_json(directory / "result.json")
            self.assertFalse(result["ok"])
            self.assertEqual(result["applyRc"], 31)
            self.assertTrue(result["restoredTopology"])

    def test_dead_progress_process_is_rejected(self):
        with tempfile.TemporaryDirectory() as tmp:
            proc = Mock()
            proc.poll.return_value = 1
            with self.assertRaisesRegex(RuntimeError, "workload exited"):
                v.wait_progress_ready(proc, Path(tmp))


if __name__ == "__main__":
    unittest.main()
