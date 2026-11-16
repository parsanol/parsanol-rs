#!/usr/bin/env python3
"""The keystroke-trace generator for the incremental receipts (rs#528).

Generates validity-preserving editing traces over a real JSON document:
every intermediate state parses as valid JSON (verified at generation
time with python's json.loads). Edit anchors live inside string-literal
content, whitespace runs, and number digits — the regions a real editor
touches without breaking structure.

Usage: gen_trace.py <source.json> <out-trace.json> <n-edits> <seed>
"""
import json
import random
import sys
from pathlib import Path

STRING_CHARS = ["a", "Z", "0", "-", "_", "parsanol", "x42"]
DIGITS = ["7", "3", "0", "9"]


def safe_spans(text):
    """(start, end) spans where single-char edits keep JSON valid."""
    spans = []
    in_string = False
    i = 0
    n = len(text)
    while i < n:
        c = text[i]
        if in_string:
            if c == "\\":
                i += 2  # skip the escaped char
                continue
            if c == '"':
                in_string = False
            else:
                spans.append(i)
        else:
            if c == '"':
                in_string = True
            elif c.isdigit():
                spans.append(i)
            elif c in " \t\n\r":
                spans.append(i)
        i += 1
    return spans


def main():
    source = Path(sys.argv[1])
    out = Path(sys.argv[2])
    n_edits = int(sys.argv[3])
    seed = int(sys.argv[4])
    rng = random.Random(f"trace:{source.name}:{seed}")

    text = source.read_text()
    json.loads(text)  # source must be valid
    edits = []
    for _ in range(n_edits):
        for _attempt in range(10):
            spans = safe_spans(text)
            if not spans:
                break
            pos = rng.choice(spans)
            roll = rng.random()
            if roll < 0.55:
                ins = rng.choice(STRING_CHARS)
                cand = text[:pos] + ins + text[pos:]
                del_len = 0
            elif roll < 0.70:
                ins = rng.choice(DIGITS)
                cand = text[:pos] + ins + text[pos:]
                del_len = 0
            elif roll < 0.85:
                ins = " "
                cand = text[:pos] + ins + text[pos:]
                del_len = 0
            else:
                cand = text[:pos] + text[pos + 1:]
                ins = ""
                del_len = 1
            try:
                json.loads(cand)
            except Exception:
                continue
            edits.append({"pos": pos, "del": del_len, "text": ins})
            text = cand
            break
        else:
            break
    json.loads(text)  # final state must be valid too

    out.write_text(
        json.dumps(
            {"source": source.name, "seed": seed, "edits": edits},
            indent=1,
        )
    )
    print(f"{len(edits)} edits -> {out}")


if __name__ == "__main__":
    main()
