//! Integration tests for the Transform system
//!
//! These tests cover pattern matching, rule application, and AST to Value conversion.

use parsanol::portable::{
    parser_dsl::{re, str, GrammarBuilder},
    transform::{ast_to_value, Pattern, Transform, Value},
    AstArena, PortableParser,
};

// ============================================================================
// Value Type Tests
// ============================================================================

#[test]
fn test_value_nil() {
    let v = Value::nil();
    assert!(v.is_nil());
}

#[test]
fn test_value_bool() {
    let v_true = Value::bool(true);
    let v_false = Value::bool(false);

    assert_eq!(v_true.as_bool(), Some(true));
    assert_eq!(v_false.as_bool(), Some(false));
}

#[test]
fn test_value_int() {
    let v = Value::int(42);
    assert_eq!(v.as_int(), Some(42));
}

#[test]
fn test_value_float() {
    let v = Value::float(1.5);
    assert_eq!(v.as_float(), Some(1.5));
}

#[test]
fn test_value_string() {
    let v = Value::string("hello");
    assert_eq!(v.as_str(), Some("hello"));
}

#[test]
fn test_value_array() {
    let v = Value::array(vec![Value::int(1), Value::int(2), Value::int(3)]);
    let arr = v.as_array().unwrap();
    assert_eq!(arr.len(), 3);
}

#[test]
fn test_value_hash() {
    let v = Value::hash(vec![
        ("name", Value::string("Alice")),
        ("age", Value::int(30)),
    ]);
    let hash = v.as_hash().unwrap();
    assert_eq!(hash.get("name").unwrap().as_str(), Some("Alice"));
    assert_eq!(hash.get("age").unwrap().as_int(), Some(30));
}

// ============================================================================
// Pattern Matching Tests
// ============================================================================

#[test]
fn test_pattern_simple_match_int() {
    let pattern = Pattern::simple("x");
    let value = Value::int(42);

    let bindings = pattern.match_value(&value).expect("Should match int");
    assert!(bindings.get_int("x").is_ok());
    assert_eq!(bindings.get_int("x").unwrap(), 42);
}

#[test]
fn test_pattern_simple_match_string() {
    let pattern = Pattern::simple("s");
    let value = Value::string("hello");

    let bindings = pattern.match_value(&value).expect("Should match string");
    assert!(bindings.get_string("s").is_ok());
}

#[test]
fn test_pattern_str_exact_match() {
    let pattern = Pattern::str("+");
    let value = Value::string("+");

    assert!(pattern.match_value(&value).is_some());
}

#[test]
fn test_pattern_str_no_match() {
    let pattern = Pattern::str("+");
    let value = Value::string("-");

    assert!(pattern.match_value(&value).is_none());
}

#[test]
fn test_pattern_int_exact_match() {
    let pattern = Pattern::int(42);
    let value = Value::int(42);

    assert!(pattern.match_value(&value).is_some());
}

#[test]
fn test_pattern_int_no_match() {
    let pattern = Pattern::int(42);
    let value = Value::int(43);

    assert!(pattern.match_value(&value).is_none());
}

#[test]
fn test_pattern_sequence_match() {
    let pattern = Pattern::sequence("items");
    let value = Value::array(vec![Value::int(1), Value::int(2), Value::int(3)]);

    let bindings = pattern.match_value(&value).expect("Should match array");
    let items = bindings.get("items").unwrap();
    assert_eq!(items.as_array().unwrap().len(), 3);
}

#[test]
fn test_pattern_hash_match() {
    let pattern = Pattern::hash()
        .field("left", "l")
        .field("right", "r")
        .build();

    let value = Value::hash(vec![
        ("left", Value::int(1)),
        ("right", Value::int(2)),
        ("extra", Value::string("ignored")), // Extra fields are OK (default)
    ]);

    let bindings = pattern.match_value(&value).expect("Should match hash");
    assert!(bindings.get_int("l").is_ok());
    assert!(bindings.get_int("r").is_ok());
}

#[test]
fn test_pattern_hash_missing_field() {
    let pattern = Pattern::hash().field("required", "r").build();

    let value = Value::hash(vec![("other", Value::int(1))]);

    assert!(
        pattern.match_value(&value).is_none(),
        "Should not match without required field"
    );
}

#[test]
fn test_pattern_hash_strict() {
    let pattern = Pattern::hash().field("a", "x").strict().build();

    let matching = Value::hash(vec![("a", Value::int(1))]);
    assert!(pattern.match_value(&matching).is_some());

    let with_extra = Value::hash(vec![("a", Value::int(1)), ("b", Value::int(2))]);
    assert!(
        pattern.match_value(&with_extra).is_none(),
        "Should not match with extra fields in strict mode"
    );
}

#[test]
fn test_pattern_subtree_match() {
    let pattern = Pattern::subtree("node");

    // Should match anything
    assert!(pattern.match_value(&Value::int(42)).is_some());
    assert!(pattern.match_value(&Value::string("hello")).is_some());
    assert!(pattern.match_value(&Value::nil()).is_some());
}

// ============================================================================
// Transform Rule Tests
// ============================================================================

#[test]
fn test_transform_rule_int() {
    let transform = Transform::new().rule("int", |v| {
        let n = v.as_int().ok_or_else(|| {
            parsanol::portable::transform::TransformError::Custom("not an int".into())
        })?;
        Ok(Value::int(n * 2))
    });

    let value = Value::hash(vec![("int", Value::int(21))]);
    let result = transform.apply(&value).expect("Transform should succeed");
    assert_eq!(result.as_int(), Some(42));
}

#[test]
fn test_transform_chain() {
    let transform = Transform::new()
        .rule("a", |_| Ok(Value::int(1)))
        .rule("b", |_| Ok(Value::int(2)));

    // First matching rule wins
    let value = Value::hash(vec![("a", Value::nil())]);
    let result = transform.apply(&value).expect("Transform should succeed");
    assert_eq!(result.as_int(), Some(1));
}

// ============================================================================
// AST to Value Conversion Tests
// ============================================================================

#[test]
fn test_ast_to_value_string() {
    let grammar = GrammarBuilder::new().rule("text", str("hello")).build();

    let input = "hello";
    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(&grammar, input, &mut arena);
    let ast = parser.parse().expect("Should parse");

    let value = ast_to_value(&ast, &arena, input);
    assert_eq!(value.as_str(), Some("hello"));
}

#[test]
fn test_ast_to_value_number() {
    let grammar = GrammarBuilder::new().rule("num", re(r"[0-9]+")).build();

    let input = "42";
    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(&grammar, input, &mut arena);
    let ast = parser.parse().expect("Should parse");

    let value = ast_to_value(&ast, &arena, input);
    assert_eq!(value.as_str(), Some("42"));
}

// ============================================================================
// Complex Pattern Tests
// ============================================================================

#[test]
fn test_pattern_nested_hash() {
    // Pattern: { expr: { left: simple(:l), op: simple(:o), right: simple(:r) } }
    let inner_pattern = Pattern::hash()
        .field("left", "l")
        .field("op", "o")
        .field("right", "r")
        .build();

    let outer_pattern = Pattern::hash().field_pattern("expr", inner_pattern).build();

    let value = Value::hash(vec![(
        "expr",
        Value::hash(vec![
            ("left", Value::int(1)),
            ("op", Value::string("+")),
            ("right", Value::int(2)),
        ]),
    )]);

    let bindings = outer_pattern
        .match_value(&value)
        .expect("Should match nested hash");
    assert!(bindings.get_int("l").is_ok());
    assert!(bindings.get_string("o").is_ok());
    assert!(bindings.get_int("r").is_ok());
}

#[test]
fn test_pattern_bool_and_nil() {
    let bool_pattern = Pattern::bool(true);
    assert!(bool_pattern.match_value(&Value::bool(true)).is_some());
    assert!(bool_pattern.match_value(&Value::bool(false)).is_none());

    let nil_pattern = Pattern::nil();
    assert!(nil_pattern.match_value(&Value::nil()).is_some());
    assert!(nil_pattern.match_value(&Value::int(0)).is_none());
}

// ============================================================================
// Bindings Tests
// ============================================================================

#[test]
fn test_bindings_get_methods() {
    let pattern = Pattern::hash()
        .field("int_val", "i")
        .field("str_val", "s")
        .field("bool_val", "b")
        .field("float_val", "f")
        .build();

    let value = Value::hash(vec![
        ("int_val", Value::int(42)),
        ("str_val", Value::string("hello")),
        ("bool_val", Value::bool(true)),
        ("float_val", Value::float(1.5)),
    ]);

    let bindings = pattern.match_value(&value).expect("Should match");

    assert!(bindings.get_int("i").is_ok());
    assert!(bindings.get_string("s").is_ok());
    assert!(bindings.get_bool("b").is_ok());
    assert!(bindings.get_float("f").is_ok());
}
