"""只使用模拟系统调用；运行测试不会改变屏幕状态。"""
import argparse
import os
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
                                  config="unused", log=None, receipt=kw.get("receipt"),
                                  watchdog_seconds=15, parent_crash=kw.get("parent_crash", False),
                                  input_test=False)

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

    def test_validate_keep_off_retries_clone_after_87(self):
        paths = [path(), path(False, ident=2)]
        with patch.object(v, "check_layout"), \
             patch.object(p, "query_topology", return_value=(paths, [])), \
             patch.object(p, "set_display_config", side_effect=[87, 0]) as setter, \
             patch.object(p, "apply_topology_clone", return_value=0) as clone:
            result = v.validate_keep_off(adjust_clone=True)
        self.assertEqual(result["rc"], 0)
        self.assertTrue(result["adjustedClone"])
        clone.assert_called_once()
        self.assertEqual(setter.call_count, 2)
        for call in setter.call_args_list:
            self.assertTrue(call.args[2] & p.SDC_VALIDATE)
            self.assertFalse(call.args[2] & p.SDC_APPLY)

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
                v.start_run("unused", "unused", "timer", -1)
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

    def test_parent_crash_exits_without_waiting_after_arm(self):
        proc = Mock()
        proc.pid = 99
        with tempfile.TemporaryDirectory() as tmp:
            receipt = Path(tmp) / "receipt.json"
            log = Path(tmp) / "off.jsonl"
            v.write_json(receipt, {"ok": True})
            args = self.args(parent_crash=True, receipt=str(receipt))
            args.log = str(log)
            with patch.object(p, "query_topology", return_value=([path(), path(False, ident=2)], [])), \
                 patch.object(p, "set_display_config", return_value=0), \
                 patch.object(v, "verify_receipt"), patch.object(p, "save_topology"), \
                 patch.object(v, "start_run", return_value=proc) as start, \
                 patch.object(v, "wait_result") as wait, patch.object(p, "log_event"), \
                 patch.object(os, "_exit", side_effect=SystemExit(17)) as crash:
                with self.assertRaises(SystemExit):
                    v.guarded_experiment(args, "disable-path")
            crash.assert_called_once_with(17)
            start.assert_called_once()
            wait.assert_not_called()

    def test_hold_allows_zero_seconds_and_passes_parent_pid(self):
        with tempfile.TemporaryDirectory() as tmp:
            directory = Path(tmp) / "run"
            proc = Mock(pid=11)
            with patch.object(v, "spawn", return_value=proc) as spawn, \
                 patch.object(v, "wait_ready"):
                self.assertIs(v.start_run("cfg", directory, "disable-path", 0, parent_pid=22), proc)
            command = spawn.call_args.args[0]
            self.assertIn("--seconds", command)
            self.assertEqual(command[command.index("--seconds") + 1], 0)
            self.assertEqual(command[command.index("--parent-pid") + 1], "22")

    def test_start_run_can_reuse_existing_directory(self):
        with tempfile.TemporaryDirectory() as tmp:
            directory = Path(tmp) / "run"
            directory.mkdir()
            proc = Mock(pid=11)
            with patch.object(v, "spawn", return_value=proc), patch.object(v, "wait_ready"):
                v.start_run("cfg", directory, "disable-path", 0, exist_ok=True)
            with patch.object(v, "spawn") as spawn:
                with self.assertRaises(FileExistsError):
                    v.start_run("cfg", directory, "disable-path", 0)
                spawn.assert_not_called()

    def test_hold_release_restores_and_skips_progress(self):
        with tempfile.TemporaryDirectory() as tmp:
            directory = Path(tmp)
            v.write_json(directory / "arm.json", {"pid": v.os.getpid()})
            v.request_release(directory)
            paths = [path(), path(False, ident=2)]
            off = [path(active=False), path(False, ident=2)]
            args = argparse.Namespace(directory=tmp, config="unused", kind="disable-path",
                                      target="internal", seconds=0, input_test=False, parent_pid=0)
            with patch.object(p, "load_topology", return_value=(paths, [])), \
                 patch.object(v, "current_fingerprint", return_value=v.fingerprint(paths, [])), \
                 patch.object(p.user32, "RegisterHotKey", return_value=1), \
                 patch.object(p.user32, "UnregisterHotKey"), \
                 patch.object(v, "hotkey_received", return_value=False), \
                 patch.object(v, "spawn") as spawn, \
                 patch.object(p, "set_display_config", return_value=0), \
                 patch.object(p, "restore_topology", return_value=0) as restore, \
                 patch.object(p, "monitor_power"), \
                 patch.object(p, "query_topology", return_value=(paths, [])), \
                 patch.object(p, "snapshot_status", return_value={}), \
                 patch.object(p, "log_event"), patch.object(v.time, "sleep"):
                self.assertEqual(v.worker(args), 0)
                spawn.assert_not_called()
                restore.assert_called_once_with("unused")
            result = v.read_json(directory / "result.json")
            self.assertTrue(result["ok"])
            self.assertEqual(result["reason"], "release")

    def test_hold_parent_exit_restores(self):
        with tempfile.TemporaryDirectory() as tmp:
            directory = Path(tmp)
            v.write_json(directory / "arm.json", {"pid": v.os.getpid()})
            paths = [path(), path(False, ident=2)]
            args = argparse.Namespace(directory=tmp, config="unused", kind="disable-path",
                                      target="internal", seconds=0, input_test=False, parent_pid=4242)
            with patch.object(p, "load_topology", return_value=(paths, [])), \
                 patch.object(v, "current_fingerprint", return_value=v.fingerprint(paths, [])), \
                 patch.object(p.user32, "RegisterHotKey", return_value=1), \
                 patch.object(p.user32, "UnregisterHotKey"), \
                 patch.object(v, "hotkey_received", return_value=False), \
                 patch.object(v, "pid_running", return_value=False), \
                 patch.object(p, "set_display_config", return_value=0), \
                 patch.object(p, "restore_topology", return_value=0), \
                 patch.object(p, "monitor_power"), \
                 patch.object(p, "query_topology", return_value=(paths, [])), \
                 patch.object(p, "snapshot_status", return_value={}), \
                 patch.object(p, "log_event"), patch.object(v.time, "sleep"):
                self.assertEqual(v.worker(args), 0)
            self.assertEqual(v.read_json(directory / "result.json")["reason"], "parent-exit")

    def test_successful_preflight_receipt_can_authorize_same_topology(self):
        with tempfile.TemporaryDirectory() as tmp:
            receipt = Path(tmp) / "receipt.json"
            args = argparse.Namespace(config="unused", receipt=str(receipt), log=None, hotkey_seconds=45)

            def start(config, directory, kind, seconds):
                v.write_json(directory / "result.json", {"ok": True, "reason": kind})
                return Mock()

            with patch.object(p, "save_topology"), patch.object(v, "current_fingerprint", return_value="same"), \
                 patch.object(v, "start_run", side_effect=start), \
                 patch.object(v, "wait_result", side_effect=lambda proc, directory, seconds: v.read_json(directory / "result.json")), \
                 patch.object(p, "log_event"):
                self.assertEqual(v.preflight(args), 0)
                self.assertEqual(v.read_json(receipt)["hotkey"], v.HOTKEY)
                v.verify_receipt(receipt)


if __name__ == "__main__":
    unittest.main()
