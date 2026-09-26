#!/bin/bash
# HARD-CONTAINED test runner: the VM must never take the host down.
# 2GB RAM cap, no swap, 2 CPUs, read-only except the cargo target cache.
set -e
ROOT="$(cd "$(dirname "$0")" && pwd)"
exec docker run --rm \
  --memory 4g --memory-swap 4g \
  --cpus 2 \
  --pids-limit 256 \
  -v "$ROOT":/work -w /work \
  -v parsanol_cargo_registry:/usr/local/cargo/registry \
  -v /Users/mulgogi/src/parsanol/parsanol-rs/target-docker:/work/target \
  -v /Users/mulgogi/src/pubid/pubid-grammar:/pubid-grammar:ro \
  -e PG_ARTIFACT_DIR=/pubid-grammar/artifacts \
  -e PG_SUITES_DIR=/pubid-grammar/suites \
  -e PG_CORPUS_DIR=/pubid-grammar/corpora \
 \
  rust:1-slim \
  cargo "$@"
