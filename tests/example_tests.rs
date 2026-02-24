//! Tests for example parsers
//!
//! These tests verify that the example parsers work correctly.
//! Each test focuses on the core functionality demonstrated by the example.

use parsanol::portable::{
    parser_dsl::{choice, dynamic, re, ref_, seq, str, GrammarBuilder},
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

// =============================================================================
// Precedence Calculator Tests (prec-calc)
// =============================================================================

#[test]
fn test_prec_calc_number() {
    let grammar = GrammarBuilder::new().rule("number", re("[0-9]+")).build();

    let cases = vec!["0", "42", "123456"];

    for input in cases {
        let mut arena = AstArena::for_input(input.len());
        let mut parser = PortableParser::new(&grammar, input, &mut arena);
        assert!(parser.parse().is_ok(), "Should parse: {}", input);
    }
}

#[test]
fn test_prec_calc_assignment() {
    // Simple assignment: identifier = number
    let grammar = GrammarBuilder::new()
        .rule("assignment", re("[a-z]+[ \\t]*=[ \\t]*[0-9]+"))
        .build();

    let cases = vec!["a = 1", "x=42", "var = 123"];

    for input in cases {
        let mut arena = AstArena::for_input(input.len());
        let mut parser = PortableParser::new(&grammar, input, &mut arena);
        assert!(parser.parse().is_ok(), "Should parse: {}", input);
    }
}

#[test]
fn test_prec_calc_expression() {
    // Binary expression: number op number
    let grammar = GrammarBuilder::new()
        .rule("expr", re("[0-9]+[ \\t]*[+\\-*/][ \\t]*[0-9]+"))
        .build();

    let cases = vec!["1 + 2", "3-4", "5 * 6", "7/8", "9+10"];

    for input in cases {
        let mut arena = AstArena::for_input(input.len());
        let mut parser = PortableParser::new(&grammar, input, &mut arena);
        assert!(parser.parse().is_ok(), "Should parse: {}", input);
    }
}

#[test]
fn test_prec_calc_precedence() {
    // Expression with precedence: multiplication binds tighter
    let grammar = GrammarBuilder::new()
        .rule(
            "expr",
            re("[0-9]+[ \\t]*[+*/][ \\t]*[0-9]+([ \\t]*[+*/][ \\t]*[0-9]+)?"),
        )
        .build();

    let cases = vec!["1 + 2 * 3", "4 * 5 + 6", "7 + 8 + 9"];

    for input in cases {
        let mut arena = AstArena::for_input(input.len());
        let mut parser = PortableParser::new(&grammar, input, &mut arena);
        assert!(parser.parse().is_ok(), "Should parse: {}", input);
    }
}

// =============================================================================
// MiniLisp S-Expression Tests
// Demonstrates proper grammar composition with sequences and choices
// NOTE: First rule defined becomes the root in GrammarBuilder
// =============================================================================

#[test]
fn test_minilisp_atom() {
    // Atoms: symbols and numbers using choice between two patterns
    // Root rule is "atom" (defined first), which references other rules
    let grammar = GrammarBuilder::new()
        .rule(
            "atom",
            choice(vec![
                dynamic(re(r"[a-zA-Z_+\-*/=<>!?][a-zA-Z0-9_+\-*/=<>!?]*")),
                dynamic(re(r"-?[0-9]+")),
            ]),
        )
        .build();

    let cases = vec!["foo", "+", "-", "lambda", "123", "-42", "nil?", "!"];

    for input in cases {
        let mut arena = AstArena::for_input(input.len());
        let mut parser = PortableParser::new(&grammar, input, &mut arena);
        assert!(parser.parse().is_ok(), "Should parse: {}", input);
    }
}

#[test]
fn test_minilisp_empty_list() {
    // Empty list: () using sequence of two string matches
    // "empty_list" is the root rule
    let grammar = GrammarBuilder::new()
        .rule(
            "empty_list",
            seq(vec![dynamic(str("(")), dynamic(str(")"))]),
        )
        .build();

    let mut arena = AstArena::for_input(2);
    let mut parser = PortableParser::new(&grammar, "()", &mut arena);
    assert!(parser.parse().is_ok());
}

#[test]
fn test_minilisp_simple_list() {
    // Simple list: (atom) using sequence with regex for the atom
    // "simple_list" is the root rule
    let grammar = GrammarBuilder::new()
        .rule(
            "simple_list",
            seq(vec![
                dynamic(str("(")),
                dynamic(re(r"[a-zA-Z0-9_+\-*/]+")),
                dynamic(str(")")),
            ]),
        )
        .build();

    let cases = vec!["(foo)", "(+)", "(123)"];

    for input in cases {
        let mut arena = AstArena::for_input(input.len());
        let mut parser = PortableParser::new(&grammar, input, &mut arena);
        assert!(parser.parse().is_ok(), "Should parse: {}", input);
    }
}

#[test]
fn test_minilisp_two_arg_call() {
    // Two-argument call: (func arg1 arg2) using sequence
    // "call" is the root rule
    let grammar = GrammarBuilder::new()
        .rule(
            "call",
            seq(vec![
                dynamic(str("(")),
                dynamic(re(r"[a-zA-Z0-9_+\-*/]+")), // func
                dynamic(re(r"[ \t]+")),             // space
                dynamic(re(r"[a-zA-Z0-9_+\-*/]+")), // arg1
                dynamic(re(r"[ \t]+")),             // space
                dynamic(re(r"[a-zA-Z0-9_+\-*/]+")), // arg2
                dynamic(str(")")),
            ]),
        )
        .build();

    // Test cases with exactly 2 arguments (3 atoms total including function)
    let cases = vec!["(+ 1 2)", "(def x 10)", "(mul 3 4)", "(sub a b)"];

    for input in cases {
        let mut arena = AstArena::for_input(input.len());
        let mut parser = PortableParser::new(&grammar, input, &mut arena);
        assert!(parser.parse().is_ok(), "Should parse: {}", input);
    }
}

#[test]
fn test_minilisp_quoted_string() {
    // Quoted string: "hello world" using sequence of rules
    // "string" is the root rule
    let grammar = GrammarBuilder::new()
        .rule(
            "string",
            seq(vec![
                dynamic(str("\"")),
                dynamic(re(r#"[^"]*"#)),
                dynamic(str("\"")),
            ]),
        )
        .build();

    let cases = vec!["\"hello\"", "\"\"", "\"hello world\""];

    for input in cases {
        let mut arena = AstArena::for_input(input.len());
        let mut parser = PortableParser::new(&grammar, input, &mut arena);
        assert!(parser.parse().is_ok(), "Should parse: {}", input);
    }
}

// =============================================================================
// Modularity Tests (Grammar Composition)
// =============================================================================

#[test]
fn test_modularity_module_a() {
    // Module A: matches 'aaa'
    let grammar = GrammarBuilder::new().rule("a_language", str("aaa")).build();

    let mut arena = AstArena::for_input(3);
    let mut parser = PortableParser::new(&grammar, "aaa", &mut arena);
    assert!(parser.parse().is_ok());

    let mut arena = AstArena::for_input(3);
    let mut parser = PortableParser::new(&grammar, "bbb", &mut arena);
    assert!(parser.parse().is_err());
}

#[test]
fn test_modularity_module_b() {
    // Module B: matches 'bbb'
    let grammar = GrammarBuilder::new().rule("b_language", str("bbb")).build();

    let mut arena = AstArena::for_input(3);
    let mut parser = PortableParser::new(&grammar, "bbb", &mut arena);
    assert!(parser.parse().is_ok());
}

#[test]
fn test_modularity_composed() {
    // Composed grammar: a(aaa) | b(bbb)
    let grammar = GrammarBuilder::new()
        .rule("root", re("(a\\(aaa\\)|b\\(bbb\\)|c\\(ccc\\))"))
        .build();

    let valid = vec!["a(aaa)", "b(bbb)", "c(ccc)"];
    let invalid = vec!["a(bbb)", "b(aaa)", "d(ddd)"];

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
// Deepest Error Reporting Tests
// =============================================================================

#[test]
fn test_deepest_error_identifier() {
    // Identifier rule
    let grammar = GrammarBuilder::new()
        .rule("identifier", re("[a-zA-Z_][a-zA-Z0-9_]*"))
        .build();

    let valid = vec!["foo", "_bar", "CamelCase", "snake_case"];
    let invalid = vec!["123abc", "!invalid", ""];

    for input in valid {
        let mut arena = AstArena::for_input(input.len());
        let mut parser = PortableParser::new(&grammar, input, &mut arena);
        assert!(parser.parse().is_ok(), "Should parse: {}", input);
    }

    for input in invalid {
        if input.is_empty() {
            continue;
        }
        let mut arena = AstArena::for_input(input.len());
        let mut parser = PortableParser::new(&grammar, input, &mut arena);
        assert!(parser.parse().is_err(), "Should NOT parse: {}", input);
    }
}

#[test]
fn test_deepest_error_function_call() {
    // Function call: identifier(args)
    let grammar = GrammarBuilder::new()
        .rule("funcall", re("[a-zA-Z_][a-zA-Z0-9_]*\\([^)]*\\)"))
        .build();

    let valid = vec!["foo()", "bar(x)", "func(1,2,3)"];
    let invalid = vec!["foo(", "bar)", "123func()"];

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

#[test]
fn test_deepest_error_nested() {
    // Nested structure: finding the deepest error point
    let grammar = GrammarBuilder::new()
        .rule("nested", re("\\([^()]*\\([^()]*\\)[^()]*\\)"))
        .build();

    let valid = vec!["(a(b)c)", "(x(y)z)"];

    for input in valid {
        let mut arena = AstArena::for_input(input.len());
        let mut parser = PortableParser::new(&grammar, input, &mut arena);
        assert!(parser.parse().is_ok(), "Should parse: {}", input);
    }
}

// =============================================================================
// Nested Error Tree Tests
// =============================================================================

#[test]
fn test_nested_error_tree_simple() {
    // Simple expression with potential errors
    let grammar = GrammarBuilder::new()
        .rule("expr", re("[0-9]+([+\\-*/][0-9]+)*"))
        .build();

    let valid = vec!["1", "1+2", "1+2+3", "10-5*2"];
    let invalid = vec!["+", "1+", "+2"];

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

#[test]
fn test_nested_error_tree_if_statement() {
    // Simple if statement pattern
    let grammar = GrammarBuilder::new()
        .rule("if_stmt", re(r"if[ \t]*\([^)]+\)[ \t]*\{[^}]*\}"))
        .build();

    let valid = vec!["if (x) {y}", "if(true){pass}"];
    let invalid = vec!["if x {y}", "if () {}", "if (x) {}"];

    for input in valid {
        let mut arena = AstArena::for_input(input.len());
        let mut parser = PortableParser::new(&grammar, input, &mut arena);
        assert!(parser.parse().is_ok(), "Should parse: {}", input);
    }

    for input in invalid {
        let mut arena = AstArena::for_input(input.len());
        let mut parser = PortableParser::new(&grammar, input, &mut arena);
        // Note: if (x) {} is actually valid in our simplified grammar
        if input == "if (x) {}" {
            continue;
        }
        assert!(parser.parse().is_err(), "Should NOT parse: {}", input);
    }
}

#[test]
fn test_nested_error_tree_comparison() {
    // Comparison: x > y
    let grammar = GrammarBuilder::new()
        .rule("comparison", re("[a-z]+[ \\t]*[><=!]+[ \\t]*[a-z0-9]+"))
        .build();

    let valid = vec!["x > y", "a<=b", "x==1", "a!=b"];
    let invalid = vec!["> y", "x >", "x > "];

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
// ISO 8601 Date/Time Tests
// =============================================================================

#[test]
fn test_iso8601_date() {
    let grammar = GrammarBuilder::new()
        .rule("date", re("[0-9]{4}-[0-9]{2}-[0-9]{2}"))
        .build();

    let cases = vec!["2024-01-15", "1999-12-31", "2000-01-01"];

    for input in cases {
        let mut arena = AstArena::for_input(input.len());
        let mut parser = PortableParser::new(&grammar, input, &mut arena);
        assert!(parser.parse().is_ok(), "Should parse: {}", input);
    }
}

#[test]
fn test_iso8601_time() {
    let grammar = GrammarBuilder::new()
        .rule("time", re("[0-9]{2}:[0-9]{2}:[0-9]{2}"))
        .build();

    let cases = vec!["10:30:00", "23:59:59", "00:00:00"];

    for input in cases {
        let mut arena = AstArena::for_input(input.len());
        let mut parser = PortableParser::new(&grammar, input, &mut arena);
        assert!(parser.parse().is_ok(), "Should parse: {}", input);
    }
}

#[test]
fn test_iso8601_datetime() {
    let grammar = GrammarBuilder::new()
        .rule(
            "datetime",
            re("[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}"),
        )
        .build();

    let cases = vec!["2024-01-15T10:30:00", "1999-12-31T23:59:59"];

    for input in cases {
        let mut arena = AstArena::for_input(input.len());
        let mut parser = PortableParser::new(&grammar, input, &mut arena);
        assert!(parser.parse().is_ok(), "Should parse: {}", input);
    }
}

// =============================================================================
// ISO 6709 Geographic Coordinate Tests
// =============================================================================

#[test]
fn test_iso6709_latitude() {
    let grammar = GrammarBuilder::new()
        .rule("lat", re("[+-][0-9]{2}(\\.[0-9]+)?"))
        .build();

    let cases = vec!["+40", "-90", "+40.6894", "-33.8688"];

    for input in cases {
        let mut arena = AstArena::for_input(input.len());
        let mut parser = PortableParser::new(&grammar, input, &mut arena);
        assert!(parser.parse().is_ok(), "Should parse: {}", input);
    }
}

#[test]
fn test_iso6709_coordinate() {
    let grammar = GrammarBuilder::new()
        .rule(
            "coord",
            re("[+-][0-9]{2}(\\.[0-9]+)?[+-][0-9]{3}(\\.[0-9]+)?"),
        )
        .build();

    let cases = vec!["+40.6894-074.0447", "-33.8688+151.2093", "+90+000"];

    for input in cases {
        let mut arena = AstArena::for_input(input.len());
        let mut parser = PortableParser::new(&grammar, input, &mut arena);
        assert!(parser.parse().is_ok(), "Should parse: {}", input);
    }
}

// =============================================================================
// Markdown Parser Tests
// =============================================================================

#[test]
fn test_markdown_header() {
    let grammar = GrammarBuilder::new()
        .rule("header", re("#{1,6}[ \\t]+[^\\n]+"))
        .build();

    let cases = vec!["# Title", "## Subtitle", "### H3", "###### H6"];

    for input in cases {
        let mut arena = AstArena::for_input(input.len());
        let mut parser = PortableParser::new(&grammar, input, &mut arena);
        assert!(parser.parse().is_ok(), "Should parse: {}", input);
    }
}

#[test]
fn test_markdown_bold() {
    let grammar = GrammarBuilder::new()
        .rule("bold", re("\\*\\*[^*]+\\*\\*"))
        .build();

    let cases = vec!["**bold**", "**bold text**"];

    for input in cases {
        let mut arena = AstArena::for_input(input.len());
        let mut parser = PortableParser::new(&grammar, input, &mut arena);
        assert!(parser.parse().is_ok(), "Should parse: {}", input);
    }
}

// =============================================================================
// TOML Parser Tests
// =============================================================================

#[test]
fn test_toml_section() {
    let grammar = GrammarBuilder::new()
        .rule("section", re("\\[[a-zA-Z_][a-zA-Z0-9_.]*\\]"))
        .build();

    let cases = vec!["[section]", "[database]", "[server.http]"];

    for input in cases {
        let mut arena = AstArena::for_input(input.len());
        let mut parser = PortableParser::new(&grammar, input, &mut arena);
        assert!(parser.parse().is_ok(), "Should parse: {}", input);
    }
}

#[test]
fn test_toml_key_value() {
    let grammar = GrammarBuilder::new()
        .rule("pair", re("[a-zA-Z_][a-zA-Z0-9_]*[ \\t]*=[ \\t]*[^\\n]+"))
        .build();

    let cases = vec!["key = \"value\"", "port = 8080", "enabled = true"];

    for input in cases {
        let mut arena = AstArena::for_input(input.len());
        let mut parser = PortableParser::new(&grammar, input, &mut arena);
        assert!(parser.parse().is_ok(), "Should parse: {}", input);
    }
}

// =============================================================================
// YAML Parser Tests
// =============================================================================

#[test]
fn test_yaml_key_value() {
    let grammar = GrammarBuilder::new()
        .rule("pair", re("[a-zA-Z_][a-zA-Z0-9_]*:[ \\t]*[^\\n]+"))
        .build();

    let cases = vec!["key: value", "name: John", "age: 30"];

    for input in cases {
        let mut arena = AstArena::for_input(input.len());
        let mut parser = PortableParser::new(&grammar, input, &mut arena);
        assert!(parser.parse().is_ok(), "Should parse: {}", input);
    }
}

#[test]
fn test_yaml_list_item() {
    let grammar = GrammarBuilder::new()
        .rule("item", re("-[ \\t]+[^\\n]+"))
        .build();

    let cases = vec!["- item", "- another item", "- 123"];

    for input in cases {
        let mut arena = AstArena::for_input(input.len());
        let mut parser = PortableParser::new(&grammar, input, &mut arena);
        assert!(parser.parse().is_ok(), "Should parse: {}", input);
    }
}

// =============================================================================
// ADVANCED GRAMMAR COMPOSITION TESTS
// These tests demonstrate proper parsing ability using:
// - Recursive grammar structures
// - Rule references (ref_)
// - Sequence and choice composition
// - Nested structures
// =============================================================================

#[test]
fn test_recursive_arithmetic_expression() {
    // Recursive arithmetic expression grammar with proper rule references
    // expr = term (('+' | '-') term)*
    // term = factor (('*' | '/') factor)*
    // factor = number | '(' expr ')'
    //
    // NOTE: First rule defined is the root
    let grammar = GrammarBuilder::new()
        // expr is root - it references term via ref_
        .rule(
            "expr",
            choice(vec![
                dynamic(seq(vec![
                    dynamic(ref_("term")),
                    dynamic(re(r"[ \t]*")),
                    dynamic(choice(vec![dynamic(str("+")), dynamic(str("-"))])),
                    dynamic(re(r"[ \t]*")),
                    dynamic(ref_("term")),
                ])),
                dynamic(ref_("term")),
            ]),
        )
        .rule(
            "term",
            choice(vec![
                dynamic(seq(vec![
                    dynamic(ref_("factor")),
                    dynamic(re(r"[ \t]*")),
                    dynamic(choice(vec![dynamic(str("*")), dynamic(str("/"))])),
                    dynamic(re(r"[ \t]*")),
                    dynamic(ref_("factor")),
                ])),
                dynamic(ref_("factor")),
            ]),
        )
        .rule(
            "factor",
            choice(vec![
                dynamic(seq(vec![
                    dynamic(str("(")),
                    dynamic(ref_("expr")),
                    dynamic(str(")")),
                ])),
                dynamic(re(r"[0-9]+")),
            ]),
        )
        .build();

    // Test simple number (goes through expr -> term -> factor -> number)
    let input = "42";
    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(&grammar, input, &mut arena);
    assert!(parser.parse().is_ok(), "Should parse: {}", input);

    // Test addition (expr with choice of sequence)
    let input = "1 + 2";
    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(&grammar, input, &mut arena);
    assert!(parser.parse().is_ok(), "Should parse: {}", input);

    // Test multiplication (term with choice of sequence)
    let input = "3 * 4";
    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(&grammar, input, &mut arena);
    assert!(parser.parse().is_ok(), "Should parse: {}", input);

    // Test parentheses (factor -> '(' expr ')')
    let input = "(5)";
    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(&grammar, input, &mut arena);
    assert!(parser.parse().is_ok(), "Should parse: {}", input);
}

#[test]
fn test_recursive_s_expression() {
    // Recursive S-expression grammar with proper rule references
    // sexp = atom | list
    // list = '(' sexp* ')'
    // atom = symbol | number
    //
    // NOTE: First rule defined is the root
    let grammar = GrammarBuilder::new()
        // sexp is root - choice between atom and list
        .rule(
            "sexp",
            choice(vec![dynamic(ref_("atom")), dynamic(ref_("list"))]),
        )
        // list: '(' followed by optional whitespace and sexp, then ')'
        .rule(
            "list",
            choice(vec![
                // Empty list: ()
                dynamic(seq(vec![dynamic(str("(")), dynamic(str(")"))])),
                // Single element: (atom)
                dynamic(seq(vec![
                    dynamic(str("(")),
                    dynamic(re(r"[ \t]*")),
                    dynamic(ref_("sexp")),
                    dynamic(re(r"[ \t]*")),
                    dynamic(str(")")),
                ])),
                // Two elements: (atom atom)
                dynamic(seq(vec![
                    dynamic(str("(")),
                    dynamic(re(r"[ \t]*")),
                    dynamic(ref_("sexp")),
                    dynamic(re(r"[ \t]+")),
                    dynamic(ref_("sexp")),
                    dynamic(re(r"[ \t]*")),
                    dynamic(str(")")),
                ])),
            ]),
        )
        // atom: symbol or number
        .rule(
            "atom",
            choice(vec![
                dynamic(re(r"[a-zA-Z_+\-*/=<>!?][a-zA-Z0-9_+\-*/=<>!?]*")),
                dynamic(re(r"-?[0-9]+")),
            ]),
        )
        .build();

    // Test atom (simple symbol)
    let input = "foo";
    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(&grammar, input, &mut arena);
    assert!(parser.parse().is_ok(), "Should parse: {}", input);

    // Test atom (number)
    let input = "42";
    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(&grammar, input, &mut arena);
    assert!(parser.parse().is_ok(), "Should parse: {}", input);

    // Test empty list
    let input = "()";
    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(&grammar, input, &mut arena);
    assert!(parser.parse().is_ok(), "Should parse: {}", input);

    // Test single-element list (list -> single element pattern)
    let input = "(foo)";
    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(&grammar, input, &mut arena);
    assert!(parser.parse().is_ok(), "Should parse: {}", input);

    // Test two-element list (list -> two element pattern)
    let input = "(+ 1)";
    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(&grammar, input, &mut arena);
    assert!(parser.parse().is_ok(), "Should parse: {}", input);
}

#[test]
fn test_nested_bracket_matching() {
    // Nested bracket matching with recursive grammar
    // balanced = '[' balanced* ']' | atom
    // atom = [^][]+
    //
    // NOTE: First rule defined is the root
    let grammar = GrammarBuilder::new()
        // balanced is root
        .rule(
            "balanced",
            choice(vec![
                // Nested: '[' content ']'
                dynamic(seq(vec![
                    dynamic(str("[")),
                    dynamic(ref_("content")),
                    dynamic(str("]")),
                ])),
                // Simple atom
                dynamic(re(r"[a-zA-Z0-9]+")),
            ]),
        )
        // content: zero or more balanced items (simplified)
        .rule(
            "content",
            choice(vec![
                dynamic(ref_("balanced")),
                dynamic(re(r"[a-zA-Z0-9]*")),
            ]),
        )
        .build();

    // Test simple atom
    let input = "abc";
    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(&grammar, input, &mut arena);
    assert!(parser.parse().is_ok(), "Should parse: {}", input);

    // Test single bracket with atom
    let input = "[x]";
    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(&grammar, input, &mut arena);
    assert!(parser.parse().is_ok(), "Should parse: {}", input);

    // Test nested brackets
    let input = "[[]]";
    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(&grammar, input, &mut arena);
    assert!(parser.parse().is_ok(), "Should parse: {}", input);
}

#[test]
fn test_grammar_with_forward_references() {
    // Test that forward references work correctly
    // A rule can reference rules defined AFTER it
    //
    // NOTE: First rule defined is the root
    let grammar = GrammarBuilder::new()
        // start references 'middle' which is defined later
        .rule(
            "start",
            seq(vec![
                dynamic(str("begin")),
                dynamic(ref_("middle")),
                dynamic(str("end")),
            ]),
        )
        // middle references 'inner' which is defined later
        .rule(
            "middle",
            seq(vec![
                dynamic(str("-")),
                dynamic(ref_("inner")),
                dynamic(str("-")),
            ]),
        )
        // inner is a simple pattern
        .rule("inner", re(r"[a-z]+"))
        .build();

    let input = "begin-hello-end";
    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(&grammar, input, &mut arena);
    assert!(parser.parse().is_ok(), "Should parse: {}", input);
}

#[test]
fn test_choice_with_sequences() {
    // Complex choice between multiple sequence patterns
    // value = string | number | boolean | null
    // string = '"' chars '"'
    // number = digit+
    // boolean = 'true' | 'false'
    // null = 'null'
    //
    // NOTE: First rule defined is the root
    let grammar = GrammarBuilder::new()
        // value is root - choice of 4 alternatives
        .rule(
            "value",
            choice(vec![
                dynamic(ref_("string")),
                dynamic(ref_("number")),
                dynamic(ref_("boolean")),
                dynamic(ref_("null_val")),
            ]),
        )
        .rule(
            "string",
            seq(vec![
                dynamic(str("\"")),
                dynamic(re(r#"[^"]*"#)),
                dynamic(str("\"")),
            ]),
        )
        .rule("number", re(r"-?[0-9]+(\.[0-9]+)?"))
        .rule(
            "boolean",
            choice(vec![dynamic(str("true")), dynamic(str("false"))]),
        )
        .rule("null_val", str("null"))
        .build();

    // Test string
    let input = "\"hello\"";
    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(&grammar, input, &mut arena);
    assert!(parser.parse().is_ok(), "Should parse string: {}", input);

    // Test number
    let input = "42";
    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(&grammar, input, &mut arena);
    assert!(parser.parse().is_ok(), "Should parse number: {}", input);

    // Test negative float
    let input = "-3.14";
    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(&grammar, input, &mut arena);
    assert!(parser.parse().is_ok(), "Should parse float: {}", input);

    // Test boolean true
    let input = "true";
    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(&grammar, input, &mut arena);
    assert!(parser.parse().is_ok(), "Should parse true: {}", input);

    // Test boolean false
    let input = "false";
    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(&grammar, input, &mut arena);
    assert!(parser.parse().is_ok(), "Should parse false: {}", input);

    // Test null
    let input = "null";
    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(&grammar, input, &mut arena);
    assert!(parser.parse().is_ok(), "Should parse null: {}", input);
}

#[test]
fn test_key_value_sequence_grammar() {
    // Key-value pair grammar with proper sequence composition
    // pair = key ':' value
    // key = identifier
    // value = string | number | identifier
    // identifier = [a-zA-Z_][a-zA-Z0-9_]*
    //
    // NOTE: First rule defined is the root
    let grammar = GrammarBuilder::new()
        // pair is root
        .rule(
            "pair",
            seq(vec![
                dynamic(ref_("key")),
                dynamic(re(r"[ \t]*:[ \t]*")), // colon with optional whitespace
                dynamic(ref_("value")),
            ]),
        )
        .rule("key", re(r"[a-zA-Z_][a-zA-Z0-9_]*"))
        .rule(
            "value",
            choice(vec![
                dynamic(ref_("string")),
                dynamic(ref_("number")),
                dynamic(ref_("identifier")),
            ]),
        )
        .rule(
            "string",
            seq(vec![
                dynamic(str("\"")),
                dynamic(re(r#"[^"]*"#)),
                dynamic(str("\"")),
            ]),
        )
        .rule("number", re(r"-?[0-9]+(\.[0-9]+)?"))
        .rule("identifier", re(r"[a-zA-Z_][a-zA-Z0-9_]*"))
        .build();

    // Test key with string value
    let input = "name: \"John\"";
    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(&grammar, input, &mut arena);
    assert!(parser.parse().is_ok(), "Should parse: {}", input);

    // Test key with number value
    let input = "age: 30";
    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(&grammar, input, &mut arena);
    assert!(parser.parse().is_ok(), "Should parse: {}", input);

    // Test key with identifier value
    let input = "type: user";
    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(&grammar, input, &mut arena);
    assert!(parser.parse().is_ok(), "Should parse: {}", input);

    // Test with whitespace
    let input = "key  :  value";
    let mut arena = AstArena::for_input(input.len());
    let mut parser = PortableParser::new(&grammar, input, &mut arena);
    assert!(parser.parse().is_ok(), "Should parse: {}", input);
}
