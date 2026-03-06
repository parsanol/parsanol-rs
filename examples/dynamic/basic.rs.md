# Dynamic Atoms - Rust Implementation

## How to Run

```bash
cargo run --example dynamic/basic
```

## Code Walkthrough

### Registering a Callback

Callbacks must implement `DynamicCallback` and be registered:

```rust
use parsanol::portable::dynamic::{DynamicCallback, DynamicContext, register_dynamic_callback};

struct MyCallback;

impl DynamicCallback for MyCallback {
    fn call(&self, ctx: &DynamicContext) -> Option<Atom> {
        // Return atom based on context
        Some(Atom::Str { pattern: "hello".into() })
    }
}

let callback_id = register_dynamic_callback(Box::new(MyCallback));
```

### Using in Grammar

```rust
let grammar = GrammarBuilder::new()
    .rule("dynamic", dynamic_with_id(callback_id))
    .build();
```

### DynamicContext API

Callbacks receive context with parsing state:

```rust
impl DynamicCallback for MyCallback {
    fn call(&self, ctx: &DynamicContext) -> Option<Atom> {
        // Current position in input
        let pos = ctx.pos();

        // Full input string
        let input = ctx.input();

        // Access captures made so far
        if let Some(lang) = ctx.get_capture("language") {
            // Use capture to determine behavior
        }

        // Return the atom to use at this position
        Some(Atom::Str { pattern: "hello".into() })
    }
}
```

### Context-Sensitive Keywords

Different keywords based on preceding context:

```rust
struct LanguageKeywordCallback;

impl DynamicCallback for LanguageKeywordCallback {
    fn call(&self, ctx: &DynamicContext) -> Option<Atom> {
        let input = ctx.input();
        let pos = ctx.pos();

        // Look at preceding context
        if pos >= 5 && &input[pos - 5..pos] == "ruby " {
            Some(Atom::Str { pattern: "def".into() })
        } else if pos >= 7 && &input[pos - 7..pos] == "python " {
            Some(Atom::Str { pattern: "lambda".into() })
        } else {
            Some(Atom::Str { pattern: "function".into() })
        }
    }
}
```

### Capture-Aware Decisions

Use previously made captures to determine parsing:

```rust
struct TypeAwareCallback;

impl DynamicCallback for TypeAwareCallback {
    fn call(&self, ctx: &DynamicContext) -> Option<Atom> {
        match ctx.get_capture("type") {
            Some("int") => Some(Atom::Regex { pattern: r"\d+".into() }),
            Some("str") => Some(Atom::Regex { pattern: r#"[^"]*"#.into() }),
            Some("bool") => Some(Atom::Alternative {
                atoms: vec![
                    Atom::Str { pattern: "true".into() },
                    Atom::Str { pattern: "false".into() },
                ],
            }),
            _ => None,
        }
    }
}
```

### Configuration-Driven Parsing

Change behavior based on configuration:

```rust
struct ConfigCallback {
    strict_mode: bool,
}

impl DynamicCallback for ConfigCallback {
    fn call(&self, _ctx: &DynamicContext) -> Option<Atom> {
        if self.strict_mode {
            Some(Atom::Regex { pattern: r"[a-z][a-z0-9_]*".into() })
        } else {
            Some(Atom::Regex { pattern: r"[a-zA-Z_][a-zA-Z0-9_]*".into() })
        }
    }
}
```

## Output Types

Callbacks return `Option<Atom>`:
- `Some(atom)` - Use this atom at the current position
- `None` - Fail at this position (no match)

```rust
pub enum Atom {
    Str { pattern: String },
    Regex { pattern: String },
    Sequence { atoms: Vec<usize> },
    Alternative { atoms: Vec<usize> },
    Repetition { atom: usize, min: u32, max: Option<u32> },
    // ... other variants
}
```

## Backend Compatibility

| Backend | Support | Notes |
|---------|---------|-------|
| Packrat | Native | Direct callback invocation |
| Bytecode | Fallback | Uses Packrat internally |
| Streaming | Fallback | Uses Packrat internally |

**Important**: In Bytecode and Streaming backends, dynamic atoms use Packrat fallback.
For heavy dynamic usage, prefer the Packrat backend.

## Design Decisions

### Thread Safety

Callbacks must be `Send + Sync`:

```rust
pub trait DynamicCallback: Send + Sync {
    fn call(&self, ctx: &DynamicContext) -> Option<Atom>;
}
```

The callback registry uses `parking_lot::RwLock` for thread safety.

### Callback Registration

Callbacks are registered globally and referenced by ID:

```rust
let callback_id: u64 = register_dynamic_callback(Box::new(callback));
```

- IDs are unique and never reused
- Registration is O(1)
- No unregistration (callbacks live for program duration)

### Context Access

`DynamicContext` provides read-only access:
- Position is immutable
- Input is immutable
- Captures are immutable

This ensures callbacks don't modify parser state.

## Performance Notes

| Backend | Overhead |
|---------|----------|
| Packrat | ~5% per dynamic atom |
| Bytecode | ~20% slower (fallback) |
| Streaming | ~20% slower (fallback) |

**Optimization Tips**:
1. Keep callbacks fast - no I/O or heavy computation
2. Cache expensive computations outside the callback
3. Use Packrat backend for heavy dynamic usage
4. Consider grammar restructuring to avoid dynamic atoms

## When to Use Dynamic Atoms

**Use when**:
- Parsing behavior depends on configuration
- Keywords change based on position
- Type information affects parsing
- Building plugin systems

**Avoid when**:
- Static grammar can express the pattern
- Performance is critical
- Simpler alternatives exist
