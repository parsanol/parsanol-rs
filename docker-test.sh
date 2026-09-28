#!/bin/bash
# HARD-CONTAINED test runner: the VM must never take the host down.
# 4GB RAM cap, no swap, 2 CPUs, read-only except the cargo caches.
#
# Optional: set PG_ARTIFACT_DIR on the HOST to pubid-grammar's
# artifacts/ directory — the grammar tree is mounted read-only into
# the container and PG_*_DIR point at the in-container paths. Without
# it, artifact-gated tests skip.
set -e
ROOT="$(cd "$(dirname "$0")" && pwd)"

mounts=()
envs=()
if [ -n "$PG_ARTIFACT_DIR" ] && [ -d "$PG_ARTIFACT_DIR" ]; then
  grammar_root="$(cd "$PG_ARTIFACT_DIR/.." && pwd)"
  mounts+=(-v "$grammar_root":/pubid-grammar:ro)
  envs+=(
    -e PG_ARTIFACT_DIR=/pubid-grammar/artifacts
    -e PG_SUITES_DIR=/pubid-grammar/suites
    -e PG_CORPUS_DIR=/pubid-grammar/corpora
  )
fi

exec docker run --rm \
  --memory 4g --memory-swap 4g \
  --cpus 2 \
  --pids-limit 256 \
  -v "$ROOT":/work -w /work \
  -v "$ROOT/target-docker":/work/target \
  -v parsanol_cargo_registry:/usr/local/cargo/registry \
  "${mounts[@]}" \
  "${envs[@]}" \
  rust:1-slim \
  cargo "$@"
