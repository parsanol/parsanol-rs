#!/usr/bin/env python3
"""Benchmark regression gate (rs#463): compare two receipt tables.

Reads the `| benchmark | ns/iter |` rows from an old and a new
benchmarks/latest.md and fails with every benchmark that got more than
--threshold (default 10%) slower. First-run or renamed benchmarks are
new rows — reported, never failures. The discipline: min-of-sample
criterion numbers on one runner class; trends only, never absolute
cross-machine comparisons.
"""
import argparse
import re
import sys

ROW = re.compile(r"^\|\s*(\S+)\s*\|\s*([\d,]+)\s*\|$")


def table(path):
    rows = {}
    with open(path, encoding="utf-8") as handle:
        for line in handle:
            match = ROW.match(line.strip())
            if match:
                rows[match.group(1)] = int(match.group(2).replace(",", ""))
    return rows


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("old")
    parser.add_argument("new")
    parser.add_argument("--threshold", type=float, default=10.0)
    args = parser.parse_args()

    old_rows = table(args.old)
    new_rows = table(args.new)
    if not old_rows:
        print(f"no rows in {args.old} — first receipt, nothing to gate")
        return 0

    regressions = []
    new_rows_report = []
    for name, old_ns in sorted(old_rows.items()):
        if name not in new_rows:
            new_rows_report.append(f"dropped: {name}")
            continue
        new_ns = new_rows[name]
        delta = (new_ns - old_ns) / old_ns * 100.0
        if delta > args.threshold:
            regressions.append(
                f"REGRESSION {name}: {old_ns} -> {new_ns} ns (+{delta:.1f}%)"
            )
    for name in new_rows:
        if name not in old_rows:
            new_rows_report.append(f"new: {name}")

    for line in new_rows_report:
        print(line)
    if regressions:
        print()
        for line in regressions:
            print(line)
        print(
            f"\n{len(regressions)} benchmark(s) regressed more than "
            f"{args.threshold:.0f}% — investigate before merging"
        )
        return 1
    print(f"no regressions over {args.threshold:.0f}% ({len(old_rows)} benchmarks compared)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
