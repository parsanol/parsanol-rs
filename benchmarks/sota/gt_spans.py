#!/usr/bin/env python3
"""Ground-truth node spans for the recovery receipts (rs#527).

A minimal byte-position JSON scanner: returns the sorted set of
(start) offsets — and (start, end) byte spans — of every value node
in a valid JSON document: objects, arrays, strings, numbers,
literals. The same node notion both recovery runners emit
(tree-sitter: non-error named nodes; parsanol: recovered structure
nodes). Structures span their braces; leaves their tokens.
"""
import json


def value_span_pairs(text: str) -> list[tuple[int, int]]:
    """(start, end) byte spans of every value node — the span-aware
    ground truth (the recall scorer matches recovered spans by
    containment, not by start-offset equality)."""
    spans: list[tuple[int, int]] = []
    n = len(text)

    def skip_ws(i):
        while i < n and text[i] in " \t\n\r":
            i += 1
        return i

    def scan_string(i):
        # text[i] is the opening quote
        i += 1
        while i < n:
            c = text[i]
            if c == "\\":
                i += 2
                continue
            if c == '"':
                return i + 1
            i += 1
        raise ValueError("unterminated string")

    def scan_value(i):
        i = skip_ws(i)
        c = text[i]
        if c == "{":
            start = i
            i = skip_ws(i + 1)
            if text[i] == "}":
                spans.append((start, i + 1))
                return i + 1
            while True:
                i = skip_ws(i)
                i = scan_string(i)  # the key
                i = skip_ws(i)
                assert text[i] == ":"
                i = scan_value(i + 1)
                i = skip_ws(i)
                if text[i] == ",":
                    i += 1
                    continue
                if text[i] == "}":
                    spans.append((start, i + 1))
                    return i + 1
                raise ValueError("bad object")
        if c == "[":
            start = i
            i = skip_ws(i + 1)
            if text[i] == "]":
                spans.append((start, i + 1))
                return i + 1
            while True:
                i = scan_value(i)
                i = skip_ws(i)
                if text[i] == ",":
                    i += 1
                    i = skip_ws(i)
                    continue
                if text[i] == "]":
                    spans.append((start, i + 1))
                    return i + 1
                raise ValueError("bad array")
        if c == '"':
            start = i
            end = scan_string(i)
            spans.append((start, end))
            return end
        # number / literal: a token run
        j = i
        while j < n and text[j] not in " \t\n\r,]}":
            j += 1
        spans.append((i, j))
        return j

    end = scan_value(0)
    end = skip_ws(end)
    assert end == n, f"trailing bytes at {end}"
    return sorted(spans)


def value_spans(text: str) -> list[int]:
    return [start for start, _ in value_span_pairs(text)]


if __name__ == "__main__":
    import sys

    doc = open(sys.argv[1]).read()
    json.loads(doc)  # the document must be valid
    print(len(value_spans(doc)))
