"""20 次以内的受保护关屏循环；快捷键恢复或任何失败都会终止整组。"""
import argparse
import json
from pathlib import Path
import time

import probe
import validation


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--config", required=True)
    parser.add_argument("--receipt", required=True)
    parser.add_argument("--directory", required=True)
    parser.add_argument("--count", type=int, default=20)
    parser.add_argument("--seconds", type=int, default=15)
    parser.add_argument("--confirm", required=True)
    args = parser.parse_args()
    probe.require_confirm(args.confirm)
    if not 1 <= args.count <= 20 or not 1 <= args.seconds <= 15:
        parser.error("count must be 1..20; seconds must be 1..15")
    directory = Path(args.directory).resolve()
    directory.mkdir(parents=True, exist_ok=False)
    manifest = {"requested": args.count, "runs": [], "completed": False}
    for index in range(1, args.count + 1):
        log = directory / f"cycle-{index:02}.jsonl"
        probe.log_event(str(directory / "cycles.jsonl"), {"event": "cycle_start", "cycle": index})
        command = argparse.Namespace(config=args.config, receipt=args.receipt, log=str(log),
                                     confirm=args.confirm, target="internal", validate_only=False,
                                     input_test=True, watchdog_seconds=args.seconds)
        try:
            rc = validation.guarded_experiment(command, "disable-path")
            events = [json.loads(line) for line in log.read_text(encoding="utf-8").splitlines()]
            results = [row for row in events if row["event"] == "experiment_result"]
            result = results[-1] if results else {}
            run = next((row["directory"] for row in events if row["event"] == "recovery_ready"), None)
            entry = {"cycle": index, "rc": rc, "reason": result.get("reason"),
                     "ok": result.get("ok", False), "directory": run}
        except Exception as exc:
            entry = {"cycle": index, "ok": False, "error": repr(exc)}
        manifest["runs"].append(entry)
        validation.write_json(directory / "manifest.json", manifest)
        probe.log_event(str(directory / "cycles.jsonl"), {"event": "cycle_done", **entry})
        if not entry["ok"] or entry.get("reason") != "timer":
            # 用户主动恢复意味着整组取消，不能在几秒后再次关屏。
            return 1
        if index < args.count:
            time.sleep(5)
    manifest["completed"] = True
    validation.write_json(directory / "manifest.json", manifest)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
