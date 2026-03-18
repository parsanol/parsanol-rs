# Parsanol-rs PR: Single `parse()` API with Lazy Line/Column

## Summary

Simplifies the API to a **single `parse()` method** with **lazy line/column computation** in Slice objects.

### The Problem

Previous API had too many confusing methods:
- `parse_parslet(grammar_json, input)` - fast, no positions
- `parse_parslet_with_positions(grammar_json, input, position_cache)` - with positions
- `parse_with_transform(grammar_json, input, position_cache)` - deprecated alias
- `parse_to_objects(grammar_json, input, type_map)` - zero copy

Users had to decide upfront whether they needed line/column info, and the position cache added overhead even if never used.

### The Solution

**Single method with lazy line/column:**

```ruby
# Just this - no decisions, no options
result = Parsanol::Native.parse(grammar_json, input)

# Line/column computed LAZILY only when requested
slice = result[:name]
slice.offset            # => 42 (always available, zero cost)
slice.content           # => "hello" (always available, zero cost)
slice.line_and_column   # => [5, 1] (computed on first call, then cached)
```

## API Comparison

| Before | After |
|--------|-------|
| `parse_parslet(g, i)` | `parse(g, i)` |
| `parse_parslet_with_positions(g, i, cache)` | `parse(g, i)` |
| `parse_with_transform(g, i, cache)` | `parse(g, i)` |
| `parse_to_objects(g, i, map)` | `parse(g, i)` |

## How Lazy Line/Column Works

The Slice object stores:
- `offset` - character position in source
- `content` - the string value
- `input` - reference to original input string

When `line_and_column` is called:
1. Scan backwards from offset to count newlines
2. Find line start position
3. Calculate column
4. Cache result for future calls

**Performance:** O(line_length) which is microseconds for normal lines.

## Files Changed

| File | Change |
|------|--------|
| `parser.rs` | Single `parse()` function, removed `parse_with_positions` |
| `normalize.rs` | **NEW** - Universal AST normalization, creates Slices with input ref |
| `init.rs` | Exports only `parse`, `parse_batch`, `parse_with_builder` |
| `mod.rs` | Updated exports |

## Performance

| Operation | Before | After |
|-----------|--------|-------|
| Parse (no line/col) | Fast | **Same** (zero overhead) |
| Parse + line/col | Required cache upfront | **Lazy** (no cost unless used) |
| Line/col lookup | O(1) with pre-built cache | O(line_length) on first call |
| Subsequent lookups | O(1) | O(1) (cached) |

## Benefits

- ✅ **Single method**: `parse(grammar_json, input)` - that's it
- ✅ **Zero decisions**: No need to decide upfront about positions
- ✅ **Zero overhead**: No cost unless you call `line_and_column`
- ✅ **Always available**: Line/column always accessible
- ✅ **Simple caching**: Result cached after first call
- ✅ **Clean code**: Removed ~150 lines of redundant API code

## Usage Example

```ruby
require 'parsanol/native'

# Build and serialize grammar once
grammar = str('hello').as(:greeting) >> str(' ').maybe >> match('[a-z]').repeat(1).as(:name)
grammar_json = Parsanol::Native.serialize_grammar(grammar)

# Parse - simple and clean
result = Parsanol::Native.parse(grammar_json, "hello world")
# => {greeting: "hello"@0, name: "world"@6}

# Line/column available when needed
result[:greeting].line_and_column  # => [1, 1]
result[:name].line_and_column      # => [1, 7]
```

## Breaking Changes

All old methods are removed:
- `parse_parslet` → use `parse`
- `parse_parslet_with_positions` → use `parse`
- `parse_with_transform` → use `parse`
- `parse_to_objects` → use `parse`

## Testing

```bash
cd parsanol-rs
cargo test
cd ../parsanol-ruby
bundle exec rake compile
bundle exec rspec spec/
```
