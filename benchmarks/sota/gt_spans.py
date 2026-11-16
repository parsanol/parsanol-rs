#!/usr/bin/env python3
"""Ground-truth node spans for the recovery receipts (rs#527).

A minimal byte-position JSON scanner: returns the sorted set of
(start) offsets of every value node in a valid JSON document —
objects, arrays, strings, numbers, literals — the same node notion
both recovery runners emit (tree-sitter: non-error named nodes;
parsanol: recovered structure nodes). Structures get their opening
offset; leaves theirs.
"""
import json


def value_spans(text: str) -> list[int]:
    spans: list[int] = []
    i, n = 0, len(text)

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
            spans.append(i)
            i = skip_ws(i + 1)
            if text[i] == "}":
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
                    return i + 1
                raise ValueError("bad object")
        if c == "[":
            spans.append(i)
            i = skip_ws(i + 1)
            if text[i] == "]":
                return i + 1
            while True:
                i = scan_value(i)
                i = skip_ws(i)
                if text[i] == ",":
                    i += 1
                    i = skip_ws(i)
                    continue
                if text[i] == "]":
                    return i + 1
                raise ValueError("bad array")
        if c == '"':
            spans.append(i)
            return scan_string(i)
        # number / literal: a token run
        spans.append(i)
        j = i
        while j < n and text[j] not in " \t\n\r,]}":
            j += 1
        return j

    end = scan_value(0)
    end = skip_ws(end)
    assert end == n, f"trailing bytes at {end}"
    return sorted(spans)


if __name__ == "__main__":
    import sys
    doc = open(sys.argv[1]).read()
    json.loads(doc)  # the document must be valid
    spans = value_spans(doc)
    print(len(spans))
