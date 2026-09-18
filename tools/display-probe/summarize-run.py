"""汇总探针日志；系统检查结果不能代替物理观察。"""
import argparse
import json
from pathlib import Path


def rows(path):
    return [json.loads(line) for line in path.read_text(encoding="utf-8").splitlines() if line.strip()]


def summarize(directory):
    directory = Path(directory)
    recovery = rows(directory / "recovery.jsonl")
    progress = rows(directory / "progress.jsonl") if (directory / "progress.jsonl").exists() else []
    result = json.loads((directory / "result.json").read_text(encoding="utf-8")) if (directory / "result.json").exists() else {}
    states = [row for row in recovery if row["event"] == "state"]
    counters = [row["iterations"] for row in progress]
    return {
        "directory": str(directory.resolve()),
        "completed": bool(result),
        "reason": result.get("reason"),
        "softwareChecksPassed": result.get("ok"),
        "physicalObservation": "requires operator evidence",
        "elapsedSeconds": result.get("elapsedSeconds"),
        "applyRc": result.get("applyRc"),
        "restoreRc": result.get("restoreRc"),
        "restoredTopology": result.get("restoredTopology"),
        "stateSamples": len(states),
        "activeInternalCounts": sorted({row["status"]["activeInternal"] for row in states}),
        "activeAuxiliaryCounts": sorted({row["status"]["activeAuxiliary"] for row in states}),
        "syntheticInputEvents": sum(row["event"] == "input_synthesized" for row in recovery),
        "progressSamples": len(progress),
        "lastIterationCount": counters[-1] if counters else None,
        "progressStrictlyIncreasing": len(counters) > 1 and all(b > a for a, b in zip(counters, counters[1:])),
        "progressWallSpanSeconds": progress[-1]["clocks"]["wallUnix"] - progress[0]["clocks"]["wallUnix"] if progress else None,
        "maxProgressGapSeconds": max((row["wallGapSeconds"] for row in progress), default=None),
    }


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory")
    parser.add_argument("--output")
    args = parser.parse_args()
    text = json.dumps(summarize(args.directory), ensure_ascii=False, indent=2)
    print(text)
    if args.output:
        Path(args.output).write_text(text + "\n", encoding="utf-8")
