#!/usr/bin/env python3
"""The windowed tree-edit-distance scorer (rs#527's TED instrument).

Joins the trees-mode runner outputs against the corruption manifest
and scores, per corrupted case, the tree edit distance between the
CLEAN document's value tree and the engine's RECOVERED tree — both
pruned to a window around the corruption site. Ordered-forest edit
distance (Zhang-Shasha); rename costs 0 on equal node kinds, 1
otherwise; insert/delete cost 1. Normalized by the larger tree's
node count, so 0.0 = the window's structure is intact, 1.0 =
nothing survived.

Both engines are scored on the same rule: their trees come from their
own parsers (parsanol: the recovery envelope's shaped `partial`; the
ts runner: named non-error nodes, kinds as-is). The clean reference is
each engine's own parse of the clean document.

Usage: score_ted.py <manifest.json> <parsanol-trees> <ts-trees>
"""
import json
import statistics
import sys
from pathlib import Path

WINDOW = 1024
MAX_NODES = 80  # cap per pruned tree (closest to the site wins)


def loads_trees(path):
    rows = {}
    for line in Path(path).read_text().splitlines():
        if not line.strip():
            continue
        row = json.loads(line)
        rows[row["name"]] = row
    return rows


def prune(node, lo, hi, budget):
    """Keep the subtree of nodes overlapping [lo, hi), nearest-first,
    up to budget nodes. Returns a (kind, kids-tuple) tree or None."""
    kept = [0]

    def walk(node):
        # Both wire shapes normalize here: the ts nodes carry
        # {"kind","start","end","kids"}; the parsanol shaped tree mixes
        # rule-name hashes, arrays and slice leaves.
        if isinstance(node, list):
            kind, start, end, kids = "list", None, None, node
        elif isinstance(node, dict):
            if "kind" in node:
                kind = node["kind"]
                start, end = node.get("start"), node.get("end")
                kids = node.get("kids", [])
            elif "offset" in node:
                # slice leaf: window-check on its own offset (+length
                # when the wire carries one)
                kind = "tok"
                start = node["offset"]
                end = start + node.get("length", 1)
                kids = []
            else:
                kind, start, end, kids = "hash", None, None, list(node.values())
        else:
            kind, start, end, kids = "atom", None, None, []
        in_window = start is None or (start < hi and end > lo)
        if not in_window:
            return None
        if kept[0] >= budget:
            return None
        kept[0] += 1
        pruned_kids = []
        for child in kids:
            if kept[0] >= budget:
                break
            pruned = walk(child)
            if pruned is not None:
                pruned_kids.append(pruned)
        return (kind, tuple(pruned_kids))

    return walk(node)



def postorder(tree):
    """Post-ordered nodes with leftmost-leaf and parent pointers."""
    order = []
    lml = {}
    parent = {}

    leftmost = {}

    def visit(node, par):
        parent[id(node)] = par
        first = None
        for child in node[1]:
            visit(child, node)
            if first is None:
                first = child
        order.append(node)
        # The leftmost leaf: the first child's (a leaf's own).
        leftmost[id(node)] = leftmost[id(first)] if first is not None else node

    visit(tree, None)
    return order, leftmost, parent


def zs_distance(a, b, rename_cost):
    """Zhang-Shasha between two rooted ordered trees."""
    if a is None or b is None:
        return 1  # nothing on one side of the window
    A, lmlA, parentA = postorder(a)
    B, lmlB, parentB = postorder(b)
    n, m = len(A), len(B)
    # Postorder indexes (1-based); the parent maps key on object ids
    # and translate to indexes here.
    idxA = {id(node): i + 1 for i, node in enumerate(A)}
    idxB = {id(node): j + 1 for j, node in enumerate(B)}
    pa = {idxA[id(node)]: (idxA[id(parentA[id(node)])] if parentA[id(node)] is not None else 0) for node in A}
    pb = {idxB[id(node)]: (idxB[id(parentB[id(node)])] if parentB[id(node)] is not None else 0) for node in B}
    lrA = {idxA[id(node)]: idxA[id(lmlA[id(node)])] for node in A}
    lrB = {idxB[id(node)]: idxB[id(lmlB[id(node)])] for node in B}
    ka = {idxA[id(node)]: node[0] for node in A}
    kb = {idxB[id(node)]: node[0] for node in B}

    td = {}

    def treedist(i, j):
        li, lj = lrA[i], lrB[j]
        prev = {}
        for x in range(li, i + 1):
            prev[(x, lj - 1)] = x - li + 1
        for y in range(lj, j + 1):
            prev[(li - 1, y)] = y - lj + 1
        for x in range(li, i + 1):
            for y in range(lj, j + 1):
                xl, yl = lrA[x], lrB[y]
                if xl == li and yl == lj:
                    prev[(x, y)] = min(
                        prev.get((x - 1, y), 0) + 1,
                        prev.get((x, y - 1), 0) + 1,
                        prev.get((x - 1, y - 1), 0) + rename_cost(ka[x], kb[y]),
                    )
                    td[(x, y)] = prev[(x, y)]
                else:
                    prev[(x, y)] = min(
                        prev.get((x - 1, y), 0) + 1,
                        prev.get((x, y - 1), 0) + 1,
                        prev.get((xl - 1, yl - 1), 0) + td.get((xl, yl), 0),
                    )
        return td[(i, j)]

    # Key roots, ascending: nested subproblems resolve in order.
    krs_a = sorted(
        {i for i in range(1, n + 1) if lrA[i] == i or (pa[i] and lrA[pa[i]] != lrA[i])} | {n}
    )
    krs_b = sorted(
        {j for j in range(1, m + 1) if lrB[j] == j or (pb[j] and lrB[pb[j]] != lrB[j])} | {m}
    )
    for i in krs_a:
        for j in krs_b:
            treedist(i, j)
    return td[(n, m)]
def main():
    manifest = json.loads(Path(sys.argv[1]).read_text())
    pars = loads_trees(sys.argv[2])
    ts_rows = loads_trees(sys.argv[3])

    per_class = {}
    for case in manifest["cases"]:
        name = case["file"]
        site = min(case["corruption_sites"])
        cls, depth = case["class"], case["depth"]
        lo, hi = max(0, site - WINDOW), site + WINDOW
        p, t = pars.get(name), ts_rows.get(name)
        if not p or not t:
            continue
        prow = per_class.setdefault(
            (cls, depth), {"n": 0, "p": [], "t": []}
        )
        prow["n"] += 1
        for label, row in (("p", p), ("t", t)):
            clean = prune(row["clean"], lo, hi, MAX_NODES)
            recovered = prune(row["recovered"], lo, hi, MAX_NODES)
            # offset-shape trees (parsanol wire) have no start/end on
            # containers — the prune walks slices only; containers pass
            # through (in_window None -> kept). Depth/size identical.
            dist = zs_distance(recovered, clean, lambda x, y: 0 if x == y else 1)
            size = max(
                _count(clean) or 1,
                _count(recovered) or 1,
            )
            prow[label].append(min(dist / max(size, 1), 1.0))

    print("| class | depth | n | parsanol windowed nTED | tree-sitter windowed nTED |")
    print("|---|---|---|---|---|")
    for (cls, depth), row in sorted(per_class.items()):
        pm = statistics.mean(row["p"]) if row["p"] else 1.0
        tm = statistics.mean(row["t"]) if row["t"] else 1.0
        print(f"| {cls} | {depth} | {row['n']} | {pm:.3f} | {tm:.3f} |")
    allp = [x for r in per_class.values() for x in r["p"]]
    allt = [x for r in per_class.values() for x in r["t"]]
    print()
    print(
        f"TOTAL windowed nTED: parsanol {statistics.mean(allp):.3f} "
        f"| tree-sitter {statistics.mean(allt):.3f} (lower is better; "
        f"window ±{WINDOW}B, ≤{MAX_NODES} nodes per tree)"
    )


def _count(tree):
    if tree is None:
        return 0
    return 1 + sum(_count(k) for k in tree[1])


if __name__ == "__main__":
    main()
