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

from gt_spans import value_spans


def load_runner_output(path):
    rows = {}
    for line in Path(path).read_text().splitlines():
        parts = line.split()
        if len(parts) < 4:
            continue
        name = parts[0]
        if parts[3] in ("true", "false"):
            # parsanol rows: name len covered ok [offsets]; ok rows carry
            # covered=0, fail rows covered=pos, first_err in parts[4]
            ok = parts[3] == "true"
            rows[name] = {
                "len": int(parts[1]),
                "pos": int(parts[2]) if ok else int(parts[2]),
                "first_err": int(parts[4]) if len(parts) > 4 and not ok else None,
                "ok": ok,
                "offsets": (
                    sorted(int(x) for x in parts[5].split(","))
                    if len(parts) > 5
                    else []
                ),
            }
        else:
            # ts fail rows: name len covered first_err false offsets
            rows[name] = {
                "len": int(parts[1]),
                "pos": int(parts[2]),
                "first_err": int(parts[3]),
                "ok": False,
                "offsets": (
                    sorted(int(x) for x in parts[5].split(","))
                    if len(parts) > 5
                    else []
                ),
            }
    return rows


CORPUS_DIR = None  # set in main: the clean corpus directory


def gt_offsets(source_name):
    doc = (CORPUS_DIR / source_name).read_text()
    return set(value_spans(doc))


def main():
    manifest = json.loads(Path(sys.argv[1]).read_text())
    global CORPUS_DIR
    CORPUS_DIR = Path(sys.argv[1]).parent.parent / "json"
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
             "p_recall": [], "t_recall": [], "suffix_defined": True},
        )
        row["n"] += 1
        # parsanol's diagnostic is the failure offset (covered is
        # consumed bytes, a different quantity)
        row["p_diag"].append(abs((p["first_err"] if p["first_err"] is not None else p["pos"]) - site))
        row["t_diag"].append(abs((t["first_err"] or 0) - site))
        # Structure recall (the TED-precursor family): recovered node
        # offsets past the site that match a ground-truth value span
        # within the corruption's length delta, over the ground-truth
        # suffix spans.
        gt = gt_offsets(case["source"])
        delta = abs(p["len"] - len((CORPUS_DIR / case["source"]).read_text()))
        tol = max(1, delta + 2)
        gt_suffix = {g for g in gt if g >= site}
        p_past = [o for o in p["offsets"] if o >= site]
        t_past = [o for o in t["offsets"] if o >= site]
        p_matched = sum(1 for o in p_past if any(abs(o - g) <= tol for g in gt_suffix))
        t_matched = sum(1 for o in t_past if any(abs(o - g) <= tol for g in gt_suffix))
        if p_past:
            row["p_recall"].append(p_matched / len(p_past))
        if t_past:
            row["t_recall"].append(t_matched / len(t_past))
        if case["class"] == "truncate":
            # truncation removes the suffix; suffix recovery is undefined
            row["suffix_defined"] = False
            continue
        row["p_suffix"].append(min(max(p["pos"] - site, 0) / suffix, 1.0))
        row["t_suffix"].append(min(max(t["pos"] - site, 0) / suffix, 1.0))

    print("| class | depth | n | parsanol diag± | parsanol suffix | parsanol recall | ts diag± | ts suffix | ts recall |")
    print("|---|---|---|---|---|---|---|---|---|")
    for (cls, depth), row in sorted(per_class.items()):
        if row["suffix_defined"]:
            ps = f"{statistics.mean(row['p_suffix']) * 100:.1f}%"
            tsuf = f"{statistics.mean(row['t_suffix']) * 100:.1f}%"
        else:
            ps = tsuf = "—"
        pr = f"{statistics.mean(row['p_recall']) * 100:.1f}%" if row["p_recall"] else "—"
        tr = f"{statistics.mean(row['t_recall']) * 100:.1f}%" if row["t_recall"] else "—"
        print(
            f"| {cls} | {depth} | {row['n']} "
            f"| {statistics.mean(row['p_diag']):.0f} "
            f"| {ps} "
            f"| {pr} "
            f"| {statistics.mean(row['t_diag']):.0f} "
            f"| {tsuf} "
            f"| {tr} |"
        )

    all_rows = [r for r in per_class.values() if r["suffix_defined"]]
    p_all = [x for r in all_rows for x in r["p_suffix"]]
    t_all = [x for r in all_rows for x in r["t_suffix"]]
    pd_all = [x for r in all_rows for x in r["p_diag"]]
    td_all = [x for r in all_rows for x in r["t_diag"]]
    pr_all = [x for r in all_rows for x in r["p_recall"]]
    tr_all = [x for r in all_rows for x in r["t_recall"]]
    print()
    print(
        f"TOTAL: parsanol suffix {statistics.mean(p_all) * 100:.1f}% "
        f"(diag±{statistics.mean(pd_all):.0f}, recall "
        + (f"{statistics.mean(pr_all) * 100:.1f}% over {len(pr_all)} node-samples)" if pr_all else "none recovered — non-spine grammars produce no partial structure (rs#539)")
        + ") | "
        f"tree-sitter suffix {statistics.mean(t_all) * 100:.1f}% "
        f"(diag±{statistics.mean(td_all):.0f}, recall {statistics.mean(tr_all) * 100:.1f}%"
        + (f" over {len(tr_all)} cases)" if tr_all else ")")
    )


if __name__ == "__main__":
    main()
