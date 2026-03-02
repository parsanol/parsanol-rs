//! Integration tests for the FromAst derive macro

use parsanol::derive::FromAst;
use parsanol::portable::transform::Value;
use std::convert::TryInto;

// ============================================================================
// Basic struct tests - these work
// ============================================================================

#[derive(Debug, PartialEq, FromAst)]
struct Point {
    x: i64,
    y: i64,
}

#[test]
fn test_struct_from_hash() {
    let value = Value::hash(vec![
        ("x".to_string(), Value::int(10)),
        ("y".to_string(), Value::int(20)),
    ]);

    let point: Point = value.try_into().unwrap();
    assert_eq!(point.x, 10);
    assert_eq!(point.y, 20);
}

// ============================================================================
// Tuple struct tests
// ============================================================================

#[derive(Debug, PartialEq, FromAst)]
struct Wrapper(i64);

#[test]
fn test_tuple_struct_from_value() {
    let value = Value::int(42);
    let wrapper: Wrapper = value.try_into().unwrap();
    assert_eq!(wrapper.0, 42);
}

// ============================================================================
// Unit struct tests
// ============================================================================

#[derive(Debug, PartialEq, FromAst)]
struct UnitStruct;

#[test]
fn test_unit_struct() {
    let value = Value::nil();
    let _unit: UnitStruct = value.try_into().unwrap();
}
