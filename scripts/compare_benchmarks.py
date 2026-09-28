#!/usr/bin/env python3
"""Compare two exported benchmark snapshots, optionally enforcing a mean budget."""

import argparse
import json
from pathlib import Path
import sys


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("before", type=Path)
    parser.add_argument("after", type=Path)
    parser.add_argument("--max-regression", type=float, help="fail above this percent increase in mean time")
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args()
    before = json.loads((args.before / "results.json").read_text())["benchmarks"]
    after = json.loads((args.after / "results.json").read_text())["benchmarks"]
    old_meta = json.loads((args.before / "metadata.json").read_text())
    new_meta = json.loads((args.after / "metadata.json").read_text())
    mismatches = [key for key in ("rustc", "platform", "cpu", "rustflags") if old_meta[key] != new_meta[key]]
    comparisons = []
    for name in sorted(before.keys() & after.keys()):
        old = before[name]["mean_ns"]
        new = after[name]["mean_ns"]
        comparisons.append({"name": name, "before_ns": old, "after_ns": new, "change_percent": (new / old - 1) * 100})
    if not comparisons:
        raise SystemExit("Snapshots contain no matching benchmark IDs")
    failed = args.max_regression is not None and any(row["change_percent"] > args.max_regression for row in comparisons)
    report = {"schema_version": 1, "ok": not failed, "metadata_mismatches": mismatches,
              "missing_after": sorted(before.keys() - after.keys()), "added_after": sorted(after.keys() - before.keys()),
              "comparisons": comparisons}
    if args.json:
        print(json.dumps(report))
    else:
        if mismatches:
            print("Different comparison conditions: " + ", ".join(mismatches), file=sys.stderr)
        for row in comparisons:
            print(f'{row["name"]:40} {row["before_ns"]:12.1f} → {row["after_ns"]:12.1f} ns  {row["change_percent"]:+7.2f}%')
    return int(failed)


if __name__ == "__main__":
    raise SystemExit(main())
