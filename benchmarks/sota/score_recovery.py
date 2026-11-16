#!/usr/bin/env python3
"""The recovery scorer for the head-to-head receipt (rs#527).

Joins the runner outputs (parsanol recovery mode, tree-sitter recovery
mode) against the corruption manifest and emits the per-class table.

Metrics (per corrupted case):
  - diagnostic distance: |reported error position - nearest corruption site|
  - suffix recovery: fraction of the post-corruption suffix the engine
    still parses into valid structure
      * parsanol: deepest-failure position past the site (an upper-bound
        proxy for resumed structure — named as such in receipts)
      * tree-sitter: bytes under non-ERROR nodes past the site

Usage: score_recovery.py <manifest.json> <parsanol-out> <ts-out>
"""
import json
import statistics
import sys
from pathlib import Path


def load_runner_output(path):
    rows = {}
    for line in Path(path).read_text().splitlines():
        parts = line.split()
        if len(parts) < 4:
            continue
        name = parts[0]
        rows[name] = {
            "len": int(parts[1]),
            "pos": int(parts[2]),
            "first_err": int(parts[3]) if len(parts) > 4 else None,
            "ok": len(parts) < 5 or parts[4] == "true",
        }
    return rows


def main():
    manifest = json.loads(Path(sys.argv[1]).read_text())
    parsanol = load_runner_output(sys.argv[2])
    ts = load_runner_output(sys.argv[3])

    per_class = {}
    for case in manifest["cases"]:
        name = case["file"]
        site = min(case["corruption_sites"])
        p = parsanol[name]
        t = ts[name]
        suffix = p["len"] - site
        if suffix <= 0:
            continue
        row = per_class.setdefault(
            (case["class"], case["depth"]),
            {"n": 0, "p_diag": [], "p_suffix": [], "t_diag": [], "t_suffix": [],
             "suffix_defined": True},
        )
        row["n"] += 1
        row["p_diag"].append(abs(p["pos"] - site))
        row["t_diag"].append(abs((t["first_err"] or 0) - site))
        if case["class"] == "truncate":
            # truncation removes the suffix; suffix recovery is undefined
            row["suffix_defined"] = False
            continue
        row["p_suffix"].append(min(max(p["pos"] - site, 0) / suffix, 1.0))
        row["t_suffix"].append(min(max(t["pos"] - site, 0) / suffix, 1.0))

    print("| class | depth | n | parsanol diag± | parsanol suffix | ts diag± | ts suffix |")
    print("|---|---|---|---|---|---|---|")
    for (cls, depth), row in sorted(per_class.items()):
        if row["suffix_defined"]:
            ps = f"{statistics.mean(row['p_suffix']) * 100:.1f}%"
            tsuf = f"{statistics.mean(row['t_suffix']) * 100:.1f}%"
        else:
            ps = tsuf = "—"
        print(
            f"| {cls} | {depth} | {row['n']} "
            f"| {statistics.mean(row['p_diag']):.0f} "
            f"| {ps} "
            f"| {statistics.mean(row['t_diag']):.0f} "
            f"| {tsuf} |"
        )

    all_rows = [r for r in per_class.values() if r["suffix_defined"]]
    p_all = [x for r in all_rows for x in r["p_suffix"]]
    t_all = [x for r in all_rows for x in r["t_suffix"]]
    pd_all = [x for r in all_rows for x in r["p_diag"]]
    td_all = [x for r in all_rows for x in r["t_diag"]]
    print()
    print(
        f"TOTAL: parsanol suffix {statistics.mean(p_all) * 100:.1f}% "
        f"(diag±{statistics.mean(pd_all):.0f}) | "
        f"tree-sitter suffix {statistics.mean(t_all) * 100:.1f}% "
        f"(diag±{statistics.mean(td_all):.0f})"
    )


if __name__ == "__main__":
    main()
