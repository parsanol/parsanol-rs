#!/usr/bin/env python3
"""The industry benchmark driver (rs#526).

Runs every engine runner over the shared corpus and emits the receipts
table. Methodology: each engine's native runner, min-of-N per document,
the totals aggregated per byte. All runners verify full consumption /
clean trees before timing (a bench that silently short-parses is
worthless).

Usage: bench.py <corpus-dir> <artifact.json> <entry> <rounds>
"""
import subprocess
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent


def run(cmd, label, expected_total_bytes=None):
    print(f"== {label} ==", file=sys.stderr)
    t0 = time.perf_counter()
    proc = subprocess.run(cmd, capture_output=True, text=True)
    wall = time.perf_counter() - t0
    if proc.returncode != 0:
        print(f"{label}: FAILED\n{proc.stderr[-2000:]}", file=sys.stderr)
        return None
    total = None
    for line in proc.stdout.splitlines():
        if line.startswith("TOTAL"):
            _, bytes_, ns = line.split()
            total = (int(bytes_), int(ns))
    if total is None:
        print(f"{label}: no TOTAL line", file=sys.stderr)
        return None
    bytes_, ns = total
    if expected_total_bytes is not None and bytes_ != expected_total_bytes:
        print(f"{label}: byte mismatch {bytes_} != {expected_total_bytes}", file=sys.stderr)
        return None
    us_per_b = ns / max(bytes_, 1) / 1000.0
    print(
        f"| {label} | {bytes_:,} | {ns:,} | {us_per_b:.3f} |",
        flush=True,
    )
    print(f"   (runner wall {wall:.1f}s)", file=sys.stderr)
    return us_per_b


def main():
    corpus = Path(sys.argv[1]).resolve()
    artifact = Path(sys.argv[2]).resolve()
    entry = sys.argv[3]
    rounds = sys.argv[4]

    corpus_files = sorted(corpus.glob("*.json"))
    expected = sum(p.stat().st_size for p in corpus_files)
    print(f"corpus: {len(corpus_files)} docs, {expected:,} bytes, rounds={rounds}")
    print()
    print("| engine | bytes | ns (min-of-N total) | us/byte |")
    print("|---|---|---|---|")

    run(
        [
            str(HERE / "runners/parsanol/target/release/run_parsanol"),
            str(artifact), entry, str(corpus), rounds, "vm",
        ],
        "parsanol VM (rust)",
        expected,
    )
    run(
        [
            str(HERE / "runners/parsanol/target/release/run_parsanol"),
            str(artifact), entry, str(corpus), rounds, "interpreter",
        ],
        "parsanol interpreter (rust)",
        expected,
    )
    run(
        [sys.executable, str(HERE / "runners/run_lark.py"), str(corpus), rounds],
        "Lark LALR (python)",
        expected,
    )
    run(
        [
            str(HERE / "runners/ts-rust/target/release/run_ts"),
            str(corpus), rounds,
        ],
        "tree-sitter (rust)",
        expected,
    )
    run(
        [
            str(HERE / "runners/antlr/.venv/bin/python"),
            str(HERE / "runners/antlr/run_antlr.py"), str(corpus), rounds,
        ],
        "ANTLR 4.13 (python3 target)",
        expected,
    )


if __name__ == "__main__":
    main()
