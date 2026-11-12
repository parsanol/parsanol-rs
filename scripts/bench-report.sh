#!/usr/bin/env bash
# Benchmark receipts (rs#463): run the comparison + libraries benches
# with bounded samples and emit benchmarks/latest.md — machine, rustc,
# commit, and the tables. CI-cheap by design (criterion bounded run);
# compare TRENDS on the same runner class, never absolutes across
# machines. The scheduled workflow gates >10% regressions against the
# committed receipt.
set -eu
cd "$(dirname "$0")/.."
mkdir -p benchmarks

commit="$(git rev-parse HEAD)"
rustc_version="$(rustc --version)"
machine="${RUNNER_NAME:-$(uname -srm)}"
date_stamp="$(date -u +%Y-%m-%d)"

# Bounded criterion run. Criterion's default output names each
# benchmark on its own line followed by `time: [lo mid hi unit]` —
# extracted to table rows with the MEDIAN estimate in ns.
run_bench() {
  cargo bench -p parsanol --bench "$1" --no-default-features -- \
    --sample-size 10 --warm-up-time 1 --measurement-time 3 2>&1 \
    | python3 -c '
import re, sys
SCALE = {"ns": 1, "\u00b5s": 1_000, "ms": 1_000_000, "s": 1_000_000_000}

def emit(name, bracket):
    parts = bracket.split()
    # [lo unit mid unit hi unit]
    if len(parts) == 6 and parts[3] in SCALE:
        ns = int(float(parts[2].replace(",", "")) * SCALE[parts[3]])
        print(f"| {name} | {ns} |")

pending = None
for line in sys.stdin:
    same = re.match(r"^(Benchmarking\s+)?(\S+)\s+time:\s*\[([^\]]+)\]", line)
    if same and not same.group(1):
        emit(same.group(2), same.group(3))
        pending = None
        continue
    if same:
        pending = None
        continue
    if re.match(r"^Benchmarking\s+", line):
        pending = None
        continue
    if pending:
        found = re.search(r"time:\s*\[([^\]]+)\]", line)
        if found:
            emit(pending, found.group(1))
            pending = None
            continue
    bare = re.match(r"^(\S+)\s*$", line)
    if bare and ":" not in line and not line.startswith(" "):
        pending = bare.group(1)
'

}

{
  echo "# Benchmark receipts"
  echo
  echo "- commit: \`${commit}\`"
  echo "- rustc: \`${rustc_version}\`"
  echo "- machine: \`${machine}\`"
  echo "- date: ${date_stamp}"
  echo "- method: criterion, bounded CI run (sample-size 10, 1s warm-up, 3s measure); the reported number is the criterion estimate in ns/iter"
  echo
  echo "## libraries — parsanol native vs the parslet-compatibility path"
  echo
  echo "The campaign bar (\"beat parslet on both paths\") measured on the"
  echo "same wire: the native engine and the parslet-compatibility"
  echo "transformation over identical grammars and inputs."
  echo
  echo "| benchmark | ns/iter |"
  echo "|---|---:|"
  run_bench libraries
  echo
  echo "## comparison — summary groups (serialize vs direct paths)"
  echo
  echo "| benchmark | ns/iter |"
  echo "|---|---:|"
  run_bench comparison
} > benchmarks/latest.md

echo "wrote benchmarks/latest.md (${commit})"
