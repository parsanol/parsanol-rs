# Incremental Parsing - Rust Implementation

## How to Run

```bash
cd parsanol-rs
cargo run --example incremental/basic --no-default-features
```

## Code Walkthrough

### Region Structure

Track parsed content with positions:

```rust
struct Region {
    start: usize,
    end: usize,
    rule: String,
    valid: bool,
}
```

Each region knows its position and validity status.

### Initial Parse

Parse and record all regions:

```rust
fn parse_full(&mut self, input: &str) {
    // Build regions based on structure
    for line in input.lines() {
        self.regions.push(Region {
            start: line_start,
            end: line_end,
            rule: "line",
            valid: true,
        });
    }
}
```

Store regions for later incremental updates.

### Edit Application

Apply edit and find affected regions:

```rust
fn apply_edit(&mut self, pos: usize, delete_len: usize, insert: &str) {
    // Find affected region range
    let affected_start = self.find_region_start(pos);
    let affected_end = self.find_region_end(pos + delete_len);

    // Mark affected regions as invalid
    for region in &mut self.regions {
        if region.start >= affected_start && region.end <= affected_end {
            region.valid = false;
        }
    }
}
```

Only mark affected regions for reparse.

### Incremental Reparse

Reparse only invalid regions:

```rust
fn reparse_invalid(&mut self) {
    for region in &mut self.regions {
        if !region.valid {
            // Reparse only this region
            region.valid = true;
        }
    }
}
```

O(affected) instead of O(entire input).

### Position Adjustment

Update positions after edits:

```rust
let delta = insert.len() as isize - delete_len as isize;
for region in &mut self.regions {
    if region.start > pos + delete_len {
        region.start = (region.start as isize + delta) as usize;
        region.end = (region.end as isize + delta) as usize;
    }
}
```

Maintain correct positions after insertions/deletions.

## Output Types

```
Initial Parse:
  Parsed 3 regions

Apply Edit:
  Insert 'Beautiful ' at position 6
  Affected regions: 1-2

Incremental Reparse:
  Reparsing 1 invalid regions
  Done!
```

## Design Decisions

### Why Regions?

Region-based tracking allows identifying exactly what needs reparsing. Finer granularity = less work.

### Why Track Validity?

Boolean flag enables quick filtering of invalid regions without removing/recreating.

### When to Use

Use incremental parsing when:
- Building IDE/editor plugins
- Implementing live preview
- Creating collaborative editing tools
- Need fast response to user input

### Trade-offs

- More memory for tracking structures
- Complexity in position adjustment
- Requires structure-aware grammar
