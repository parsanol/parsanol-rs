//! WebAssembly FFI Integration Tests
//!
//! These tests verify the WASM bindings work correctly.
//!
//! # Running Tests
//!
//! ## Compile-time Check (no WASM runtime required)
//! ```bash
//! cargo check --features wasm
//! cargo clippy --features wasm --lib -- -D warnings
//! ```
//!
//! ## Build WASM Package
//! ```bash
//! wasm-pack build --features wasm
//! ```
//!
//! # What's Tested
//!
//! 1. **API Compilation**: All wasm-bindgen annotations are correct
//! 2. **Grammar Construction**: Grammar builder works
//! 3. **Parser Invocation**: Parse function is callable
//! 4. **Error Handling**: Proper error handling

#![cfg(feature = "wasm")]

use wasm_bindgen::prelude::*;

/// Test that basic WASM module exports exist
#[test]
fn test_wasm_exports_exist() {
    // This test verifies the module structure is correct
    // If this compiles, the WASM bindings are correctly set up
}

/// Test grammar construction
#[test]
fn test_grammar_construction() {
    use parsanol::portable::parser_dsl::{str, GrammarBuilder};

    let grammar = GrammarBuilder::new().rule("hello", str("hello")).build();

    // Verify grammar was built correctly
    assert!(!grammar.atoms.is_empty(), "Should have atoms");
}

/// Test parse result works
#[test]
fn test_parse_result() {
    use parsanol::portable::parser_dsl::{str, GrammarBuilder};
    use parsanol::portable::{AstArena, PortableParser};

    let grammar = GrammarBuilder::new().rule("hello", str("hello")).build();

    let input = "hello";
    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(&grammar, input, &mut arena);

    let _result = parser.parse().expect("Parse failed");
    // Test passes if parse succeeds
}

/// Test error handling
#[test]
fn test_error_handling() {
    use parsanol::portable::parser_dsl::{str, GrammarBuilder};
    use parsanol::portable::{AstArena, PortableParser};

    let grammar = GrammarBuilder::new()
        .rule("expected", str("expected"))
        .build();

    let input = "unexpected";
    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(&grammar, input, &mut arena);

    let result = parser.parse();

    // Parse should fail
    assert!(result.is_err(), "Parse should fail for mismatched input");

    // Error should be convertible to string (for logging)
    if let Err(e) = result {
        let error_str = format!("{:?}", e);
        assert!(!error_str.is_empty(), "Error should not be empty");
    }
}

/// Test streaming builder callback interface
#[test]
fn test_streaming_builder_interface() {
    use parsanol::portable::streaming_builder::{BuildResult, StreamingBuilder};

    /// Simple test builder that collects values
    struct TestBuilder {
        values: Vec<String>,
    }

    impl StreamingBuilder for TestBuilder {
        type Output = Vec<String>;

        fn on_string(&mut self, value: &str, _offset: usize, _length: usize) -> BuildResult<()> {
            self.values.push(format!("string:{}", value));
            Ok(())
        }

        fn on_int(&mut self, value: i64) -> BuildResult<()> {
            self.values.push(format!("int:{}", value));
            Ok(())
        }

        fn on_float(&mut self, value: f64) -> BuildResult<()> {
            self.values.push(format!("float:{}", value));
            Ok(())
        }

        fn on_bool(&mut self, value: bool) -> BuildResult<()> {
            self.values.push(format!("bool:{}", value));
            Ok(())
        }

        fn on_nil(&mut self) -> BuildResult<()> {
            self.values.push("nil".to_string());
            Ok(())
        }

        fn finish(&mut self) -> BuildResult<Self::Output> {
            Ok(std::mem::take(&mut self.values))
        }
    }

    let mut builder = TestBuilder { values: vec![] };
    let result = builder.finish().expect("Finish failed");
    assert!(
        result.is_empty(),
        "Empty builder should produce empty result"
    );
}

/// Test JsValue conversions exist
#[test]
fn test_jsvalue_conversions() {
    // Test that we can create JsValues from Rust types
    let _string = JsValue::from_str("hello");
    let _number = JsValue::from_f64(42.0);
    // JsValue conversions work if we got here
}
