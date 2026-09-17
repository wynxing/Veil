"""Windows 实机验证工具：校验不关屏；关屏由独立恢复进程执行。"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import time
import uuid
from ctypes import POINTER, byref, c_int32, c_uint32, c_void_p, sizeof
from ctypes.wintypes import MSG

import probe as p

HOTKEY = "Ctrl+Alt+Shift+F10"
WM_HOTKEY = 0x0312
PROFILES = {
    "basic": (p.QDC_ONLY_ACTIVE_PATHS, p.SDC_USE_SUPPLIED_DISPLAY_CONFIG | p.SDC_ALLOW_CHANGES),
    "virtual": (p.QDC_ONLY_ACTIVE_PATHS | p.QDC_VIRTUAL_MODE_AWARE,
                p.SDC_USE_SUPPLIED_DISPLAY_CONFIG | p.SDC_ALLOW_CHANGES | p.SDC_VIRTUAL_MODE_AWARE),
    "refresh": (p.QUERY_FLAGS, p.SET_BASE_FLAGS),
}
p.user32.RegisterHotKey.argtypes = [c_void_p, c_int32, c_uint32, c_uint32]
p.user32.RegisterHotKey.restype = c_int32
p.user32.UnregisterHotKey.argtypes = [c_void_p, c_int32]
p.user32.UnregisterHotKey.restype = c_int32
p.user32.PeekMessageW.argtypes = [POINTER(MSG), c_void_p, c_uint32, c_uint32, c_uint32]
p.user32.PeekMessageW.restype = c_int32


def write_json(path, data):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    temp = path.with_name(path.name + ".tmp")
    temp.write_text(json.dumps(data, ensure_ascii=False, indent=2), encoding="utf-8")
    os.replace(temp, path)


def read_json(path):
    return json.loads(Path(path).read_text(encoding="utf-8"))


def fingerprint(paths, modes):
    return hashlib.sha256(b"".join(bytes(x) for x in [*paths, *modes])).hexdigest()


def current_fingerprint():
    return fingerprint(*p.query_topology())


def check_layout():
    expected = {"LUID": 8, "DISPLAYCONFIG_PATH_SOURCE_INFO": 20,
                "DISPLAYCONFIG_PATH_TARGET_INFO": 48, "DISPLAYCONFIG_PATH_INFO": 72,
                "DISPLAYCONFIG_VIDEO_SIGNAL_INFO": 48, "DISPLAYCONFIG_MODE_INFO": 64}
    actual = {name: sizeof(getattr(p, name)) for name in expected}
    if sizeof(c_void_p) != 8 or actual != expected or p.DISPLAYCONFIG_MODE_INFO.mode.offset != 16:
        raise ValueError(f"unexpected Windows ABI layout: {actual}")
    return actual


def diagnostic(log):
    p.log_event(log, {"event": "abi", "sizes": check_layout()})
    results = []
    for profile, (query_flags, set_flags) in PROFILES.items():
        # 每组重新查询，绝不把虚拟模式的联合索引当作基础模式索引。
        paths, modes = p.query_topology(query_flags)
        changed, count, remaining = p.deactivated_paths(paths, "internal")
        for name, candidates in (("unchanged", paths), ("disable-internal", changed)):
            flags = set_flags | p.SDC_VALIDATE
            raw = p.topology_to_dict(candidates, modes)
            raw["queryFlags"] = query_flags
            rc = p.set_display_config(candidates, modes, flags)
            row = {"event": "diagnostic", "profile": profile, "case": name,
                   "queryFlags": query_flags, "setFlags": flags, "rc": rc,
                   "message": p.win_message(rc), "disabledCount": count,
                   "remainingActive": remaining, "parameters": raw,
                   "indices": [{"flags": x.flags, "sourceRaw": x.sourceInfo.modeInfoIdx,
                                "targetRaw": x.targetInfo.modeInfoIdx} for x in candidates]}
            p.log_event(log, row)
            results.append(row)
    return 0 if all(r["rc"] == 0 for r in results if r["case"] == "unchanged") else 2


def spawn(arguments):
    command = [sys.executable, str(Path(__file__).resolve()), *map(str, arguments)]
    kwargs = dict(stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
                  stderr=subprocess.DEVNULL, close_fds=True)
    flags = p.CREATE_NEW_PROCESS_GROUP | p.CREATE_NO_WINDOW
    try:
        return subprocess.Popen(command, creationflags=flags | p.CREATE_BREAKAWAY_FROM_JOB, **kwargs)
    except OSError:
        return subprocess.Popen(command, creationflags=flags, **kwargs)


def wait_ready(proc, directory, timeout=8):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if proc.poll() is not None:
            raise RuntimeError(f"recovery worker exited before ready: {proc.returncode}; {directory}")
        ready = Path(directory) / "ready.json"
        if ready.exists():
            data = read_json(ready)
            if data.get("pid") != proc.pid or not data.get("hotkeyRegistered"):
                raise RuntimeError("invalid readiness acknowledgement")
            return data
        time.sleep(0.05)
    # 未 arm 的 worker 自行退出，不能在可能已 arm 后杀恢复进程。
    raise RuntimeError(f"recovery worker not ready within {timeout}s")


def start_run(config, directory, kind, seconds, target="internal", input_test=False):
    if not 1 <= seconds <= 900:
        raise ValueError("seconds must be in [1, 900]")
    directory = Path(directory).resolve()
    directory.mkdir(parents=True, exist_ok=False)
    args = ["worker", "--config", str(Path(config).resolve()), "--directory", str(directory),
            "--kind", kind, "--seconds", seconds, "--target", target]
    if input_test:
        args.append("--input-test")
    proc = spawn(args)
    wait_ready(proc, directory)
    write_json(directory / "arm.json", {"pid": proc.pid})
    return proc


def hotkey_received():
    msg = MSG()
    found = False
    while p.user32.PeekMessageW(byref(msg), None, WM_HOTKEY, WM_HOTKEY, 1):
        found |= msg.wParam == 1
    return found


def active_targets(paths):
    return sorted((p.luid_str(x.targetInfo.adapterId), x.targetInfo.id)
                  for x in paths if x.flags & p.DISPLAYCONFIG_PATH_ACTIVE)


def wait_progress_ready(proc, directory):
    deadline = time.monotonic() + 5
    while time.monotonic() < deadline:
        if proc.poll() is not None:
            raise RuntimeError("background workload exited before ready")
        ready = directory / "progress-ready.json"
        if ready.exists() and read_json(ready).get("pid") == proc.pid:
            return
        time.sleep(0.05)
    raise RuntimeError("background workload did not become ready")


def worker(args):
    directory = Path(args.directory)
    log = str(directory / "recovery.jsonl")
    result = {"kind": args.kind, "reason": "not-armed", "restoreRc": None,
              "restoredTargets": False, "restoredTopology": False, "applyRc": None, "ok": False}
    registered = False
    armed = False
    progress = None
    try:
        check_layout()
        paths, modes = p.load_topology(args.config)
        if fingerprint(paths, modes) != current_fingerprint():
            raise RuntimeError("topology changed since save; repeat preflight")
        if not p.user32.RegisterHotKey(None, 1, 0x4007, 0x79):
            raise OSError(p.get_last_error(), "RegisterHotKey failed")
        registered = True
        write_json(directory / "ready.json", {"pid": os.getpid(), "hotkeyRegistered": True,
                                              "hotkey": HOTKEY})
        deadline = time.monotonic() + 10
        while not (directory / "arm.json").exists():
            if hotkey_received():
                result["reason"] = "cancelled-before-arm"
                return 3
            if time.monotonic() >= deadline:
                return 3
            time.sleep(0.05)
        if read_json(directory / "arm.json").get("pid") != os.getpid():
            raise RuntimeError("arm PID mismatch")
        # apply 和恢复在同一个独立进程/消息线程内，防止父进程晚于恢复再次关屏。
        armed = True
        if hotkey_received():
            raise RuntimeError("hotkey received before start; cancelled without switching")
        progress = spawn(["progress", "--directory", directory, "--seconds", args.seconds + 10])
        wait_progress_ready(progress, directory)
        if hotkey_received():
            raise RuntimeError("hotkey received during preparation; cancelled without switching")
        result["applyRc"] = 0
        expected = active_targets(paths)
        if args.kind == "disable-path":
            changed, count, remaining = p.deactivated_paths(paths, args.target)
            if count == 0 or remaining == 0:
                raise RuntimeError("no eligible internal target or no remaining active target")
            rc = p.set_display_config(changed, modes, p.SET_BASE_FLAGS | p.SDC_VALIDATE)
            if rc:
                raise RuntimeError(f"worker validation failed: {rc}")
            expected = active_targets(changed)
            result["applyRc"] = p.set_display_config(changed, modes, p.SET_BASE_FLAGS | p.SDC_APPLY)
        elif args.kind == "temp-off":
            sent, error = p.monitor_power(2)
            result["applyRc"] = 0 if sent else error or 1
        p.log_event(log, {"event": "experiment_started", **result, "seconds": args.seconds})
        if result["applyRc"]:
            raise RuntimeError("apply failed")
        start = previous = time.monotonic()
        poll_at = start
        input_done = False
        result["reason"] = "timer"
        while True:
            now = time.monotonic()
            if now - previous > 3:
                result["reason"] = "execution-gap"
                break
            if hotkey_received():
                result["reason"] = "hotkey"
                break
            if now - start >= args.seconds:
                break
            previous = now
            if args.input_test and not input_done and now - start >= 3:
                p.synthesize_input()
                input_done = True
                p.log_event(log, {"event": "input_synthesized"})
            if now >= poll_at:
                status = p.snapshot_status()
                p.log_event(log, {"event": "state", "elapsed": now - start, "status": status})
                if args.kind == "disable-path" and active_targets(p.query_topology()[0]) != expected:
                    result["reason"] = "unexpected-topology"
                    break
                poll_at = now + 1
            time.sleep(0.05)
        result["elapsedSeconds"] = time.monotonic() - start
    except Exception as exc:
        result["reason"] = "error"
        result["error"] = repr(exc)
    finally:
        if armed:
            try:
                result["restoreRc"] = p.restore_topology(args.config)
                p.monitor_power(-1)
                time.sleep(0.5)
                result["restoredTargets"] = active_targets(p.query_topology()[0]) == active_targets(paths)
                result["restoredTopology"] = current_fingerprint() == fingerprint(paths, modes)
                if result["restoreRc"] or not result["restoredTargets"]:
                    result["fallbackRc"] = p.apply_topology_internal()
                result["statusAfter"] = p.snapshot_status()
            except Exception as exc:
                result["restoreError"] = repr(exc)
        if registered:
            p.user32.UnregisterHotKey(None, 1)
        result["ok"] = (result["reason"] in ("timer", "hotkey") and result["applyRc"] == 0
                        and result["restoreRc"] == 0 and result["restoredTargets"] and result["restoredTopology"])
        write_json(directory / "stop.json", {})
        if progress is not None:
            try:
                progress.wait(timeout=3)
            except subprocess.TimeoutExpired:
                result["progressExitPending"] = True
        p.log_event(log, {"event": "recovery_done", **result})
        write_json(directory / "result.json", result)
    return 0 if result["ok"] else 1


def progress_worker(args):
    directory = Path(args.directory)
    deadline = time.monotonic() + args.seconds
    counter = 0
    digest = bytes(32)
    previous = time.time()
    while time.monotonic() < deadline and not (directory / "stop.json").exists():
        for _ in range(10000):
            digest = hashlib.sha256(digest).digest()
        counter += 10000
        now = time.time()
        p.log_event(str(directory / "progress.jsonl"), {"event": "progress", "iterations": counter,
                    "digest": digest.hex(), "wallGapSeconds": now - previous})
        if counter == 10000:
            write_json(directory / "progress-ready.json", {"pid": os.getpid()})
        previous = now
        time.sleep(1)
    return 0


def wait_result(proc, directory, seconds):
    try:
        proc.wait(timeout=seconds + 15)
    except subprocess.TimeoutExpired:
        raise RuntimeError(f"recovery overdue; do not run another experiment; inspect {directory}")
    return read_json(Path(directory) / "result.json")


def preflight(args):
    p.save_topology(args.config)
    token = current_fingerprint()
    receipt = {"fingerprint": token, "created": time.time(), "hotkey": HOTKEY, "ok": False}
    write_json(args.receipt, receipt)  # 先使旧凭证失效。
    base = Path(args.receipt).resolve().parent
    for kind, seconds in (("timer", 2), ("hotkey", args.hotkey_seconds)):
        directory = base / ("preflight-" + kind + "-" + uuid.uuid4().hex[:8])
        proc = start_run(args.config, directory, kind, seconds)
        p.log_event(args.log, {"event": "preflight_wait", "kind": kind,
                              "seconds": seconds, "hotkey": HOTKEY, "directory": str(directory)})
        result = wait_result(proc, directory, seconds)
        receipt[kind] = {"directory": str(directory), "result": result}
        if not result["ok"] or result["reason"] != kind:
            write_json(args.receipt, receipt)
            return 2
    receipt["ok"] = current_fingerprint() == token
    write_json(args.receipt, receipt)
    return 0 if receipt["ok"] else 2


def verify_receipt(path):
    if not path:
        raise RuntimeError("apply requires --receipt from successful preflight")
    data = read_json(path)
    if (not data.get("ok") or data.get("hotkey") != HOTKEY
            or not 0 <= time.time() - data.get("created", 0) < 3600
            or data.get("fingerprint") != current_fingerprint()):
        raise RuntimeError("preflight missing, expired, or topology changed")
    for kind in ("timer", "hotkey"):
        result = read_json(Path(data[kind]["directory"]) / "result.json")
        if not result.get("ok") or result.get("reason") != kind:
            raise RuntimeError("preflight evidence failed")


def guarded_experiment(args, kind):
    p.require_confirm(args.confirm)
    check_layout()
    paths, modes = p.query_topology()
    if kind == "disable-path":
        changed, count, remaining = p.deactivated_paths(paths, args.target)
        rc = p.set_display_config(changed, modes, p.SET_BASE_FLAGS | p.SDC_VALIDATE)
        p.log_event(args.log, {"event": "disable_path_validate", "rc": rc,
                              "disabledCount": count, "remainingActive": remaining})
        if rc:
            return 2
        if args.validate_only:
            return 0
        if count == 0 or remaining == 0:
            return 3
    verify_receipt(getattr(args, "receipt", None))
    p.save_topology(args.config)
    receipt = read_json(args.receipt)
    receipt["ok"] = False  # 父进程崩溃或结果未落盘时，下次必须重做预检。
    write_json(args.receipt, receipt)
    directory = Path(args.log).resolve().parent / ("run-" + uuid.uuid4().hex[:8])
    try:
        proc = start_run(args.config, directory, kind, args.watchdog_seconds,
                         getattr(args, "target", "internal"), getattr(args, "input_test", False))
        p.log_event(args.log, {"event": "recovery_ready", "pid": proc.pid,
                              "directory": str(directory), "hotkey": HOTKEY})
        result = wait_result(proc, directory, args.watchdog_seconds)
        p.log_event(args.log, {"event": "experiment_result", **result})
        receipt["ok"] = result["ok"]
        write_json(args.receipt, receipt)
        return 0 if result["ok"] else 1
    except Exception:
        data = read_json(args.receipt)
        data["ok"] = False
        write_json(args.receipt, data)
        raise


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    diag = sub.add_parser("diagnose")
    diag.add_argument("--log", required=True)
    pre = sub.add_parser("preflight")
    pre.add_argument("--config", required=True)
    pre.add_argument("--receipt", required=True)
    pre.add_argument("--log", required=True)
    pre.add_argument("--hotkey-seconds", type=int, default=45)
    work = sub.add_parser("worker")
    work.add_argument("--config", required=True)
    work.add_argument("--directory", required=True)
    work.add_argument("--kind", choices=["timer", "hotkey", "temp-off", "disable-path"], required=True)
    work.add_argument("--seconds", type=int, required=True)
    work.add_argument("--target", choices=["internal", "all"], default="internal")
    work.add_argument("--input-test", action="store_true")
    progress = sub.add_parser("progress")
    progress.add_argument("--directory", required=True)
    progress.add_argument("--seconds", type=int, required=True)
    args = parser.parse_args()
    try:
        if args.command == "diagnose":
            return diagnostic(args.log)
        if args.command == "preflight":
            return preflight(args)
        if args.command == "worker":
            return worker(args)
        return progress_worker(args)
    except Exception as exc:
        p.log_event(getattr(args, "log", None), {"event": "error", "error": repr(exc)})
        return 1


if __name__ == "__main__":
    sys.exit(main())
