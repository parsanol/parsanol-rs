# parsanol-tree/v2 — the frozen output-tree contract

Status: **frozen**. Every binder (ruby, ts/wasm, rs) materializes this
shape; the artifact envelope's `shape` field names it. Changing anything
below requires a new version string (`parsanol-tree/v3`) and a new
engine family — artifacts declaring v2 must parse identically forever.

## Node kinds

A parse result is a tree of exactly these node kinds:

- **InputRef** — `{offset, length}` into the source input; the leaf for
  every consumed span. Line/column are derived, never stored.
- **TaggedArray** — `{tag, items[]}`; the tag is one of the frozen
  vocabulary below.
- **Hash** — capture envelope `{name => node}`; produced by `as <name>`.
- **Nil** — the absent marker (empty optional, failed-side lookahead
  that still succeeds its negation).

No other kinds exist. Binders must reject trees containing anything else.

## Frozen tag vocabulary

| tag | producer | flattening semantics |
|---|---|---|
| `:sequence` | a rule body sequence | items concatenate |
| `:repetition` | `x*` / `x+` | flattens to an array |
| `:maybe` | `x.repeat(0,1)` absent case | flattens to nil (named) or `""` (unnamed) |

## Capture Hash semantics

- One `as` per name per rule invocation: the hash holds `name => node`.
- **Repeated sibling captures with the same name collapse to an array**
  under that key (the #36 rule, identical in every engine).
- A capture inside an alternation that did not run is simply absent —
  never `null`.

## Maybe semantics

- A *present* optional (`x.repeat(0,1)` with one match) flattens to the
  value itself in every context.
- An *absent* optional keeps the `:maybe` tag so downstream flattening
  yields nil (named) or `""` (unnamed) — never an empty array.

## Wire encoding

The flat batch encoding (C ABI) and the JSON tree agree on offsets:
offsets are **byte** offsets into the UTF-8 input. Binders computing
line/column must do so from byte offsets with the same algorithm.

## Validation duty

Loaders reject an artifact whose `shape` field is not exactly
`"parsanol-tree/v2"` with an error naming this contract. Deep tree
validation (beyond kind checking) remains the differential test suites'
job; the loader's check is a version negotiation, not a schema walk.
