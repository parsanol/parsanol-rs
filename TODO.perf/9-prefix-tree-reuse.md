# 9. Prefix-tree reuse (specced, unscheduled)

The measured ceiling of edit-span memo retention (items 4/8) is the
structural re-walk: every keystroke still traverses the whole rule
structure against memo hits (~2 hits/line on the KV benchmark,
~1.5–1.7x total). Single-digit-millisecond keystrokes on
hundred-KiB-plus documents require reusing the previous parse TREE
for the unaffected prefix — the tree-sitter design:

- Persist the previous AST (arena-backed) alongside the memo window.
- On edit at offset O: the reused prefix is the deepest ancestor
  chain whose span ends at or before O; splice its subtree, reparse
  the suffix, rebuild ancestors' containers along the right spine.
- Node identity/height bookkeeping decides splice points (a parent
  cannot be reused when the edit lands inside its span).

This is a parser-architecture change (persistent trees, not just
persistent memo), gated by the same tree-equality corpus as item 4.
