#!/usr/bin/env python3
"""The Lark runner for the industry benchmark suite (rs#526).

Usage: run_lark.py <corpus-dir> <rounds>

Uses the canonical Lark JSON grammar (lark's own json example) with the
LALR transformer — lark's documented fastest configuration. Prints the
same line protocol as run_parsanol: `<doc> <bytes> <ns_per_parse>` per
document (min over rounds), then `TOTAL <bytes> <total_ns>`.
"""
import json
import sys
import time
from pathlib import Path

from lark import Lark
from lark.parsers.lalr_interactive_parser import InteractiveParser  # noqa: F401 (the lalr import sanity)

GRAMMAR = r"""
?start: value

?value: object
      | array
      | string
      | SIGNED_NUMBER -> number
      | "true"        -> true
      | "false"       -> false
      | "null"        -> null

?array:     "[" [value ("," value)*] "]"
?object:    "{" [pair ("," pair)*] "}"
pair:       string ":" value

string:     ESCAPED_STRING

%import common.ESCAPED_STRING
%import common.SIGNED_NUMBER
%ignore /[\t \f\r\n]+/
"""


def main():
    if len(sys.argv) != 3:
        print("usage: run_lark.py <corpus-dir> <rounds>", file=sys.stderr)
        sys.exit(2)
    corpus = Path(sys.argv[1])
    rounds = int(sys.argv[2])

    parser = Lark(GRAMMAR, parser="lalr", start="start")

    docs = sorted(corpus.glob("*.json"))
    if not docs:
        print("no corpus documents", file=sys.stderr)
        sys.exit(2)

    # warmup + validity: every doc must parse and round-trip through json
    total_bytes = 0
    for d in docs:
        text = d.read_text()
        total_bytes += len(text)
        tree = parser.parse(text)
        json.loads(text)  # the semantic cross-check

    total_ns = 0
    for d in docs:
        text = d.read_text()
        best = None
        for _ in range(rounds):
            t = time.perf_counter_ns()
            black_box = parser.parse(text)
            del black_box
            ns = time.perf_counter_ns() - t
            if best is None or ns < best:
                best = ns
        total_ns += best
        print(f"{d.name} {len(text)} {best}")
    print(f"TOTAL {total_bytes} {total_ns}")


if __name__ == "__main__":
    main()
