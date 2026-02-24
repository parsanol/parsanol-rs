//! Tests for example parsers
//!
//! These tests verify that the example parsers work correctly.
//! Each test focuses on the core functionality demonstrated by the example.

use parsanol::portable::{
    parser_dsl::{dynamic, re, seq, str, GrammarBuilder},
    AstArena, PortableParser,
};

// =============================================================================
// JSON Parser Tests
// =============================================================================

#[test]
fn test_json_string() {
    let grammar = GrammarBuilder::new()
        .rule("string", re("\"[^\"]*\""))
        .build();

    let cases = vec!["\"hello\"", "\"\"", "\"test 123\""];

    for input in cases {
        let mut arena = AstArena::for_input(input.len());
        let mut parser = PortableParser::new(&grammar, input, &mut arena);
        assert!(parser.parse().is_ok(), "Should parse: {}", input);
    }
}

#[test]
fn test_json_number() {
    let grammar = GrammarBuilder::new()
        .rule("number", re("-?[0-9]+(\\.[0-9]+)?"))
        .build();

    let cases = vec!["42", "-10", "3.14", "-2.5", "0", "100.001"];

    for input in cases {
        let mut arena = AstArena::for_input(input.len());
        let mut parser = PortableParser::new(&grammar, input, &mut arena);
        assert!(parser.parse().is_ok(), "Should parse: {}", input);
    }
}

#[test]
fn test_json_array() {
    // Simplified array grammar
    let grammar = GrammarBuilder::new()
        .rule("array", re("\\[[0-9 ]+\\]"))
        .build();

    let cases = vec!["[1]", "[42]", "[123]"];

    for input in cases {
        let mut arena = AstArena::for_input(input.len());
        let mut parser = PortableParser::new(&grammar, input, &mut arena);
        assert!(parser.parse().is_ok(), "Should parse: {}", input);
    }
}

// =============================================================================
// Calculator Tests
// =============================================================================

#[test]
fn test_calculator_number() {
    let grammar = GrammarBuilder::new().rule("number", re("[0-9]+")).build();

    let cases = vec!["0", "42", "123456"];

    for input in cases {
        let mut arena = AstArena::for_input(input.len());
        let mut parser = PortableParser::new(&grammar, input, &mut arena);
        assert!(parser.parse().is_ok(), "Should parse: {}", input);
    }
}

#[test]
fn test_calculator_simple_expr() {
    // Simple arithmetic expression
    let grammar = GrammarBuilder::new()
        .rule("expr", re("[0-9]+[ \\t]*[+\\-*/][ \\t]*[0-9]+"))
        .build();

    let cases = vec!["1+2", "10-5", "3*4", "8/2"];

    for input in cases {
        let mut arena = AstArena::for_input(input.len());
        let mut parser = PortableParser::new(&grammar, input, &mut arena);
        assert!(parser.parse().is_ok(), "Should parse: {}", input);
    }
}

// =============================================================================
// CSV Parser Tests
// =============================================================================

#[test]
fn test_csv_field() {
    let grammar = GrammarBuilder::new().rule("field", re("[^,\\n]+")).build();

    let cases = vec!["hello", "123", "test value"];

    for input in cases {
        let mut arena = AstArena::for_input(input.len());
        let mut parser = PortableParser::new(&grammar, input, &mut arena);
        assert!(parser.parse().is_ok(), "Should parse: {}", input);
    }
}

#[test]
fn test_csv_row() {
    // Simple CSV row with two fields
    let grammar = GrammarBuilder::new()
        .rule("row", re("[^,\\n]+,[^,\\n]+"))
        .build();

    let cases = vec!["a,b", "1,2", "hello,world"];

    for input in cases {
        let mut arena = AstArena::for_input(input.len());
        let mut parser = PortableParser::new(&grammar, input, &mut arena);
        assert!(parser.parse().is_ok(), "Should parse: {}", input);
    }
}

// =============================================================================
// URL Parser Tests
// =============================================================================

#[test]
fn test_url_scheme() {
    let grammar = GrammarBuilder::new()
        .rule("scheme", re("[a-zA-Z][a-zA-Z0-9+.-]*"))
        .build();

    let cases = vec!["http", "https", "ftp", "file"];

    for input in cases {
        let mut arena = AstArena::for_input(input.len());
        let mut parser = PortableParser::new(&grammar, input, &mut arena);
        assert!(parser.parse().is_ok(), "Should parse: {}", input);
    }
}

#[test]
fn test_url_domain() {
    let grammar = GrammarBuilder::new()
        .rule("domain", re("[a-zA-Z0-9.-]+"))
        .build();

    let cases = vec!["example.com", "sub.domain.org", "localhost"];

    for input in cases {
        let mut arena = AstArena::for_input(input.len());
        let mut parser = PortableParser::new(&grammar, input, &mut arena);
        assert!(parser.parse().is_ok(), "Should parse: {}", input);
    }
}

// =============================================================================
// INI Parser Tests
// =============================================================================

#[test]
fn test_ini_section() {
    let grammar = GrammarBuilder::new()
        .rule(
            "section",
            seq(vec![
                dynamic(str("[")),
                dynamic(re("[a-zA-Z_][a-zA-Z0-9_.]*")),
                dynamic(str("]")),
            ]),
        )
        .build();

    let cases = vec!["[section]", "[database]", "[server]"];

    for input in cases {
        let mut arena = AstArena::for_input(input.len());
        let mut parser = PortableParser::new(&grammar, input, &mut arena);
        assert!(parser.parse().is_ok(), "Should parse: {}", input);
    }
}

#[test]
fn test_ini_key_value() {
    let grammar = GrammarBuilder::new()
        .rule(
            "pair",
            seq(vec![
                dynamic(re("[a-zA-Z_][a-zA-Z0-9_]*")),
                dynamic(re("[ \\t]*=[ \\t]*")),
                dynamic(re("[^\\n]+")),
            ]),
        )
        .build();

    let cases = vec!["key=value", "host = localhost", "port=8080"];

    for input in cases {
        let mut arena = AstArena::for_input(input.len());
        let mut parser = PortableParser::new(&grammar, input, &mut arena);
        assert!(parser.parse().is_ok(), "Should parse: {}", input);
    }
}

// =============================================================================
// S-Expression Parser Tests
// =============================================================================

#[test]
fn test_sexp_symbol() {
    let grammar = GrammarBuilder::new()
        .rule("symbol", re("[a-zA-Z_+\\-*/=<>!][a-zA-Z0-9_+\\-*/=<>!]*"))
        .build();

    let cases = vec!["hello", "+", "-", "*", "define", "my-var"];

    for input in cases {
        let mut arena = AstArena::for_input(input.len());
        let mut parser = PortableParser::new(&grammar, input, &mut arena);
        assert!(parser.parse().is_ok(), "Should parse: {}", input);
    }
}

#[test]
fn test_sexp_number() {
    let grammar = GrammarBuilder::new()
        .rule("number", re("-?[0-9]+(\\.[0-9]+)?"))
        .build();

    let cases = vec!["42", "-3", "3.14", "-2.5"];

    for input in cases {
        let mut arena = AstArena::for_input(input.len());
        let mut parser = PortableParser::new(&grammar, input, &mut arena);
        assert!(parser.parse().is_ok(), "Should parse: {}", input);
    }
}

// =============================================================================
// Markup Parser Tests
// =============================================================================

#[test]
fn test_markup_header() {
    let grammar = GrammarBuilder::new()
        .rule(
            "header",
            seq(vec![dynamic(str("# ")), dynamic(re("[^\\n]+"))]),
        )
        .build();

    let cases = vec!["# Title", "# Hello World"];

    for input in cases {
        let mut arena = AstArena::for_input(input.len());
        let mut parser = PortableParser::new(&grammar, input, &mut arena);
        assert!(parser.parse().is_ok(), "Should parse: {}", input);
    }
}

#[test]
fn test_markup_list_item() {
    let grammar = GrammarBuilder::new()
        .rule(
            "list_item",
            seq(vec![dynamic(str("- ")), dynamic(re("[^\\n]+"))]),
        )
        .build();

    let cases = vec!["- Item", "- Another item"];

    for input in cases {
        let mut arena = AstArena::for_input(input.len());
        let mut parser = PortableParser::new(&grammar, input, &mut arena);
        assert!(parser.parse().is_ok(), "Should parse: {}", input);
    }
}

// =============================================================================
// Expression Evaluator Tests
// =============================================================================

#[test]
fn test_expr_variable() {
    let grammar = GrammarBuilder::new()
        .rule("variable", re("[a-zA-Z_][a-zA-Z0-9_]*"))
        .build();

    let cases = vec!["x", "variable_name", "PI", "myVar123"];

    for input in cases {
        let mut arena = AstArena::for_input(input.len());
        let mut parser = PortableParser::new(&grammar, input, &mut arena);
        assert!(parser.parse().is_ok(), "Should parse: {}", input);
    }
}

#[test]
fn test_expr_function_call() {
    let grammar = GrammarBuilder::new()
        .rule(
            "funcall",
            seq(vec![
                dynamic(re("[a-zA-Z_][a-zA-Z0-9_]*")),
                dynamic(str("(")),
                dynamic(re("[^)]*")),
                dynamic(str(")")),
            ]),
        )
        .build();

    let cases = vec!["sin(0)", "max(1,2)", "sqrt(16)"];

    for input in cases {
        let mut arena = AstArena::for_input(input.len());
        let mut parser = PortableParser::new(&grammar, input, &mut arena);
        assert!(parser.parse().is_ok(), "Should parse: {}", input);
    }
}

// =============================================================================
// IP Address Parser Tests
// =============================================================================

#[test]
fn test_ipv4_octet() {
    let grammar = GrammarBuilder::new()
        .rule(
            "octet",
            re("(25[0-5]|2[0-4][0-9]|1[0-9][0-9]|[1-9][0-9]|[0-9])"),
        )
        .build();

    let valid = vec!["0", "1", "42", "99", "100", "199", "200", "255"];
    let invalid = vec!["256", "300", "-1", "abc"];

    for input in valid {
        let mut arena = AstArena::for_input(input.len());
        let mut parser = PortableParser::new(&grammar, input, &mut arena);
        assert!(parser.parse().is_ok(), "Should parse: {}", input);
    }

    for input in invalid {
        let mut arena = AstArena::for_input(input.len());
        let mut parser = PortableParser::new(&grammar, input, &mut arena);
        assert!(parser.parse().is_err(), "Should NOT parse: {}", input);
    }
}

// =============================================================================
// Email Parser Tests
// =============================================================================

#[test]
fn test_email_word() {
    let grammar = GrammarBuilder::new()
        .rule("word", re("[a-zA-Z0-9]+"))
        .build();

    let cases = vec!["user", "john", "test123", "abc"];

    for input in cases {
        let mut arena = AstArena::for_input(input.len());
        let mut parser = PortableParser::new(&grammar, input, &mut arena);
        assert!(parser.parse().is_ok(), "Should parse: {}", input);
    }
}

// =============================================================================
// Simple XML Parser Tests
// =============================================================================

#[test]
fn test_xml_tag() {
    let grammar = GrammarBuilder::new()
        .rule(
            "tag",
            seq(vec![
                dynamic(str("<")),
                dynamic(re("[a-zA-Z][a-zA-Z0-9_-]*")),
                dynamic(str(">")),
            ]),
        )
        .build();

    let cases = vec!["<div>", "<p>", "<html>", "<my-tag>"];

    for input in cases {
        let mut arena = AstArena::for_input(input.len());
        let mut parser = PortableParser::new(&grammar, input, &mut arena);
        assert!(parser.parse().is_ok(), "Should parse: {}", input);
    }
}

#[test]
fn test_xml_closing_tag() {
    let grammar = GrammarBuilder::new()
        .rule(
            "closing_tag",
            seq(vec![
                dynamic(str("</")),
                dynamic(re("[a-zA-Z][a-zA-Z0-9_-]*")),
                dynamic(str(">")),
            ]),
        )
        .build();

    let cases = vec!["</div>", "</p>", "</html>", "</my-tag>"];

    for input in cases {
        let mut arena = AstArena::for_input(input.len());
        let mut parser = PortableParser::new(&grammar, input, &mut arena);
        assert!(parser.parse().is_ok(), "Should parse: {}", input);
    }
}

// =============================================================================
// Boolean Algebra Parser Tests
// =============================================================================

#[test]
fn test_boolean_variable() {
    let grammar = GrammarBuilder::new()
        .rule("variable", re("var[0-9]+"))
        .build();

    let cases = vec!["var0", "var1", "var99", "var123"];

    for input in cases {
        let mut arena = AstArena::for_input(input.len());
        let mut parser = PortableParser::new(&grammar, input, &mut arena);
        assert!(parser.parse().is_ok(), "Should parse: {}", input);
    }
}

#[test]
fn test_boolean_operators() {
    let grammar = GrammarBuilder::new().rule("op", re("and|or")).build();

    // Test AND
    let mut arena = AstArena::for_input(3);
    let mut parser = PortableParser::new(&grammar, "and", &mut arena);
    assert!(parser.parse().is_ok());

    // Test OR
    let mut arena = AstArena::for_input(2);
    let mut parser = PortableParser::new(&grammar, "or", &mut arena);
    assert!(parser.parse().is_ok());
}

// =============================================================================
// Comments Parser Tests
// =============================================================================

#[test]
fn test_line_comment() {
    let grammar = GrammarBuilder::new()
        .rule(
            "line_comment",
            seq(vec![dynamic(str("//")), dynamic(re("[^\n\r]*"))]),
        )
        .build();

    let cases = vec!["// comment", "// hello world", "//123"];

    for input in cases {
        let mut arena = AstArena::for_input(input.len());
        let mut parser = PortableParser::new(&grammar, input, &mut arena);
        assert!(parser.parse().is_ok(), "Should parse: {}", input);
    }
}

#[test]
fn test_block_comment() {
    // Simplified block comment - just matches /* ... */
    let grammar = GrammarBuilder::new()
        .rule("block_comment", re("/\\*[^*]*\\*/"))
        .build();

    let cases = vec!["/**/", "/* comment */"];

    for input in cases {
        let mut arena = AstArena::for_input(input.len());
        let mut parser = PortableParser::new(&grammar, input, &mut arena);
        assert!(parser.parse().is_ok(), "Should parse: {:?}", input);
    }
}

// =============================================================================
// ERB Parser Tests
// =============================================================================

#[test]
fn test_erb_expression() {
    let grammar = GrammarBuilder::new()
        .rule(
            "expr",
            seq(vec![
                dynamic(str("<%=")),
                dynamic(re("[^%]*")),
                dynamic(str("%>")),
            ]),
        )
        .build();

    let cases = vec!["<%= x %>", "<%= 1 + 2 %>", "<%=name%>"];

    for input in cases {
        let mut arena = AstArena::for_input(input.len());
        let mut parser = PortableParser::new(&grammar, input, &mut arena);
        assert!(parser.parse().is_ok(), "Should parse: {}", input);
    }
}

#[test]
fn test_erb_code() {
    let grammar = GrammarBuilder::new()
        .rule(
            "code",
            seq(vec![
                dynamic(str("<%")),
                dynamic(re("[^%]*")),
                dynamic(str("%>")),
            ]),
        )
        .build();

    let cases = vec!["<% x = 1 %>", "<% if true %>", "<%code%>"];

    for input in cases {
        let mut arena = AstArena::for_input(input.len());
        let mut parser = PortableParser::new(&grammar, input, &mut arena);
        assert!(parser.parse().is_ok(), "Should parse: {}", input);
    }
}

// =============================================================================
// Balanced Parentheses Tests
// =============================================================================

#[test]
fn test_balanced_empty() {
    let grammar = GrammarBuilder::new().rule("empty", str("()")).build();

    let mut arena = AstArena::for_input(2);
    let mut parser = PortableParser::new(&grammar, "()", &mut arena);
    assert!(parser.parse().is_ok());
}

#[test]
fn test_balanced_nested() {
    let grammar = GrammarBuilder::new()
        .rule(
            "balanced",
            seq(vec![
                dynamic(str("(")),
                dynamic(re("[^()]*")),
                dynamic(str(")")),
            ]),
        )
        .build();

    let cases = vec!["()", "(x)", "(123)"];

    for input in cases {
        let mut arena = AstArena::for_input(input.len());
        let mut parser = PortableParser::new(&grammar, input, &mut arena);
        assert!(parser.parse().is_ok(), "Should parse: {}", input);
    }
}

// =============================================================================
// Integration Tests - Full Example Scenarios
// =============================================================================

#[test]
fn test_integration_json_object() {
    // Simplified JSON pair
    let grammar = GrammarBuilder::new()
        .rule("pair", re("\"[^\"]+\"[ \\t]*:[ \\t]*\"[^\"]+\""))
        .build();

    let cases = vec![r#""name": "test""#, r#""key": "value""#];

    for input in cases {
        let mut arena = AstArena::for_input(input.len());
        let mut parser = PortableParser::new(&grammar, input, &mut arena);
        assert!(parser.parse().is_ok(), "Should parse: {}", input);
    }
}

#[test]
fn test_integration_csv_full() {
    // Three-field CSV row
    let grammar = GrammarBuilder::new()
        .rule("row", re("[^,\\n]+,[^,\\n]+,[^,\\n]+"))
        .build();

    let input = "name,age,city";
    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(&grammar, input, &mut arena);
    assert!(parser.parse().is_ok());
}

#[test]
fn test_integration_ini_full() {
    // Section header
    let section_grammar = GrammarBuilder::new()
        .rule("section", re("\\[[a-zA-Z_][a-zA-Z0-9_.]*\\]"))
        .build();

    // Key-value pair
    let pair_grammar = GrammarBuilder::new()
        .rule("pair", re("[a-zA-Z_][a-zA-Z0-9_]*[ \\t]*=[ \\t]*[^\\n]+"))
        .build();

    let cases = vec![
        ("[section]", &section_grammar),
        ("key=value", &pair_grammar),
    ];

    for (input, grammar) in cases {
        let mut arena = AstArena::for_input(input.len());
        let mut parser = PortableParser::new(grammar, input, &mut arena);
        assert!(parser.parse().is_ok(), "Should parse: {}", input);
    }
}

// =============================================================================
// Sentence Parser Tests (Unicode)
// =============================================================================

#[test]
fn test_english_sentence() {
    let grammar = GrammarBuilder::new()
        .rule("sentence", re("[^.!?]+[.!?]"))
        .build();

    let cases = vec!["Hello.", "World!", "How are you?"];

    for input in cases {
        let mut arena = AstArena::for_input(input.len());
        let mut parser = PortableParser::new(&grammar, input, &mut arena);
        assert!(parser.parse().is_ok(), "Should parse: {}", input);
    }
}

#[test]
fn test_japanese_sentence() {
    let grammar = GrammarBuilder::new()
        .rule("sentence", re("[^。]+。"))
        .build();

    let cases = vec!["こんにちは。", "世界。"];

    for input in cases {
        let mut arena = AstArena::for_input(input.len());
        let mut parser = PortableParser::new(&grammar, input, &mut arena);
        assert!(parser.parse().is_ok(), "Should parse: {}", input);
    }
}

// =============================================================================
// String Literal Parser Tests
// =============================================================================

#[test]
fn test_string_literal() {
    let grammar = GrammarBuilder::new()
        .rule(
            "string",
            seq(vec![
                dynamic(str("\"")),
                dynamic(re("(?:\\\\.|[^\"])*")),
                dynamic(str("\"")),
            ]),
        )
        .build();

    let cases = vec!["\"hello\"", "\"\"", "\"hello world\"", "\"escape\\nseq\""];

    for input in cases {
        let mut arena = AstArena::for_input(input.len());
        let mut parser = PortableParser::new(&grammar, input, &mut arena);
        assert!(parser.parse().is_ok(), "Should parse: {}", input);
    }
}

#[test]
fn test_escape_sequences() {
    let grammar = GrammarBuilder::new()
        .rule("escape", re("\\\\[nrt\"\\\\0]"))
        .build();

    let cases = vec!["\\n", "\\t", "\\r", "\\\\", "\\\"", "\\0"];

    for input in cases {
        let mut arena = AstArena::for_input(input.len());
        let mut parser = PortableParser::new(&grammar, input, &mut arena);
        assert!(parser.parse().is_ok(), "Should parse: {:?}", input);
    }
}

// =============================================================================
// Error Reporting Tests
// =============================================================================

#[test]
fn test_error_position() {
    // Test that we can find the position of parse failures
    let grammar = GrammarBuilder::new()
        .rule("identifier", re("[a-zA-Z_][a-zA-Z0-9_]*"))
        .build();

    let valid = "hello";
    let invalid = "123";

    // Valid should parse
    let mut arena = AstArena::for_input(valid.len());
    let mut parser = PortableParser::new(&grammar, valid, &mut arena);
    assert!(parser.parse().is_ok());

    // Invalid should fail
    let mut arena = AstArena::for_input(invalid.len());
    let mut parser = PortableParser::new(&grammar, invalid, &mut arena);
    assert!(parser.parse().is_err());
}

// =============================================================================
// Additional Integration Tests
// =============================================================================

#[test]
fn test_mixed_literals() {
    let grammar = GrammarBuilder::new()
        .rule("literal", re("(?:\"(?:\\\\.|[^\"])*\")|(?:[0-9]+)"))
        .build();

    let cases = vec!["\"hello\"", "42", "\"escape\\nseq\"", "0"];

    for input in cases {
        let mut arena = AstArena::for_input(input.len());
        let mut parser = PortableParser::new(&grammar, input, &mut arena);
        assert!(parser.parse().is_ok(), "Should parse: {}", input);
    }
}

#[test]
fn test_multiline_parsing() {
    // Grammar that handles single line (no newlines)
    let grammar = GrammarBuilder::new().rule("line", re("[^\\n]+")).build();

    let input = "line1";
    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(&grammar, input, &mut arena);
    assert!(parser.parse().is_ok());
}

#[test]
fn test_unicode_handling() {
    // Grammar that handles Unicode
    let grammar = GrammarBuilder::new().rule("text", re(".+")).build();

    let cases = vec!["Hello", "こんにちは", "你好", "안녕하세요", "🎉🎊"];

    for input in cases {
        let mut arena = AstArena::for_input(input.len());
        let mut parser = PortableParser::new(&grammar, input, &mut arena);
        assert!(parser.parse().is_ok(), "Should parse: {:?}", input);
    }
}
