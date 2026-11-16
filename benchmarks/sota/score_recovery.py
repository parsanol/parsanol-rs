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

from gt_spans import value_span_pairs


def load_runner_output(path):
    rows = {}
    for line in Path(path).read_text().splitlines():
        parts = line.split()
        if len(parts) < 4:
            continue
        name = parts[0]
        # offsets ride as `start:end` byte spans (or bare starts from
        # older receipts — parsed as zero-length spans).
        raw = parts[5].split(",") if len(parts) > 5 and parts[5] else []
        spans = []
        for token in raw:
            if ":" in token:
                a, b = token.split(":")
                spans.append((int(a), int(b)))
            elif token:
                spans.append((int(token), int(token)))

        if parts[3] in ("true", "false"):
            # parsanol rows: name len covered ok [spans]; ok rows carry
            # covered=0, fail rows covered=pos, first_err in parts[4]
            ok = parts[3] == "true"
            rows[name] = {
                "len": int(parts[1]),
                "pos": int(parts[2]),
                "first_err": int(parts[4]) if len(parts) > 4 and not ok else None,
                "ok": ok,
                "spans": spans,
            }
        else:
            # ts fail rows: name len covered first_err false offsets
            rows[name] = {
                "len": int(parts[1]),
                "pos": int(parts[2]),
                "first_err": int(parts[3]),
                "ok": False,
                "spans": spans,
            }
    return rows


CORPUS_DIR = None  # set in main: the clean corpus directory


def gt_spans_of(source_name):
    doc = (CORPUS_DIR / source_name).read_text()
    return value_span_pairs(doc)


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
        # SPANS past the site, matched by containment — a recovered
        # span counts when it covers a ground-truth value span within
        # the corruption's length delta. Both engines are scored on
        # the same rule (their runners emit start:end pairs).
        gt = gt_spans_of(case["source"])
        delta = abs(p["len"] - len((CORPUS_DIR / case["source"]).read_text()))
        tol = max(1, delta + 2)
        gt_suffix = [(gs, ge) for gs, ge in gt if gs >= site]
        p_past = [sp for sp in p["spans"] if sp[0] >= site]
        t_past = [sp for sp in t["spans"] if sp[0] >= site]

        def recall(recovered, truth):
            if not truth:
                return 1.0
            hits = 0
            for gs, ge in truth:
                if any(
                    rs <= gs + tol and re_ >= ge - tol for rs, re_ in recovered
                ):
                    hits += 1
            return hits / len(truth)

        row["p_recall"].append(recall(p_past, gt_suffix))
        row["t_recall"].append(recall(t_past, gt_suffix))
        if case["class"] == "truncate":
            # truncation removes the suffix; suffix recovery is undefined
            row["suffix_defined"] = False
            continue
        # A clean full parse covers the whole suffix (both engines).
        p_covered = p["len"] if p["ok"] else p["pos"]
        t_covered = t["len"] if t["ok"] else t["pos"]
        row["p_suffix"].append(min(max(p_covered - site, 0) / suffix, 1.0))
        row["t_suffix"].append(min(max(t_covered - site, 0) / suffix, 1.0))

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
