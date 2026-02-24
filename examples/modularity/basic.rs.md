# Grammar Modularity - Rust Implementation

## How to Run

```bash
cargo run --example modularity/basic --no-default-features
```

## Code Walkthrough

### Module Functions

Each module is a function returning rules:

```rust
fn build_module_a() -> Vec<(&'static str, Parslet)> {
    let mut rules = Vec::new();
    rules.push(("a_language", re(r"aaa")));
    rules
}
```

Functions encapsulate related rules. Return type is explicit for clarity.

### Grammar Composition

Modules are combined in the main builder:

```rust
fn build_grammar() -> Grammar {
    let mut builder = GrammarBuilder::new();

    // Import rules from module A
    for (name, parslet) in build_module_a() {
        builder = builder.rule(name, parslet);
    }

    // Import rules from module B
    for (name, parslet) in build_module_b() {
        builder = builder.rule(name, parslet);
    }
    // ...
}
```

Iteration over module rules adds them to the combined grammar.

### Root Rule with Choices

The root combines module entry points:

```rust
builder = builder.rule(
    "root",
    choice(vec![
        dynamic(seq(vec![
            dynamic(str("a(")),
            dynamic(ref_("a_language")),
            dynamic(str(")")),
        ])),
        // b_language, c_language...
    ]),
);
```

Each alternative invokes a different module's rule.

### Trait-Based Approach (Alternative)

Traits provide type-safe composition:

```rust
trait GrammarModule {
    fn name(&self) -> &str;
    fn add_rules(&self, builder: &mut GrammarBuilder);
    fn entry_point(&self) -> &str;
}

struct ModuleA;
impl GrammarModule for ModuleA {
    fn name(&self) -> &str { "A" }
    fn add_rules(&self, builder: &mut GrammarBuilder) {
        builder.rule("a_language", re(r"aaa"));
    }
    fn entry_point(&self) -> &str { "a_language" }
}
```

Traits enable static dispatch and compile-time checks.

### Macro-Based Approach (Alternative)

Macros provide declarative composition:

```rust
macro_rules! compose_grammar {
    (modules: [$($mod:ty),*], root: $root:expr) => {{
        let mut builder = GrammarBuilder::new();
        $(
            <$mod as GrammarModule>::add_rules(&$mod::default(), &mut builder);
        )*
        builder.root($root);
        builder.build()
    }};
}
```

Macros generate boilerplate at compile time.

## Design Decisions

### Why Function-Based Modules?

Functions are simple and work with any builder pattern. No trait boilerplate for small grammars.

### Why Trait Alternative?

Traits provide better encapsulation and type safety for larger projects. They enable dependency injection and testing.

### Why Macro Alternative?

Macros eliminate runtime overhead. Rules are generated at compile time for maximum performance.

### Why Separate Modules?

Module boundaries match conceptual divisions in the language. Changes to one module don't affect others.

## Output

The example demonstrates three composition approaches:
1. Function-based: Simple, flexible, runtime composition
2. Trait-based: Type-safe, testable, extensible
3. Macro-based: Zero-cost, declarative, compile-time
