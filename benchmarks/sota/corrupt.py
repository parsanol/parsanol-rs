#!/usr/bin/env python3
"""The corruption harness for the recovery head-to-head (rs#527).

Generates systematically corrupted copies of the shared corpus by class:

  delete-{n}       delete a single byte/char at n corruption sites
  insert-{n}       insert a junk token at n corruption sites
  mutate-{n}       overwrite a byte at n corruption sites
  truncate-{n}     cut the document mid-structure (n sites: deepest one wins)
  unterminated-{n} drop the closing bracket/quote at n sites

Site positions are seeded (deterministic across runs and machines) and
recorded in manifest.json so every receipt is reproducible.

Usage: corrupt.py <corpus-dir> <out-dir> <seed>
"""
import json
import random
import sys
from pathlib import Path

JUNK_TOKENS = ["@", "#", "%", "~", "\\"]
PAIR_CLOSERS = {'{': '}', '[': ']', '"': '"'}


def struct_positions(text):
    """Offsets of structural bytes usable for structural corruption."""
    return [i for i, c in enumerate(text) if c in '{}[]"']


def corrupt_delete(text, rng, n):
    sites = sorted(rng.sample(range(len(text)), min(n, len(text))))
    for off in reversed(sites):
        text = text[:off] + text[off + 1:]
    return text, sites


def corrupt_insert(text, rng, n):
    sites = sorted(rng.sample(range(len(text)), min(n, len(text))))
    out = []
    prev = 0
    for off in sites:
        out.append(text[prev:off])
        out.append(rng.choice(JUNK_TOKENS))
        prev = off
        sites_idx = len(out) - 1
    out.append(text[prev:])
    return "".join(out), sites


def corrupt_mutate(text, rng, n):
    sites = sorted(rng.sample(range(len(text)), min(n, len(text))))
    chars = list(text)
    for off in sites:
        pool = [c for c in '{}[]":,abcdefghijklmnopqrstuvwxyz0123456789' if c != chars[off]]
        chars[off] = rng.choice(pool)
    return "".join(chars), sites


def corrupt_truncate(text, rng, n):
    # truncate right after one of the sampled structural positions
    structs = struct_positions(text)
    sites = sorted(rng.sample(structs, min(n, len(structs))))
    off = sites[len(sites) // 2]
    return text[: off + 1], [off]


def corrupt_unterminated(text, rng, n):
    structs = struct_positions(text)
    sites = sorted(rng.sample(structs, min(n, len(structs))))
    chars = list(text)
    for off in reversed(sites):
        closer = PAIR_CLOSERS.get(chars[off])
        if closer:
            # drop the matching closer (first occurrence scanning forward)
            idx = text.find(closer, off + 1)
            if idx >= 0:
                del chars[idx]
    return "".join(chars), sites


CLASSES = {
    "delete": corrupt_delete,
    "insert": corrupt_insert,
    "mutate": corrupt_mutate,
    "truncate": corrupt_truncate,
    "unterminated": corrupt_unterminated,
}
DEPTHS = [1, 5, 20]


def main():
    corpus = Path(sys.argv[1])
    out = Path(sys.argv[2])
    seed = int(sys.argv[3])
    out.mkdir(parents=True, exist_ok=True)

    manifest = {"seed": seed, "cases": []}
    for doc in sorted(corpus.glob("*.json")):
        text = doc.read_text()
        for cls, fn in CLASSES.items():
            for depth in DEPTHS:
                rng = random.Random(f"{seed}:{doc.name}:{cls}:{depth}")
                corrupted, sites = fn(text, rng, depth)
                name = f"{doc.stem}.{cls}-{depth}.json"
                (out / name).write_text(corrupted)
                manifest["cases"].append(
                    {
                        "source": doc.name,
                        "class": cls,
                        "depth": depth,
                        "file": name,
                        "corruption_sites": sites,
                    }
                )
    (out / "manifest.json").write_text(json.dumps(manifest, indent=1))
    print(f"{len(manifest['cases'])} corrupted cases -> {out}")


if __name__ == "__main__":
    main()
