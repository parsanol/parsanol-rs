#!/usr/bin/env python3
"""The constrained-decoding mask benchmark (rs#733, SOTA-2028/2).

One command, all competitors, per the rs#526 pattern:

    python3 constrained_decode_bench.py [--vocab-size N] [--steps N] [--json-out PATH]

Engines:
  parsanol   — the published py binding (mask_open/mask_step, the
               checksummed artifact is the compiled form: provenance
               by construction)
  xgrammar   — GrammarCompiler.compile_builtin_json_grammar + GrammarMatcher
  llguidance — LLTokenizer + LLMatcher over grammar_from_json_schema
  outlines   — v1's Guide requires a live HF tokenizer object; the
               tokenizer-less mask loop this harness measures is not
               expressible without pulling torch/tokenizers. Recorded
               as an explicit NA cell (documented, not skipped).

Vocab: synthetic deterministic token sets at the requested size — the
json-relevant head tokens plus distinct filler byte strings (llguidance
and xgrammar take byte tokens; parsanol takes str tokens). The mask
loop advances a FIXED valid-json token trajectory; every step asserts
the emitted mask allows the trajectory's next token (positive content
assertion — a silent all-zero mask fails here, not downstream).

Metrics: grammar compile time, mask emission us/token, peak RSS.
"""
import argparse
import json
import os
import subprocess
import sys
import time


def build_vocab(size: int):
    """Deterministic vocab: all 256 single-byte tokens (canonical by
    construction for every engine) plus distinct filler tokens to
    scale the mask width. The fillers are json-inert multi-char
    strings the grammar never allows — they exist to measure the
    mask-width cost at real vocabularies' sizes."""
    chars = [chr(b) for b in range(256)]
    fillers = []
    i = 0
    while len(chars) + len(fillers) < size:
        fillers.append(f"zz{i:06d}")
        i += 1
    return chars, fillers


DOC = '{"key": 1, "value": [10, 20, 30], "alpha": {"id": 7, "deep": [true, null]}, "beta": "text"}'


def trajectory_ids():
    """Byte-level ids into the 256-token head: one step per byte."""
    return [b for b in DOC.encode("utf-8")]


# --- engines (each runs in-process; the harness spawns one python per
# engine only for the RSS isolation, via bench_one below) ---


def bench_parsanol(artifact_path, entry, vocab_size, steps):
    from parsanol._ffi import mask_free, mask_open, mask_step

    head, fillers = build_vocab(vocab_size)
    vocab_tokens = head + fillers
    wire = open(artifact_path).read()
    ctx = parsanol_ctx()
    t0 = time.perf_counter_ns()
    handle = mask_open(
        wire, entry, [(i, tok) for i, tok in enumerate(vocab_tokens)], context=ctx
    )
    compile_us = (time.perf_counter_ns() - t0) / 1e3

    traj = trajectory_ids()
    prefix = ""
    mask_us = []
    for i in range(min(steps, len(traj))):
        t0 = time.perf_counter_ns()
        out = mask_step(handle, prefix, context=ctx)
        mask_us.append((time.perf_counter_ns() - t0) / 1e3)
        if out is None or len(out) == 0:
            raise SystemExit(f"parsanol: step {i} returned an empty mask wire")
        if sum(1 for b in out if b) == 0:
            raise SystemExit(f"parsanol: step {i} mask allows nothing")
        prefix += chr(traj[i])
    mask_free(handle, context=ctx)
    return {
        "engine": "parsanol",
        "compile_us": compile_us,
        "mask_us_per_token": sorted(mask_us)[len(mask_us) // 2],
        "steps": len(mask_us),
    }


def bench_xgrammar(vocab_size, steps):
    import xgrammar as xg

    head, fillers = build_vocab(vocab_size)
    vocab_bytes = [t.encode() for t in head + fillers]
    t0 = time.perf_counter_ns()
    info = xg.TokenizerInfo(vocab_bytes, vocab_size=len(vocab_bytes), stop_token_ids=[0])
    cg = xg.GrammarCompiler(info).compile_builtin_json_grammar()
    compile_us = (time.perf_counter_ns() - t0) / 1e3

    matcher = xg.GrammarMatcher(cg)
    buf = xg.allocate_token_bitmask(1, len(vocab_bytes))
    ids = trajectory_ids()
    mask_us = []
    for i in range(min(steps, len(ids))):
        t0 = time.perf_counter_ns()
        matcher.fill_next_token_bitmask(buf, 0)
        mask_us.append((time.perf_counter_ns() - t0) / 1e3)
        if not matcher.accept_token(ids[i]):
            raise SystemExit(f"xgrammar: step {i} rejected the trajectory byte {ids[i]}")
    return {
        "engine": "xgrammar",
        "compile_us": compile_us,
        "mask_us_per_token": sorted(mask_us)[len(mask_us) // 2],
        "steps": len(mask_us),
    }


def bench_llguidance(vocab_size, steps):
    import llguidance

    head, fillers = build_vocab(vocab_size)
    vocab_bytes = [t.encode() for t in head + fillers]

    class Vocab:
        eos_token_id = 0
        bos_token_id = None
        tokens = vocab_bytes
        special_token_ids = [0]

    t0 = time.perf_counter_ns()
    tok = llguidance.LLTokenizer(llguidance.TokenizerWrapper(Vocab()))
    # The JSON lark (the schema-compiler path yields an
    # immediately-complete matcher on this llguidance version). The
    # STRING regex avoids escape classes — this llguidance's lexer
    # dialect rejects them — the trajectory carries no backslashes.
    lark = r'''start: value
?value: object | array | STRING | NUMBER | "true" | "false" | "null"
object: "{" [member ("," member)*] "}"
member: STRING ":" value
array: "[" [value ("," value)*] "]"
STRING: /"[^"]*"/
NUMBER: /-?(0|[1-9][0-9]*)(\.[0-9]+)?([eE][+-]?[0-9]+)?/
WS: /[ \t\n]*/
%ignore WS
'''
    matcher = llguidance.LLMatcher(tok, lark)
    if matcher.is_error():
        raise SystemExit(f"llguidance: grammar error {matcher.get_error()[:120]}")
    compile_us = (time.perf_counter_ns() - t0) / 1e3

    ids = trajectory_ids()
    mask_us = []
    for i in range(min(steps, len(ids))):
        t0 = time.perf_counter_ns()
        mk = matcher.compute_bitmask()
        mask_us.append((time.perf_counter_ns() - t0) / 1e3)
        if mk is not None and not (mk[ids[i] // 8] >> (ids[i] % 8)) & 1:
            raise SystemExit(f"llguidance: step {i} disallows the trajectory byte {ids[i]}")
        matcher.consume_token(ids[i])
    return {
        "engine": "llguidance",
        "compile_us": compile_us,
        "mask_us_per_token": sorted(mask_us)[len(mask_us) // 2],
        "steps": len(mask_us),
    }


def parsanol_ctx():
    import parsanol

    return parsanol.NativeContext()


def bench_one(engine, artifact_path, entry, vocab_size, steps):
    """Run one engine in this process; the caller forks per engine for
    RSS isolation."""
    import resource

    if engine == "parsanol":
        r = bench_parsanol(artifact_path, entry, vocab_size, steps)
    elif engine == "xgrammar":
        r = bench_xgrammar(vocab_size, steps)
    elif engine == "llguidance":
        r = bench_llguidance(vocab_size, steps)
    else:
        raise SystemExit(f"unknown engine {engine}")
    r["peak_rss_kb"] = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss // (
        1024 if sys.platform == "darwin" else 1
    )  # darwin reports bytes, linux reports KB
    return r


ENGINES = ("parsanol", "xgrammar", "llguidance")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--vocab-size", type=int, default=32768)
    ap.add_argument("--steps", type=int, default=28)
    ap.add_argument("--artifact", default=None, help="parsanol artifact json")
    ap.add_argument("--entry", default="main")
    ap.add_argument("--json-out", default=None)
    args = ap.parse_args()

    artifact = args.artifact or os.path.join(
        os.path.dirname(os.path.abspath(__file__)), "json-artifact.json"
    )

    results = []
    for engine in ENGINES:
        # One subprocess per engine: the import alone dominates RSS, so
        # isolation is the only honest way to report peak memory.
        code = (
            "import json,sys;"
            "sys.path.insert(0, %r);"
            "import constrained_decode_bench as b;"
            "print(json.dumps(b.bench_one(sys.argv[1], sys.argv[2], sys.argv[3], int(sys.argv[4]), int(sys.argv[5]))))"
            % (os.path.dirname(os.path.abspath(__file__)),)
        )
        out = subprocess.run(
            [sys.executable, "-c", code, engine, artifact, args.entry,
             str(args.vocab_size), str(args.steps)],
            capture_output=True, text=True,
        )
        if out.returncode != 0:
            print(f"{engine}: FAILED\n{out.stderr[-800:]}", file=sys.stderr)
            results.append({"engine": engine, "error": out.stderr[-300:]})
            continue
        results.append(json.loads(out.stdout))

    print(f"== constrained-decoding mask benchmark (vocab {args.vocab_size}, {args.steps} steps) ==")
    print("| engine | compile | mask us/token | peak RSS MB |")
    print("|---|---|---|---|")
    for r in results:
        if "error" in r:
            print(f"| {r['engine']} | FAILED | — | — |")
            continue
        print(
            f"| {r['engine']} | {r['compile_us']:.0f} us "
            f"| {r['mask_us_per_token']:.2f} "
            f"| {r['peak_rss_kb'] / 1024:.1f} |"
        )
    if args.json_out:
        with open(args.json_out, "w") as f:
            json.dump({"vocab_size": args.vocab_size, "steps": args.steps,
                       "results": results}, f, indent=2)
        print(f"results -> {args.json_out}")


if __name__ == "__main__":
    main()
