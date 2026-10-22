#!/usr/bin/env python3
"""One-shot validator for the PARG match relation (TODO.sota/8 B2).

This program is written FROM THE DOCUMENT — docs/parg-semantics.md
(sections 2-6: values and flattening V0-V4, terminals T*, combinators
C*, captures K*, trivia injection as it appears in the baked artifact) —
not from the engine. It evaluates the portable grammar embedded in each
conformance artifact and checks the corpus expectations:

  accept  -> the input parses and consumes fully
  reject  -> the input does not parse
  capture -> the flattened tree contains the expected capture values
  shape   -> the flattened tree equals the expected canonical tree
  example -> bindings-layer test; not part of the match relation (skipped)

DELIBERATELY DISPOSABLE. It is a cross-check, not a runtime: run it when
the match relation in docs/parg-semantics.md changes materially (a new
combinator or a changed flatten law), triage every divergence (engine
bug or spec bug), and archive the report. It must never be imported by
the engines or shipped in a library.

Usage:
    python3 validate.py --corpus <parsanol-rs>/corpus/conformance
    python3 validate.py --corpus ... --report REPORT.md
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

# ---------------------------------------------------------------------------
# Values (V0). A raw value keeps the tagged shape the flatten laws talk
# about; flatten() reduces it to the consumer tree.
# ---------------------------------------------------------------------------


class Unsupported(Exception):
    """The artifact uses a construct this one-shot validator does not model."""


EPS = ("eps", None)


def text(s):
    return ("text", s)


def seq(values):
    return ("seq", values)


def rep(values, named=False):
    return ("rep", values)


def maybe(values):
    return ("maybe", values)


def mapping(pairs):
    return ("map", pairs)


def flatten(raw, in_capture=False):
    """V1-V4: reduce a tagged value to the consumer tree."""
    kind, value = raw
    if kind == "eps":
        return None
    if kind == "text":
        return value
    if kind == "map":
        return dict(value)
    if kind == "seq":
        # V1: children flatten left to right and merge; an empty
        # sequence flattens to the empty text. The named context does
        # not propagate through a sequence (only a direct capture's
        # value uses the named V2/V3 arms).
        merged = None
        for child in value:
            merged = merge(merged, flatten(child, in_capture=False))
        return "" if merged is None else merged
    if kind == "maybe":
        # V2: matched -> flatten(x); absent -> nil inside a capture,
        # empty text otherwise. A matched ε is "" outside a capture.
        if value:
            flattened = flatten(value[0], in_capture)
            return flattened if in_capture else (flattened if flattened is not None else "")
        return None if in_capture else ""
    if kind == "rep":
        # V3: maps win, then arrays, then text.
        if not value:
            return [] if in_capture else ""
        items = [flatten(child, in_capture) for child in value]
        maps = [item for item in items if isinstance(item, dict)]
        if maps:
            return maps
        arrays = [item for item in items if isinstance(item, list)]
        if arrays:
            return [element for array in arrays for element in array]
        return "".join(item for item in items if isinstance(item, str))
    raise Unsupported(f"unknown raw value kind {kind!r}")


def merge(left, right):
    """V1 merge laws (text/map/array/ε).

    The map/array pair is a FLAT append, not nesting (the engine's
    merge_fold): `H ⊕ A = [H] + A` and `A ⊕ H = A + [H]`. The doc's
    original table stated `[H, X]`, which nested the array — a spec
    finding recorded in REPORT.md.
    """
    if left is None:
        return right
    if right is None:
        return left
    if isinstance(left, dict) and isinstance(right, dict):
        merged = dict(left)
        merged.update(right)  # the right value wins on a duplicate key
        return merged
    if isinstance(left, str) and isinstance(right, str):
        return left + right
    if isinstance(left, str):
        return right  # text next to a non-text value is dropped
    if isinstance(right, str):
        return left
    if isinstance(left, list) and isinstance(right, list):
        return left + right
    if isinstance(right, dict):
        return (left if isinstance(left, list) else [left]) + [right]
    if isinstance(left, dict):
        return [left] + (right if isinstance(right, list) else [right])
    return [left, right]


# ---------------------------------------------------------------------------
# The matcher. Deterministic PEG: every atom yields at most one result.
# ---------------------------------------------------------------------------


class Matcher:
    def __init__(self, atoms, root, source):
        self.atoms = atoms
        self.root = root
        self.source = source
        self.depth = 0
        self.max_depth = 4000
        # K5: comment units recorded by TriviaCapture, attached to the
        # next successful Named capture.
        self.pending = []
        self._regex_cache = {}

    # -- terminal helpers ---------------------------------------------------

    def _regex(self, pattern):
        compiled = self._regex_cache.get(pattern)
        if compiled is None:
            compiled = re.compile(_rust_regex_to_python(pattern))
            self._regex_cache[pattern] = compiled
        return compiled

    # -- the relation -------------------------------------------------------

    def match_root(self):
        result = self.match(self.root, 0)
        if result is None:
            return None
        end, value = result
        # The root requires full consumption (parse() contract).
        if end != len(self.source):
            return None
        return value

    def match(self, atom_id, pos):
        self.depth += 1
        if self.depth > self.max_depth:
            raise Unsupported("recursion depth exceeded")
        try:
            return self._match(self.atoms[atom_id], pos)
        finally:
            self.depth -= 1

    def _match(self, atom, pos):
        (kind, body), = atom.items()

        if kind == "Str":
            literal = body["pattern"]
            if self.source.startswith(literal, pos):
                return pos + len(literal), text(literal)
            return None

        if kind == "Re":
            compiled = self._regex(body["pattern"])
            found = compiled.match(self.source, pos)
            if found is None:
                return None
            return found.end(), text(found.group(0))

        if kind == "Sequence":
            current = pos
            values = []
            for child in body["atoms"]:
                result = self.match(child, current)
                if result is None:
                    return None
                current, value = result
                values.append(value)
            return current, seq(values)

        if kind == "Alternative":
            for child in body["atoms"]:
                result = self.match(child, pos)
                if result is not None:
                    return result
            return None

        if kind == "Repetition":
            child = body["atom"]
            minimum = body.get("min", 0)
            maximum = body.get("max")
            tag = body.get("tag", "Repetition")
            current = pos
            values = []
            while maximum is None or len(values) < maximum:
                result = self.match(child, current)
                if result is None:
                    break
                end, value = result
                values.append(value)
                if end == current:  # zero-width body: stop (C4 guard)
                    break
                current = end
            if len(values) < minimum:
                return None
            if tag == "Maybe":
                return current, maybe(values[:1])
            return current, rep(values)

        if kind == "Named":
            snapshot = list(self.pending)
            result = self.match(body["atom"], pos)
            if result is None:
                self.pending = snapshot
                return None
            end, value = result
            # V4/K1: the capture's inner value uses the named variant.
            pairs = {body["name"]: flatten(value, in_capture=True)}
            # K5: matched trivia attaches to the next successful capture.
            if self.pending:
                pairs["comments"] = [dict([unit]) for unit in self.pending]
                self.pending = []
            return end, mapping(pairs)

        if kind == "Entity":
            # C7: a rule reference is transparent for values and flattening.
            return self.match(body["atom"], pos)

        if kind == "Lookahead":
            snapshot = list(self.pending)
            result = self.match(body["atom"], pos)
            self.pending = snapshot
            positive = body.get("positive", True)
            if positive:
                return (pos, EPS) if result is not None else None
            return (pos, EPS) if result is None else None

        if kind == "Lookbehind":
            look = body["look"]
            (look_kind, look_body), = look.items()
            if look_kind == "Literal":
                count = look_body["count"]
                window = look_body["pattern"]
                matched = pos >= count and \
                    self.source[pos - count:pos] == window
            elif look_kind == "Regex":
                # The behind-window is an end-anchored match.
                prefix = self.source[:pos]
                matched = re.search(
                    "(?:" + _rust_regex_to_python(look_body["source"]) + ")\\Z",
                    prefix,
                ) is not None
            else:
                raise Unsupported(f"lookbehind kind {look_kind!r}")
            positive = body.get("positive", True)
            if positive != matched:
                return None
            return pos, EPS

        if kind == "Trivia":
            # S3: injected trivia consumes input, contributes ε.
            result = self.match(body["atom"], pos)
            if result is None:
                return None
            return result[0], EPS

        if kind == "TriviaCapture":
            # K5: comment-shaped units are recorded; whitespace is not.
            result = self.match(body["atom"], pos)
            if result is None:
                return None
            end, raw_value = result
            unit_text = flatten(raw_value)
            if isinstance(unit_text, str) and unit_text.strip():
                label = _trivia_label(body.get("rules", []), unit_text)
                if label is not None:
                    self.pending.append({label: unit_text})
            return end, EPS

        if kind == "Ignore":
            result = self.match(body["atom"], pos)
            if result is None:
                return None
            return result[0], EPS

        if kind == "Constant":
            # T6: zero-width, yields the wire constant.
            return pos, text(str(body.get("value")))

        if kind == "Cut":
            return pos, EPS

        raise Unsupported(f"atom kind {kind!r} is outside this validator's scope")


def _trivia_label(rules, unit_text):
    for prefix, label in rules:
        if unit_text.startswith(prefix):
            return label
    return None


def collect_captures(value, out=None):
    """Every named-capture pair in the flattened tree, first value wins."""
    if out is None:
        out = {}
    if isinstance(value, dict):
        for key, item in value.items():
            out.setdefault(key, item)
            collect_captures(item, out)
    elif isinstance(value, list):
        for item in value:
            collect_captures(item, out)
    return out


def _rust_regex_to_python(pattern):
    """Translate the Rust-regex spellings the corpus uses into Python's."""
    # \u{XXXX} / \u{10FFFF} -> the literal character.
    return re.sub(
        r"\\u\{([0-9a-fA-F]+)\}",
        lambda match: chr(int(match.group(1), 16)),
        pattern,
    )


# ---------------------------------------------------------------------------
# Corpus driver.
# ---------------------------------------------------------------------------


def load_artifact(path):
    with open(path, encoding="utf-8") as handle:
        return json.load(handle)


def validate_case(envelope, case):
    entry = case.get("entry") or envelope.get("default_entry")
    entries = envelope.get("entries", {})
    if entry not in entries:
        raise Unsupported(f"entries has no {entry!r}")
    grammar = entries[entry].get("grammar")
    if grammar is None:
        raise Unsupported(f"entry {entry!r} has no portable grammar")

    input_text = case["input"]
    matcher = Matcher(grammar["atoms"], grammar["root"], input_text)
    raw = matcher.match_root()
    kind = case["kind"]
    expect = case.get("expect") or {}

    if kind == "reject":
        return None if raw is None else "expected rejection, matched"

    if raw is None:
        return "expected a match, rejected"

    tree = flatten(raw)

    if kind == "accept":
        return None

    if kind == "capture":
        # Corpus capture tests collect named captures from anywhere in
        # the flattened tree (the suite's collect_captures walk), not
        # only at the top level.
        captures = collect_captures(tree)
        for key, want in expect.items():
            if key not in captures:
                return f"caption {key!r} missing from tree"
            if captures[key] != want:
                return f"capture {key!r}: expected {want!r}, got {captures[key]!r}"
        return None

    if kind == "shape":
        if tree != expect:
            return f"shape mismatch: expected {expect!r}, got {tree!r}"
        return None

    if kind == "example":
        return None  # bindings layer, outside the match relation

    raise Unsupported(f"test kind {kind!r}")


def validate_fuzz_case(envelope, case):
    """A fuzz fixture case: {input, accept, shape} from the Ruby exporter."""
    entries = envelope.get("entries", {})
    entry = envelope.get("default_entry") or next(iter(entries), None)
    grammar = entries[entry].get("grammar")
    matcher = Matcher(grammar["atoms"], grammar["root"], case["input"])
    raw = matcher.match_root()
    if not case["accept"]:
        return None if raw is None else "expected rejection, matched"
    if raw is None:
        return "expected a match, rejected"
    tree = flatten(raw)
    if tree != case["shape"]:
        return f"shape mismatch: expected {case['shape']!r}, got {tree!r}"
    return None


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--corpus", type=Path,
                        help="conformance corpus directory (corpus/conformance)")
    parser.add_argument("--fuzz", type=Path,
                        help="fuzz fixture directory (gNNN.artifact.json + "
                             "gNNN.cases.json, as exported by the Ruby fuzz spec)")
    parser.add_argument("--report", type=Path,
                        help="write the run report to this path")
    args = parser.parse_args()
    if not args.corpus and not args.fuzz:
        parser.error("give --corpus, --fuzz, or both")

    totals = {"cases": 0, "checked": 0, "ok": 0, "diverged": 0, "skipped": 0}
    lines = ["# B2 validator report", ""]
    divergences = []

    def record(group, case, problem, unsupported=None):
        totals["cases"] += 1
        if unsupported is not None:
            totals["skipped"] += 1
            lines.append(f"- SKIP [{case.get('kind')}] "
                         f"{case.get('input')!r}: {unsupported}")
            return
        totals["checked"] += 1
        if problem is None:
            totals["ok"] += 1
        else:
            totals["diverged"] += 1
            divergences.append(
                f"{group} [{case.get('entry')}] [{case.get('kind')}] "
                f"{case.get('input')!r}: {problem}"
            )
            lines.append(f"- DIVERGE [{case.get('kind')}] "
                         f"{case.get('input')!r}: {problem}")

    if args.corpus:
        artifacts = sorted(args.corpus.glob("*.json"))
        if not artifacts:
            sys.exit(f"no artifacts under {args.corpus}")
        lines.append(f"## Conformance corpus ({len(artifacts)} artifacts)")
        lines.append("")
        for artifact_path in artifacts:
            envelope = load_artifact(artifact_path)
            name = artifact_path.stem
            marker = len(lines)
            for case in envelope.get("tests", []):
                try:
                    problem = validate_case(envelope, case)
                except Unsupported as unsupported:
                    record(name, case, None, unsupported=unsupported)
                    continue
                record(name, case, problem)
            if len(lines) > marker:
                lines.insert(marker, f"### {name}")
                lines.insert(marker + 1, "")
        lines.append("")

    if args.fuzz:
        artifacts = sorted(args.fuzz.glob("*.artifact.json"))
        if not artifacts:
            sys.exit(f"no *.artifact.json under {args.fuzz}")
        lines.append(f"## Fuzz fixtures ({len(artifacts)} artifacts)")
        lines.append("")
        for artifact_path in artifacts:
            name = artifact_path.name.removesuffix(".artifact.json")
            cases_path = args.fuzz / f"{name}.cases.json"
            if not cases_path.is_file():
                lines.append(f"- SKIP {name}: no cases file")
                continue
            envelope = load_artifact(artifact_path)
            cases = json.loads(cases_path.read_text(encoding="utf-8"))["cases"]
            marker = len(lines)
            for case in cases:
                try:
                    problem = validate_fuzz_case(envelope, case)
                except Unsupported as unsupported:
                    record(name, {"kind": "fuzz", "input": case["input"]},
                           None, unsupported=unsupported)
                    continue
                record(name, {"kind": "fuzz", "input": case["input"]}, problem)
            if len(lines) > marker:
                lines.insert(marker, f"### {name}")
                lines.insert(marker + 1, "")
        lines.append("")

    lines.extend([
        "## Totals",
        "",
        f"- cases: {totals['cases']}",
        f"- checked: {totals['checked']}",
        f"- passed: {totals['ok']}",
        f"- diverged: {totals['diverged']}",
        f"- skipped: {totals['skipped']}",
        "",
    ])

    print(f"cases={totals['cases']} checked={totals['checked']} "
          f"ok={totals['ok']} diverged={totals['diverged']} "
          f"skipped={totals['skipped']}")
    for divergence in divergences:
        print("DIVERGE:", divergence)
    if args.report:
        args.report.write_text("\n".join(lines) + "\n", encoding="utf-8")
    return 1 if totals["diverged"] else 0


if __name__ == "__main__":
    sys.exit(main())
