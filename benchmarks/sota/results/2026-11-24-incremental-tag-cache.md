# The #528 incremental profile + the first lever (2026-11-24)

## The retained bench (911 KiB doc, 100 interleaved keystrokes,
## min-of-tier, this host)

| tier | before | after the tag cache |
|---|---:|---:|
| snapshot mean | 27.42 ms | 26.33 ms |
| retained mean | 14.61 ms | **13.87 ms** |
| retained p95 | 27.46 ms | 25.90 ms |
| speedup mean | 1.88x | **1.90x** |
| early band | 1.23x | **1.26x** |
| late band | 4.26x | 4.25x |

## The profile's finding

The sample over the retained path: the tree-build's
`store_tagged_array` → `find_interned_string_with_offset` → the
SipHash of the tag STRING per array store — the `:sequence`/
`:repetition` tags fire per ITERATION/ELEMENT on the classic path.
The VM cached its internd tag indexes (`tag_seq`/`tag_rep`) from
day one; the classic parser never did.

## The lever (shipped with this receipt)

The classic parser caches the internd tag indexes (the same
pattern): `store_tagged_cached(TagKind, items)` — three `Option<u32>`
slots, one `intern_string_index` per tag per parser, the indexed
store variant per call. ~5% off both tiers.

## The honest arch position

The lever is real but small — the 30-70x tree-sitter gap is the
ARCHITECTURE: the chain-based splice re-parses the affected
container's memo chain (the early band 1.26x = the chain ≈ most of
the doc), while tree-sitter's concrete-tree replay is
O(affected nodes). The next program: the concrete-tree incremental
model (#528's core) — the retained-tree API (shipped) is its base;
the profile above is its opening receipt.
