#!/usr/bin/env python3
"""The ANTLR runner for the industry benchmark suite (rs#526).

Usage (from runners/antlr): .venv/bin/python run_antlr.py <corpus-dir> <rounds>

Uses the canonical grammars-v4 JSON grammar generated to Python3 with
the ANTLR 4.13.2 runtime — ANTLR's documented Python3 target. Prints
the same line protocol as the other runners: `<doc> <bytes> <ns>` per
doc (min-of-N), then `TOTAL <bytes> <ns>`.
"""
import sys
import time
from pathlib import Path

from antlr4 import InputStream, CommonTokenStream
from antlr4.error.ErrorListener import ErrorListener
from antlr4.error.ErrorStrategy import DefaultErrorStrategy

from JSONLexer import JSONLexer
from JSONParser import JSONParser


class Strict(ErrorListener):
    def __init__(self):
        super().__init__()
        self.errors = 0

    def syntaxError(self, recognizer, offendingSymbol, line, column, msg, e):
        self.errors += 1


def parse_doc(parser_ctor, text):
    parser = parser_ctor(text)
    tree = parser.json()  # the start rule of the grammars-v4 JSON grammar
    return tree


def main():
    if len(sys.argv) != 3:
        print("usage: run_antlr.py <corpus-dir> <rounds>", file=sys.stderr)
        sys.exit(2)
    corpus = Path(sys.argv[1])
    rounds = int(sys.argv[2])

    def parser_ctor(text):
        lexer = JSONLexer(InputStream(text))
        lexer.removeErrorListeners()
        err = Strict()
        lexer.addErrorListener(err)
        parser = JSONParser(CommonTokenStream(lexer))
        parser.removeErrorListeners()
        parser.addErrorListener(err)
        parser._errHandler = DefaultErrorStrategy()
        parser.err = err
        return parser

    docs = sorted(corpus.glob("*.json"))
    if not docs:
        print("no corpus documents", file=sys.stderr)
        sys.exit(2)

    total_bytes = 0
    for d in docs:
        text = d.read_text()
        total_bytes += len(text)
        p = parser_ctor(text)
        parse_doc(parser_ctor, text)
        if p.err.errors:
            print(f"antlr reports errors on {d.name}", file=sys.stderr)
            sys.exit(1)

    total_ns = 0
    for d in docs:
        text = d.read_text()
        best = None
        for _ in range(rounds):
            t = time.perf_counter_ns()
            parse_doc(parser_ctor, text)
            ns = time.perf_counter_ns() - t
            if best is None or ns < best:
                best = ns
        total_ns += best
        print(f"{d.name} {len(text)} {best}")
    print(f"TOTAL {total_bytes} {total_ns}")


if __name__ == "__main__":
    main()
