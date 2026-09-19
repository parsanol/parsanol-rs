# The parsanol event stream

The event stream is a flat, linear encoding of the parslet-shaped AST
(the same tree `Parsanol::Native.parse` returns) designed for
consumers that do not want the intermediate Hash/Array tree: one FFI
call returns the whole stream, and a model builder attaches domain
handling directly to the opcodes.

```ruby
events_blob, strings = Parsanol::Native::Parser.parse_events(grammar, input)
events = events_blob.unpack("q*")   # opcode words (Integers)
```

`events_blob` is a single binary string (one Ruby allocation); `unpack`
decodes it in one C call. `strings` is a small deduplicated pool of
rule names and interned literals.

## Opcode table

Each event consumes a fixed number of `i64` words after the opcode.

| op | name        | extra words        | meaning                            |
|----|-------------|--------------------|------------------------------------|
| 0  | NIL         | –                  | `nil` value                        |
| 1  | STR         | 1 (pool index)     | interned string value              |
| 2  | SLICE       | 2 (offset, length) | input slice (text with position)   |
| 3  | KEY         | 1 (pool index)     | hash key; the value events follow  |
| 4  | BEGIN_ARR   | –                  | array start                        |
| 5  | END_ARR     | –                  | array end                          |
| 6  | BEGIN_HASH  | –                  | hash start                         |
| 7  | END_HASH    | –                  | hash end                           |

Hash nodes emit `BEGIN_HASH`, then one `KEY` event per entry followed
by that entry's value events, then `END_HASH`. The explicit delimiters
keep an array of single-key hashes (a repetition of named captures)
distinct from one multi-key hash (a merged sequence) — the two shapes
carry different parslet semantics and consumers rely on the
distinction.

Zero-length input slices are emitted as `STR` of `""`: they carry no
text, and the native hydration path renders them as plain strings.

The semantics of the stream are exactly those of
`to_parslet_compatible` (sequence flattening, repetition vs wrapper
patterns, maybe flattening) — see `portable/parslet_transform.rs` and
its conformance suite.

## Reference consumer

`Parsanol::Native::EventPlayer` replays a stream back into the exact
tree `Native.parse` produces (including `Slice` offsets and symbol
keys). It documents the protocol and serves as the conformance oracle;
production builders should consume the opcodes directly.

```ruby
tree = Parsanol::Native::EventPlayer.play(events, strings, input)
```

## Conformance

Event replay equals `parse_native` output on the full `spec/syntax`
EXPRESS corpus (15/15 fixtures), verified including `Slice` offsets,
symbol keys and empty-string semantics. When changing the transform or
the encoder, re-run:

```sh
bundle exec ruby -I lib -e '
  require "parsanol"; require "parsanol/native"
  # ... replay a fixture and compare against Parsanol::Native.parse
'
```

## Rust API

`parsanol::portable::events::{linearize_events, replay_events}`
serialize and reconstruct arena trees; the module also exports the
opcode constants. `PortableParser` users can parse as usual and
linearize the shaped result — the same path the Ruby FFI uses
(`_parse_handle_events`, registered grammars, compile-once).
