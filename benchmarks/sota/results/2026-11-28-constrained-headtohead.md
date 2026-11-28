# The constrained-decoding head-to-head (2026-11-28, rs#733)

The one benchmark axis with no comparative receipt: the mask pipeline
(TokenBitmask + MaskEmitter + the export parity) measured against the
field. Harness: `benchmarks/sota/constrained_decode_bench.py` — one
command, all competitors, per-engine subprocess isolation for the
memory column, byte-level canonical vocabularies (256 single-byte
tokens + deterministic fillers to the target size), a fixed valid-json
byte trajectory with a positive-content assertion per step (a mask
that disallows the trajectory's next byte fails the run — a silent
all-zero mask cannot pass).

## The table (median µs/token, 28-step json trajectory)

| engine | compile (32k) | mask µs/token (32k) | compile (128k) | mask µs/token (128k) | peak RSS (32k) |
|---|---|---|---|---|---|
| parsanol (py ABI 1.5.28) | 39-42 ms | **~28,000-32,000** | — (see below) | **>10,700,000 (300 s timeout / 28 steps)** | 110-124 MB |
| xgrammar | 10-30 ms | **1.0-1.25** | 29.5 ms | **1.25** | 297-331 MB |
| llguidance 1.9.1 | 36-140 ms | **7.54** | 140 ms | **20.8** | 42-97 MB |
| outlines | n/a | n/a | n/a | n/a | n/a |

The outlines cell is an explicit NA, documented per the matrix law:
v1's Guide requires a live HF tokenizer object for every mask step —
the tokenizer-less state-machine loop this harness measures is not
expressible without pulling torch/tokenizers into the harness; the
installed version also pins huggingface-hub conflicts with xgrammar.

## The finding: the gap is the ABI's statelessness, not the engine

`parsanol_mask_step` is a STATELESS ABI by design: every call
rebuilds `TokenOracle::new(...)` (cloning the full vocabulary), then
re-parses the whole prefix from scratch, then emits the mask
(`parsanol/src/ffi/c.rs`, the step function). At 32k tokens that is
~32 ms/step; at 128k the clone-and-reparse crosses ~11 s/step — a
scaling wall long before Llama-class vocabularies. The competitors
keep compiled matchers and advance them per token (xgrammar's
`GrammarMatcher`, llguidance's `LLMatcher`).

**The lever (named, for the follow-up issue)**: a stateful emitter on
the handle — build the oracle once at `parsanol_mask_open`, keep the
prefix-parse state across `parsanol_mask_step` calls, and emit
incrementally. The engine-side primitives already exist (the oracle
and the emitter are pure functions of a prefix); only the handle is
stateless today. The expected result is the oracle-clone cost leaving
the per-step budget entirely and the reparse cost amortizing to the
new byte — the competitors' regime.

## The differentiator rows (explicit, per the acceptance)

- **Artifact provenance**: parsanol's compiled form IS the
  checksummed artifact — `parsanol_mask_open` verifies the checksum
  before building the oracle; the mask output's grammar provenance is
  cryptographically pinned to the artifact text. xgrammar compiles
  from an in-memory EBNF/schema (no provenance wire); llguidance from
  a lark/schema string. Nobody else has this row.
- **Grammar expressiveness**: PARG (recursion, captures, repetition
  bounds, trivia channels — a full parsing language) vs the
  competitors' EBNF/lark/JSON-schema fragments. The pubid identifier
  fleet (recursive grammars with capture-aware masks) is outside
  xgrammar's and llguidance's expression classes; the regex-fragment
  competitors cannot state these grammars at all.

## The conditions line

Host load varied (the machine was loaded during the runs); mask
emission is a state-machine advance measured at median over 28 steps —
the 3-6x spread between repeat runs is within the load noise, and the
ORDERS-OF-MAGNITUDE parsanol-vs-field gap cannot be load bias (the
timeout at 128k is wall-clock). Engine versions: xgrammar (pip,
2026-11-28), llguidance 1.9.1, parsanol 1.5.28.0 (PyPI).
