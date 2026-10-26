//! The PARG parser (rs#298 slice 2): token stream → document tree,
//! structure-parity with the Ruby parser. Slice scope: the structural
//! core — grammar/version, rules and expressions (choice/sequence/
//! repetition/optional/predicates/captures/references, literals,
//! `%x`/`%i`/`%s`/`%p`, brace bodies, atomic rules), entries, and the
//! embedded tests. Bindings blocks, preprocess, tables, and resync
//! are slice 3: the parity test records those fixtures as
//! expected-skips, never as silent passes.

use std::collections::BTreeMap;

use super::{lex, LexError, Token, TokenKind};
use serde::Serialize;

/// A tree node, mirroring the Ruby parser's Node (kind + payloads).
/// Serialization matches `node_to_h` in tools/dump_parg_document.rb
/// EXACTLY: per-kind payload keys, in its order, with explicit nulls
/// (rep's `max: null`) — the parity wire.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Node {
    /// The kind: lit / class / category / seq / alt / rep / opt / pred
    /// / cap / ref (the Ruby Node's names).
    pub kind: String,
    /// `lit`: the matched text (unquoted, unescaped).
    pub text: Option<String>,
    /// `lit`: case-insensitive (`%i`).
    pub fold: Option<bool>,
    /// `class`: inclusive codepoint ranges.
    pub ranges: Option<Vec<(u32, u32)>>,
    /// `category`: the general-category code inside `%p{…}`.
    pub code: Option<String>,
    /// `seq`: the juxtaposed children.
    pub items: Option<Vec<Node>>,
    /// `alt`: the ordered branches.
    pub branches: Option<Vec<Node>>,
    /// `rep`: the minimum iteration count.
    pub min: Option<u32>,
    /// `rep`: the maximum (none = unbounded).
    pub max: Option<u32>,
    /// rep / opt / pred / cap: the wrapped expression.
    pub child: Option<Box<Node>>,
    /// `cap`: the capture name; `ref`: the referenced rule.
    pub name: Option<String>,
    /// `pred`: true for `&x`, false for `!x`.
    pub positive: Option<bool>,
    /// `pred`: the behind-guard form (`!<x` / `&<x`).
    pub behind: Option<bool>,
    /// `table`: the table name.
    pub table: Option<String>,
    /// `table`: the column expanded into alternatives.
    pub column: Option<String>,
}

impl Node {
    fn leaf(kind: &str) -> Self {
        Node {
            kind: kind.to_string(),
            text: None,
            fold: None,
            ranges: None,
            code: None,
            items: None,
            branches: None,
            min: None,
            max: None,
            child: None,
            name: None,
            positive: None,
            behind: None,
            table: None,
            column: None,
        }
    }

    fn with(mut self, set: impl FnOnce(&mut Self)) -> Self {
        set(&mut self);
        self
    }
}

fn seq(items: Vec<Node>) -> Node {
    Node::leaf("seq").with(|n| n.items = Some(items))
}

fn alt(branches: Vec<Node>) -> Node {
    Node::leaf("alt").with(|n| n.branches = Some(branches))
}

fn rep(child: Node, min: u32, max: Option<u32>) -> Node {
    Node::leaf("rep").with(|n| {
        n.min = Some(min);
        n.max = max;
        n.child = Some(Box::new(child));
    })
}

fn opt(child: Node) -> Node {
    Node::leaf("opt").with(|n| n.child = Some(Box::new(child)))
}

fn pred(positive: bool, behind: bool, child: Node) -> Node {
    Node::leaf("pred").with(|n| {
        n.positive = Some(positive);
        if behind {
            n.behind = Some(true);
        }
        n.child = Some(Box::new(child));
    })
}

fn cap(name: String, child: Node) -> Node {
    Node::leaf("cap").with(|n| {
        n.name = Some(name);
        n.child = Some(Box::new(child));
    })
}

fn ref_node(name: String) -> Node {
    Node::leaf("ref").with(|n| n.name = Some(name))
}

fn lit_fold(text: String) -> Node {
    Node::leaf("lit").with(|n| {
        n.text = Some(text);
        n.fold = Some(true);
    })
}

fn plain_lit(text: String) -> Node {
    lit_fold(text).with(|n| n.fold = Some(false))
}

fn class_node(ranges: Vec<(u32, u32)>) -> Node {
    Node::leaf("class").with(|n| n.ranges = Some(ranges))
}

fn category(code: String) -> Node {
    Node::leaf("category").with(|n| n.code = Some(code))
}

fn hex_node(spec: &str) -> Result<Node, ParseError> {
    // `%x` was stripped by the lexer: `30-39` | `30` | `30.31.32`.
    let parse = |text: &str| u32::from_str_radix(text, 16).ok();
    let bad = || ParseError {
        message: format!("bad %x spec {spec:?}"),
    };
    if let Some((lo, hi)) = spec.split_once('-') {
        let lo = parse(lo).ok_or_else(bad)?;
        let hi = parse(hi).ok_or_else(bad)?;
        Ok(class_node(vec![(lo, hi)]))
    } else if spec.contains('.') {
        let mut items = Vec::new();
        for part in spec.split('.') {
            let value = parse(part).ok_or_else(bad)?;
            items.push(class_node(vec![(value, value)]));
        }
        // A dotted sequence is a SEQUENCE of single-codepoint classes.
        Ok(seq(items))
    } else {
        let value = parse(spec).ok_or_else(bad)?;
        Ok(class_node(vec![(value, value)]))
    }
}

fn unquote(quoted: &str) -> Result<String, ParseError> {
    let body = quoted
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .ok_or_else(|| ParseError {
            message: format!("not a quoted string: {quoted:?}"),
        })?;
    let mut out = String::new();
    let mut chars = body.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match chars.next() {
            Some('"') => out.push('"'),
            Some('\\') => out.push('\\'),
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    Ok(out)
}

/// One binding: capture -> path, with its type/cardinality and the
/// optional preprocess step name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Binding {
    /// The `as` capture the binding reads.
    pub capture: String,
    /// The bound path (`name`, `name[].leaf`).
    pub path: String,
    /// The value type (string/integer/float/boolean).
    #[serde(rename = "type")]
    pub kind: String,
    /// The cardinality (`0..1`, `0..*`, …).
    pub card: Option<String>,
    /// The preprocess step name, when declared.
    pub preprocess: Option<String>,
}

/// One preprocess step (table_lookup in v1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Step {
    /// The step operation (`table_lookup` in v1).
    pub op: String,
    /// The table to read.
    pub table: String,
    /// The key column.
    pub from: String,
    /// The value column.
    pub to: String,
}

/// One embedded test line: kind, its input, and the expectations.
impl Serialize for Node {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("kind", &self.kind)?;
        match self.kind.as_str() {
            "lit" => {
                map.serialize_entry("text", &self.text)?;
                map.serialize_entry("fold", &self.fold)?;
            }
            "class" => map.serialize_entry("ranges", &self.ranges)?,
            "category" => map.serialize_entry("code", &self.code)?,
            "seq" => map.serialize_entry("items", &self.items)?,
            "alt" => map.serialize_entry("branches", &self.branches)?,
            "rep" => {
                map.serialize_entry("min", &self.min)?;
                map.serialize_entry("max", &self.max)?;
                map.serialize_entry("child", &self.child)?;
            }
            "opt" => map.serialize_entry("child", &self.child)?,
            "pred" => {
                map.serialize_entry("positive", &self.positive)?;
                if self.behind == Some(true) {
                    map.serialize_entry("behind", &true)?;
                }
                map.serialize_entry("child", &self.child)?;
            }
            "cap" => {
                map.serialize_entry("name", &self.name)?;
                map.serialize_entry("child", &self.child)?;
            }
            "ref" => map.serialize_entry("name", &self.name)?,
            "table" => {
                map.serialize_entry("table", &self.table)?;
                map.serialize_entry("column", &self.column)?;
            }
            _ => {}
        }
        map.end()
    }
}

/// One embedded test line: kind, its input, and the expectations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TestLine {
    /// accept / reject / capture / example / shape.
    pub kind: String,
    /// The named entry the test rides, if declared.
    pub entry: Option<String>,
    /// The test input (unescaped).
    pub input: String,
    /// The capture/example expectations; `{}` when the line has none.
    pub expect: serde_json::Value,
}

/// The parsed document: the structural fields the Ruby parser's
/// Document carries (the parity wire).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
pub struct Document {
    /// `grammar NAME version …`'s name.
    pub grammar_name: String,
    /// The quoted version string.
    pub version: String,
    /// `entry NAME: rule` declarations, in order.
    pub entries: Vec<(String, String)>,
    /// Every embedded test line, in order.
    pub tests: Vec<TestLine>,
    /// Rules in declaration order (the Ruby hash preserves it).
    /// Rules in declaration order (the Ruby hash preserves it).
    pub rules: Vec<(String, Node)>,
    /// `bindings RULE { … }` sections, keyed by rule.
    pub bindings: BTreeMap<String, Vec<Binding>>,
    /// `preprocess NAME { … }` sections, keyed by name.
    pub preprocess: BTreeMap<String, Vec<Step>>,
    /// `resync RULE = pattern` declarations, keyed by rule.
    pub resyncs: BTreeMap<String, Node>,
    /// Top-level `use NAME` imports.
    pub uses: Vec<String>,
    /// The `skip = …` declaration's expression, when present.
    pub skip_declaration: Option<Box<Node>>,
    /// The `capture:` name on the skip declaration, when present.
    pub skip_capture: Option<String>,
}

/// A parse failure: what and (loosely) where.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    /// The human message, matching the Ruby parser's tone.
    pub message: String,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ParseError {}

impl From<LexError> for ParseError {
    fn from(err: LexError) -> Self {
        ParseError {
            message: format!("{err}"),
        }
    }
}

/// What the file parser produces: document, atomic names, docs.
type ParsedFile = (Document, Vec<String>, BTreeMap<String, String>);

/// The parse output that compiles from: the parity document plus the
/// out-of-band facts the envelope and the compiler need (atomic rule
/// names, the file's own entries, rule docs, and the source text).
/// Fields live here rather than on the public `Document` — adding to
/// an exhaustively-constructible pub struct is a semver major.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedSource {
    /// The parity document.
    pub document: Document,
    /// `atomic NAME = …` rule names, in declaration order.
    pub atomic_rules: Vec<String>,
    /// The entries this file declared (import merging adds more).
    pub own_entries: Vec<String>,
    /// `##` doc comments by rule name (the LSP hover contract).
    pub docs: BTreeMap<String, String>,
    /// The verbatim source (the envelope carries it + its hash).
    pub source: String,
}

/// Parse PARG source into the parity document.
pub fn parse(source: &str) -> Result<Document, ParseError> {
    Ok(parse_grammar(source)?.document)
}

/// The document plus its `atomic` rule names (the slice-4b shape).
pub fn parse_with_atomic(source: &str) -> Result<(Document, Vec<String>), ParseError> {
    parse_grammar(source).map(|parsed| (parsed.document, parsed.atomic_rules))
}

/// Parse PARG source into everything the compiler and envelope need.
pub fn parse_grammar(source: &str) -> Result<ParsedSource, ParseError> {
    let tokens = lex(source)?;
    let (document, atomic_rules, docs) = Parser::new(tokens).file()?;
    let own_entries = document
        .entries
        .iter()
        .map(|(name, _)| name.clone())
        .collect();
    Ok(ParsedSource {
        document,
        atomic_rules,
        own_entries,
        docs,
        source: source.to_string(),
    })
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Parser { tokens, pos: 0 }
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn peek_at(&self, ahead: usize) -> Option<&Token> {
        self.tokens.get(self.pos + ahead)
    }

    fn is_punct(&self, char: &str) -> bool {
        self.peek()
            .is_some_and(|t| t.kind == TokenKind::Punct && t.value == char)
    }

    fn bump(&mut self) -> Option<Token> {
        let token = self.tokens.get(self.pos).cloned();
        if token.is_some() {
            self.pos += 1;
        }
        token
    }

    fn accept_newline(&mut self) -> bool {
        if self.peek().map(|t| t.kind) == Some(TokenKind::Newline) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn skip_blank(&mut self) {
        while self.accept_newline() {}
    }

    fn expect_punct(&mut self, char: &str) -> Result<(), ParseError> {
        if self.is_punct(char) {
            self.pos += 1;
            Ok(())
        } else {
            Err(ParseError {
                message: format!(
                    "expected {char:?}, got {:?}",
                    self.peek()
                        .map(|t| (t.kind.as_ruby_name(), t.value.clone()))
                ),
            })
        }
    }

    fn expect_ident(&mut self) -> Result<String, ParseError> {
        if self.peek().map(|t| t.kind) == Some(TokenKind::Ident) {
            let name = self.peek().unwrap().value.clone();
            self.pos += 1;
            Ok(name)
        } else {
            Err(ParseError {
                message: format!(
                    "expected identifier, got {:?}",
                    self.peek()
                        .map(|t| (t.kind.as_ruby_name(), t.value.clone()))
                ),
            })
        }
    }

    fn expect_keyword(&mut self, keyword: &str) -> Result<(), ParseError> {
        let got = self.expect_ident()?;
        if got == keyword {
            Ok(())
        } else {
            Err(ParseError {
                message: format!("expected keyword {keyword:?}, got {got:?}"),
            })
        }
    }

    fn expect_arrow(&mut self) -> Result<(), ParseError> {
        if self.peek().map(|t| t.kind) == Some(TokenKind::Arrow) {
            self.pos += 1;
            Ok(())
        } else {
            Err(ParseError {
                message: format!(
                    "expected \"->\", got {:?}",
                    self.peek()
                        .map(|t| (t.kind.as_ruby_name(), t.value.clone()))
                ),
            })
        }
    }

    fn expect_string(&mut self) -> Result<String, ParseError> {
        if self.peek().map(|t| t.kind) == Some(TokenKind::Str) {
            let value = self.peek().unwrap().value.clone();
            self.pos += 1;
            Ok(value)
        } else {
            Err(ParseError {
                message: format!(
                    "expected string, got {:?}",
                    self.peek()
                        .map(|t| (t.kind.as_ruby_name(), t.value.clone()))
                ),
            })
        }
    }

    fn file(&mut self) -> Result<ParsedFile, ParseError> {
        let mut document = Document::default();
        let mut document_docs = BTreeMap::new();
        let mut atomic_rules = Vec::new();
        // Top-level items: `##` doc comments and `use` imports precede
        // the grammar block.
        loop {
            self.skip_blank();
            match self.peek().map(|t| t.kind) {
                Some(TokenKind::Doc) => {
                    self.bump();
                }
                Some(TokenKind::Ident)
                    if self.peek().map(|t| t.value.clone()) == Some("use".to_string()) =>
                {
                    self.bump();
                    let name = self.expect_ident()?;
                    document.uses.push(name);
                }
                _ => break,
            }
        }
        self.expect_keyword("grammar")?;
        document.grammar_name = self.expect_ident()?;
        self.expect_keyword("version")?;
        document.version = unquote(&self.expect_string()?)?;
        self.expect_punct("{")?;
        self.block(
            &mut document,
            &mut document_docs,
            &mut atomic_rules,
            &mut Vec::new(),
        )?;
        self.skip_blank();
        Ok((document, atomic_rules, document_docs))
    }

    #[allow(clippy::too_many_arguments)]
    fn block(
        &mut self,
        document: &mut Document,
        document_docs: &mut BTreeMap<String, String>,
        atomic_rules: &mut Vec<String>,
        docs: &mut Vec<String>,
    ) -> Result<(), ParseError> {
        loop {
            self.skip_blank();
            let Some(token) = self.peek().cloned() else {
                return Err(ParseError {
                    message: "unterminated grammar block".to_string(),
                });
            };
            match token.kind {
                TokenKind::Punct if token.value == "}" => {
                    self.pos += 1;
                    return Ok(());
                }
                TokenKind::Doc => {
                    self.bump();
                    let stripped = token
                        .value
                        .strip_prefix("##")
                        .unwrap_or(&token.value)
                        .strip_prefix(' ')
                        .unwrap_or(token.value.strip_prefix("##").unwrap_or(&token.value));
                    docs.push(stripped.to_string());
                }
                TokenKind::Ident => match token.value.as_str() {
                    "entry" => {
                        docs.clear();
                        self.entry(document)?;
                    }
                    "test" => {
                        docs.clear();
                        self.test(document)?;
                    }
                    "bindings" => {
                        docs.clear();
                        self.bindings_block(document)?;
                    }
                    "preprocess" => {
                        docs.clear();
                        self.preprocess_block(document)?;
                    }
                    "resync" => {
                        docs.clear();
                        self.resync_rule(document)?;
                    }
                    "render" | "derive" | "import" => {
                        // No corpus fixture exercises these; recognized
                        // and skipped.
                        self.skip_construct()?;
                    }
                    _ => {
                        let (name, atomic) = self.rule(document)?;
                        if !docs.is_empty() {
                            document_docs.entry(name.clone()).or_insert(docs.join("\n"));
                            docs.clear();
                        }
                        if atomic {
                            atomic_rules.push(name);
                        }
                    }
                },
                _ => {
                    let previous = if self.pos >= 2 {
                        self.tokens
                            .get(self.pos - 2)
                            .map(|t| format!("{} {:?}", t.kind.as_ruby_name(), t.value))
                            .unwrap_or_default()
                    } else {
                        String::new()
                    };
                    return Err(ParseError {
                        message: format!(
                            "unexpected {} {:?} in block (after {})",
                            token.kind.as_ruby_name(),
                            token.value,
                            previous
                        ),
                    });
                }
            }
        }
    }

    fn entry(&mut self, document: &mut Document) -> Result<(), ParseError> {
        self.bump();
        let name = self.expect_ident()?;
        self.expect_punct(":")?;
        let rule = self.expect_ident()?;
        document.entries.push((name, rule));
        Ok(())
    }

    fn test(&mut self, document: &mut Document) -> Result<(), ParseError> {
        self.bump();
        // `test NAME { … }` associates the tests with entry NAME; an
        // explicit `entry X` line inside overrides it.
        let mut entry: Option<String> = if self.peek().map(|t| t.kind) == Some(TokenKind::Ident) {
            Some(self.expect_ident()?)
        } else {
            None
        };
        self.expect_punct("{")?;
        loop {
            self.skip_blank();
            let Some(token) = self.peek().cloned() else {
                return Err(ParseError {
                    message: "unterminated test block".to_string(),
                });
            };
            match token.kind {
                TokenKind::Punct if token.value == "}" => {
                    self.pos += 1;
                    return Ok(());
                }
                TokenKind::Ident if token.value == "entry" => {
                    self.bump();
                    entry = Some(self.expect_ident()?);
                }
                TokenKind::Ident => {
                    let kind = token.value.clone();
                    self.bump();
                    let input = unquote(&self.expect_string()?)?;
                    // capture/example/shape carry expectations — any
                    // value form: `{ … }`, a bare `[ … ]` tree, or a
                    // plain string. accept/reject carry none.
                    let expects_value = matches!(kind.as_str(), "capture" | "example" | "shape");
                    let starts_value = self
                        .peek()
                        .map(|t| {
                            matches!(t.kind, TokenKind::Str | TokenKind::Num)
                                || self.is_punct("[")
                                || self.is_punct("{")
                                || self.is_punct("-")
                        })
                        .unwrap_or(false);
                    let expect = if expects_value && starts_value {
                        self.expect_value()?
                    } else {
                        serde_json::Value::Object(serde_json::Map::new())
                    };
                    document.tests.push(TestLine {
                        kind,
                        entry: entry.clone(),
                        input,
                        expect,
                    });
                }
                _ => {
                    return Err(ParseError {
                        message: format!("unexpected {} in test", token.kind.as_ruby_name()),
                    })
                }
            }
        }
    }

    fn expect_pairs(&mut self) -> Result<serde_json::Value, ParseError> {
        let mut map = serde_json::Map::new();
        loop {
            self.skip_blank();
            if self.is_punct("}") {
                return Ok(serde_json::Value::Object(map));
            }
            let key = self.expect_ident()?;
            self.expect_punct(":")?;
            let value = self.expect_value()?;
            map.insert(key, value);
            self.skip_blank();
            if self.is_punct(",") {
                self.pos += 1;
            }
        }
    }

    fn expect_value(&mut self) -> Result<serde_json::Value, ParseError> {
        self.skip_blank();
        // A negative expectation: the lexer emits `-` and `2.5` apart.
        let mut negative = false;
        if self.is_punct("-") && self.peek_at(1).map(|t| t.kind) == Some(TokenKind::Num) {
            self.pos += 1;
            negative = true;
        }
        let Some(token) = self.peek().cloned() else {
            return Err(ParseError {
                message: "unexpected end of value".to_string(),
            });
        };
        match token.kind {
            TokenKind::Str => {
                self.pos += 1;
                Ok(serde_json::Value::String(unquote(&token.value)?))
            }
            TokenKind::Num => {
                self.pos += 1;
                // Test expectations carry floats as `1.5` — the lexer's
                // `\d+` splits them; rejoin the fractional part.
                let mut text = token.value.clone();
                if self.is_punct(".") && self.peek_at(1).map(|t| t.kind) == Some(TokenKind::Num) {
                    self.pos += 1;
                    text.push('.');
                    text.push_str(&self.peek().unwrap().value);
                    self.pos += 1;
                }
                if negative {
                    text.insert(0, '-');
                }
                let number: serde_json::Number = if text.contains('.') {
                    let float = text.parse::<f64>().map_err(|_| ParseError {
                        message: format!("bad number {text}"),
                    })?;
                    serde_json::Number::from_f64(float).ok_or_else(|| ParseError {
                        message: format!("bad number {text}"),
                    })?
                } else {
                    text.parse().map_err(|_| ParseError {
                        message: format!("bad number {text}"),
                    })?
                };
                Ok(serde_json::Value::Number(number))
            }
            TokenKind::Ident => match token.value.as_str() {
                "true" => {
                    self.pos += 1;
                    Ok(serde_json::Value::Bool(true))
                }
                "false" => {
                    self.pos += 1;
                    Ok(serde_json::Value::Bool(false))
                }
                "null" => {
                    self.pos += 1;
                    Ok(serde_json::Value::Null)
                }
                other => Err(ParseError {
                    message: format!("unexpected value {other:?}"),
                }),
            },
            TokenKind::Punct if token.value == "[" => {
                self.pos += 1;
                let mut items = Vec::new();
                loop {
                    self.skip_blank();
                    if self.is_punct("]") {
                        self.pos += 1;
                        break;
                    }
                    items.push(self.expect_value()?);
                    self.skip_blank();
                    if self.is_punct(",") {
                        self.pos += 1;
                    }
                }
                Ok(serde_json::Value::Array(items))
            }
            TokenKind::Punct if token.value == "{" => {
                self.pos += 1;
                let value = self.expect_pairs()?;
                self.expect_punct("}")?;
                Ok(value)
            }
            _ => Err(ParseError {
                message: format!(
                    "unexpected {} {:?} in value (after {:?}; upcoming: {}; pos={})",
                    token.kind.as_ruby_name(),
                    token.value,
                    self.tokens
                        .get(self.pos.saturating_sub(1))
                        .map(|t| format!("{} {:?}", t.kind.as_ruby_name(), t.value))
                        .unwrap_or_else(|| "start".to_string()),
                    self.tokens
                        .iter()
                        .skip(self.pos)
                        .take(4)
                        .map(|t| format!("{} {:?}", t.kind.as_ruby_name(), t.value))
                        .collect::<Vec<_>>()
                        .join(", "),
                    self.pos,
                ),
            }),
        }
    }

    fn bindings_block(&mut self, document: &mut Document) -> Result<(), ParseError> {
        self.bump(); // bindings
        let rule = self.expect_ident()?;
        self.expect_punct("{")?;
        let mut list = Vec::new();
        loop {
            self.skip_blank();
            if self.is_punct("}") {
                self.pos += 1;
                break;
            }
            let capture = self.expect_ident()?;
            self.expect_arrow()?;
            let path = self.path_string()?;
            let mut binding = Binding {
                capture,
                path,
                kind: "string".to_string(),
                card: None,
                preprocess: None,
            };
            if self.is_punct("(") {
                self.pos += 1;
                binding.kind = self.expect_ident()?;
                if self.is_punct(",") {
                    self.pos += 1;
                    binding.card = Some(self.cardinality()?);
                }
                self.expect_punct(")")?;
            }
            if self.peek().map(|t| t.kind) == Some(TokenKind::Ident)
                && self.peek().map(|t| t.value.clone()) == Some("preprocess".to_string())
            {
                self.bump();
                self.expect_punct(":")?;
                binding.preprocess = Some(self.expect_ident()?);
            }
            list.push(binding);
        }
        document.bindings.insert(rule, list);
        Ok(())
    }

    /// A binding path: `name` with `.name` and `[]` segments, as text.
    fn path_string(&mut self) -> Result<String, ParseError> {
        let mut path = self.expect_ident()?;
        loop {
            if self.is_punct(".") {
                self.pos += 1;
                path.push('.');
                path.push_str(&self.expect_ident()?);
            } else if self.is_punct("[") {
                self.pos += 1;
                self.expect_punct("]")?;
                path.push_str("[]");
            } else {
                break;
            }
        }
        Ok(path)
    }

    fn cardinality(&mut self) -> Result<String, ParseError> {
        let mut card = String::new();
        loop {
            match self.peek() {
                Some(token) if token.kind == TokenKind::Num => {
                    card.push_str(&token.value);
                    self.pos += 1;
                }
                Some(token)
                    if token.kind == TokenKind::Punct
                        && (token.value == "." || token.value == "*") =>
                {
                    card.push_str(&token.value);
                    self.pos += 1;
                }
                _ => break,
            }
        }
        Ok(card)
    }

    fn preprocess_block(&mut self, document: &mut Document) -> Result<(), ParseError> {
        self.bump(); // preprocess
        let name = self.expect_ident()?;
        self.expect_punct("{")?;
        let mut steps = Vec::new();
        loop {
            self.skip_blank();
            if self.is_punct("}") {
                self.pos += 1;
                break;
            }
            self.expect_keyword("table_lookup")?;
            let table = self.expect_ident()?;
            let from = self.expect_ident()?;
            self.expect_arrow()?;
            let to = self.expect_ident()?;
            steps.push(Step {
                op: "table_lookup".to_string(),
                table,
                from,
                to,
            });
        }
        document.preprocess.insert(name, steps);
        Ok(())
    }

    fn resync_rule(&mut self, document: &mut Document) -> Result<(), ParseError> {
        self.bump(); // resync
        let rule = self.expect_ident()?;
        self.expect_punct("=")?;
        let pattern = self.line_expression()?.ok_or_else(|| ParseError {
            message: "resync pattern is empty".to_string(),
        })?;
        document.resyncs.insert(rule, pattern);
        self.accept_newline();
        Ok(())
    }

    /// Slice-3 constructs: skip to the matching closing brace.
    fn skip_construct(&mut self) -> Result<(), ParseError> {
        let mut depth = 0usize;
        loop {
            let Some(token) = self.bump() else {
                return Err(ParseError {
                    message: "unterminated construct".to_string(),
                });
            };
            if token.kind == TokenKind::Punct {
                match token.value.as_str() {
                    "{" => depth += 1,
                    "}" => {
                        depth -= 1;
                        if depth == 0 {
                            return Ok(());
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    fn rule(&mut self, document: &mut Document) -> Result<(String, bool), ParseError> {
        let first = self.expect_ident()?;
        if self.is_punct("=") {
            self.pos += 1;
            // `skip = X` is the skip DECLARATION, not a rule named
            // skip: the Ruby document carries no `skip` rule, and the
            // referenced rule re-attaches at the END of the rules
            // list (the observed wire). The declaration's own
            // semantics (injection, capture mode) are slice 3.
            if first == "skip" {
                let body = self.line_expression()?;
                if self.peek().map(|t| t.kind) == Some(TokenKind::Ident)
                    && self.peek().map(|t| t.value.clone()) == Some("capture".to_string())
                    && self.peek_at(1).map(|t| t.value.clone()) == Some(":".to_string())
                {
                    self.pos += 2;
                    document.skip_capture = Some(self.expect_ident()?);
                }
                if let Some(node) = body {
                    // The re-attach quirk: a skip REFERENCE moves its
                    // rule to the end of the list (the Ruby wire).
                    if node.kind == "ref" {
                        if let Some(target) = &node.name {
                            if let Some(index) =
                                document.rules.iter().position(|(n, _)| n == target)
                            {
                                let rule = document.rules.remove(index);
                                document.rules.push(rule);
                            }
                        }
                    }
                    document.skip_declaration = Some(Box::new(node));
                }
                self.accept_newline();
                return Ok((first, false));
            }
            self.rule_body(document, first.clone())?;
            return Ok((first, false));
        }
        // `atomic NAME = …` — the keyword, then the real name.
        let name = self.expect_ident()?;
        self.expect_punct("=")?;
        self.rule_body(document, name.clone())?;
        Ok((name, true))
    }

    fn rule_body(&mut self, document: &mut Document, name: String) -> Result<(), ParseError> {
        // Brace bodies (ruby#302): `name = { ... }` is
        // self-delimiting. The brace may also sit on the next line:
        // `name =` NL `{`. Peek past the newline; restore when it is
        // not a brace so line-mode error behavior is untouched (the
        // Ruby parser's exact order).
        let mut brace = self.is_punct("{");
        if !brace {
            let saved = self.pos;
            self.accept_newline();
            self.skip_blank();
            if self.is_punct("{") {
                brace = true;
            } else {
                self.pos = saved;
            }
        }
        let node = if brace {
            self.pos += 1;
            let body = self.brace_choice()?;
            self.expect_punct("}")?;
            body
        } else {
            let line = self.line_expression()?;
            self.accept_newline();
            self.skip_blank();
            line.ok_or_else(|| ParseError {
                message: format!("rule {name:?} has an empty body"),
            })?
        };
        document.rules.push((name, node));
        Ok(())
    }

    // ---- brace bodies: newlines are whitespace ----

    fn brace_choice(&mut self) -> Result<Node, ParseError> {
        let mut branches = vec![self.brace_sequence()?];
        while self.is_punct("/") {
            self.pos += 1;
            branches.push(self.brace_sequence()?);
        }
        Ok(if branches.len() == 1 {
            branches.pop().unwrap()
        } else {
            alt(branches)
        })
    }

    fn brace_sequence(&mut self) -> Result<Node, ParseError> {
        let mut items = Vec::new();
        loop {
            self.skip_blank();
            if self.is_punct("/") || self.is_punct("}") {
                break;
            }
            match self.brace_element()? {
                Some(element) => items.push(element),
                None => break,
            }
        }
        if items.is_empty() {
            return Err(ParseError {
                message: "empty brace body".to_string(),
            });
        }
        Ok(if items.len() == 1 {
            items.pop().unwrap()
        } else {
            seq(items)
        })
    }

    fn brace_element(&mut self) -> Result<Option<Node>, ParseError> {
        self.skip_blank();
        if self.is_punct("}") || self.peek().is_none() {
            return Ok(None);
        }
        if self.is_punct("!") || self.is_punct("&") {
            let positive = self.is_punct("&");
            self.pos += 1;
            let behind = self.is_punct("<");
            if behind {
                self.pos += 1;
            }
            let child = self.brace_element()?.ok_or_else(|| ParseError {
                message: "predicate needs a target".to_string(),
            })?;
            return Ok(Some(pred(positive, behind, child)));
        }
        if let Some((min, max)) = self.repetition_prefix()? {
            // `1*X as n` captures the WHOLE repetition (cap outside the
            // rep) — the Ruby wire's shape.
            let child = self.brace_primary()?.ok_or_else(|| ParseError {
                message: "repetition needs a target".to_string(),
            })?;
            return Ok(Some(self.postfix_capture(rep(child, min, max), true)?));
        }
        match self.brace_primary()? {
            Some(primary) => Ok(Some(self.postfix_capture(primary, true)?)),
            None => Ok(None),
        }
    }

    fn repetition_prefix(&mut self) -> Result<Option<(u32, Option<u32>)>, ParseError> {
        if self.peek().map(|t| t.kind) == Some(TokenKind::Num) {
            let min = self
                .peek()
                .unwrap()
                .value
                .parse::<u32>()
                .map_err(|_| ParseError {
                    message: format!("bad repetition {}", self.peek().unwrap().value),
                })?;
            self.pos += 1;
            // A bare `<n>` prefix is the RFC 5234 exact form: `2digit`
            // is exactly two (the ABNF importer emits it). The `*`
            // spelling is the open/range form.
            if self.is_punct("*") {
                self.pos += 1;
                let max = self.optional_number()?;
                return Ok(Some((min, max)));
            }
            return Ok(Some((min, Some(min))));
        }
        if self.is_punct("*") {
            self.pos += 1;
            let max = self.optional_number()?;
            return Ok(Some((0, max)));
        }
        Ok(None)
    }

    fn optional_number(&mut self) -> Result<Option<u32>, ParseError> {
        if self.peek().map(|t| t.kind) == Some(TokenKind::Num) {
            let value = self.peek().unwrap().value.clone();
            let parsed = value.parse::<u32>().map_err(|_| ParseError {
                message: format!("bad repetition {value}"),
            })?;
            self.pos += 1;
            Ok(Some(parsed))
        } else {
            Ok(None)
        }
    }

    fn brace_primary(&mut self) -> Result<Option<Node>, ParseError> {
        self.skip_blank();
        if self.is_punct("}") || self.is_punct("/") || self.peek().is_none() {
            return Ok(None);
        }
        let Some(token) = self.peek().cloned() else {
            return Ok(None);
        };
        match token.kind {
            TokenKind::Punct if token.value == "(" => {
                self.pos += 1;
                let node = self.brace_choice()?;
                self.expect_punct(")")?;
                Ok(Some(node))
            }
            TokenKind::Punct if token.value == "[" => {
                self.pos += 1;
                let node = self.brace_choice()?;
                self.expect_punct("]")?;
                Ok(Some(opt(node)))
            }
            TokenKind::Str => {
                self.pos += 1;
                Ok(Some(plain_lit(unquote(&token.value)?)))
            }
            TokenKind::Istring => {
                self.pos += 1;
                Ok(Some(lit_fold(unquote(&token.value)?)))
            }
            TokenKind::Sstring => {
                self.pos += 1;
                Ok(Some(plain_lit(unquote(&token.value)?)))
            }
            TokenKind::Hex => {
                self.pos += 1;
                Ok(Some(hex_node(&token.value)?))
            }
            TokenKind::Category => {
                self.pos += 1;
                Ok(Some(category(
                    token
                        .value
                        .trim_matches(|c| c == '{' || c == '}')
                        .to_string(),
                )))
            }
            TokenKind::Ident => {
                self.pos += 1;
                let mut name = token.value;
                // Qualified references: helper.code (use imports).
                while self.is_punct(".")
                    && self.peek_at(1).map(|t| t.kind) == Some(TokenKind::Ident)
                {
                    self.pos += 1;
                    name.push('.');
                    name.push_str(&self.expect_ident()?);
                }
                Ok(Some(ref_node(name)))
            }
            _ => Ok(None),
        }
    }

    fn postfix_capture(&mut self, node: Node, newline_ok: bool) -> Result<Node, ParseError> {
        // LINE rules end at the newline — whitespace crossing is only
        // legal inside brace bodies (where newlines are whitespace).
        if newline_ok {
            self.skip_blank();
        }
        if self.peek().map(|t| t.kind) == Some(TokenKind::Ident)
            && self.peek().map(|t| t.value.clone()) == Some("as".to_string())
        {
            self.pos += 1;
            let name = self.expect_ident()?;
            Ok(cap(name, node))
        } else {
            Ok(node)
        }
    }

    // ---- line rules: a newline terminates ----

    fn line_expression(&mut self) -> Result<Option<Node>, ParseError> {
        let mut branches = vec![self.line_sequence()?];
        while self.is_punct("/") {
            self.pos += 1;
            branches.push(self.line_sequence()?);
        }
        Ok(Some(if branches.len() == 1 {
            branches.pop().unwrap()
        } else {
            alt(branches)
        }))
    }

    fn line_sequence(&mut self) -> Result<Node, ParseError> {
        let mut items = Vec::new();
        loop {
            // `capture:` is the skip declaration's reserved suffix —
            // it terminates the line like a newline (parsanol-ruby#152).
            let capture_suffix = self.peek().map(|t| t.kind) == Some(TokenKind::Ident)
                && self.peek().map(|t| t.value.clone()) == Some("capture".to_string())
                && self.peek_at(1).map(|t| t.value.clone()) == Some(":".to_string());
            if self.peek().map(|t| t.kind) == Some(TokenKind::Newline)
                || self.is_punct("/")
                || self.is_punct("}")
                || capture_suffix
            {
                break;
            }
            match self.line_element()? {
                Some(element) => items.push(element),
                None => break,
            }
        }
        if items.is_empty() {
            let offset = self.peek().map(|t| t.offset).unwrap_or(usize::MAX);
            return Err(ParseError {
                message: format!("empty line (at char offset {offset})"),
            });
        }
        Ok(if items.len() == 1 {
            items.pop().unwrap()
        } else {
            seq(items)
        })
    }

    fn line_element(&mut self) -> Result<Option<Node>, ParseError> {
        let Some(token) = self.peek().cloned() else {
            return Ok(None);
        };
        match token.kind {
            TokenKind::Newline => Ok(None),
            TokenKind::Punct if token.value == "!" || token.value == "&" => {
                let positive = token.value == "&";
                self.pos += 1;
                let behind = self.is_punct("<");
                if behind {
                    self.pos += 1;
                }
                let child = self.line_element()?.ok_or_else(|| ParseError {
                    message: "predicate needs a target".to_string(),
                })?;
                Ok(Some(pred(positive, behind, child)))
            }
            TokenKind::Num => {
                let min = token.value.parse::<u32>().map_err(|_| ParseError {
                    message: format!("bad repetition {}", token.value),
                })?;
                self.pos += 1;
                // A bare `<n>` prefix is the RFC 5234 exact form: `2digit`
                // is exactly two (the ABNF importer emits it).
                let max = if self.is_punct("*") {
                    self.pos += 1;
                    self.optional_number()?
                } else {
                    Some(min)
                };
                // `1*X as n` captures the WHOLE repetition.
                let child = self.line_primary()?.ok_or_else(|| ParseError {
                    message: "repetition needs a target".to_string(),
                })?;
                Ok(Some(self.postfix_capture(rep(child, min, max), false)?))
            }
            TokenKind::Punct if token.value == "*" => {
                self.pos += 1;
                let max = self.optional_number()?;
                let child = self.line_primary()?.ok_or_else(|| ParseError {
                    message: "repetition needs a target".to_string(),
                })?;
                Ok(Some(self.postfix_capture(rep(child, 0, max), false)?))
            }
            _ => {
                let Some(primary) = self.line_primary()? else {
                    return Ok(None);
                };
                Ok(Some(self.postfix_capture(primary, false)?))
            }
        }
    }

    fn line_primary(&mut self) -> Result<Option<Node>, ParseError> {
        let Some(token) = self.peek().cloned() else {
            return Ok(None);
        };
        match token.kind {
            TokenKind::Newline => Ok(None),
            TokenKind::Punct if token.value == "(" => {
                self.pos += 1;
                let node = self.line_expression()?;
                self.expect_punct(")")?;
                Ok(node)
            }
            TokenKind::Punct if token.value == "[" => {
                self.pos += 1;
                let node = self.line_expression()?;
                self.expect_punct("]")?;
                Ok(Some(opt(node.ok_or_else(|| ParseError {
                    message: "empty optional".to_string(),
                })?)))
            }
            TokenKind::Str => {
                self.pos += 1;
                Ok(Some(plain_lit(unquote(&token.value)?)))
            }
            TokenKind::Istring => {
                self.pos += 1;
                Ok(Some(lit_fold(unquote(&token.value)?)))
            }
            TokenKind::Sstring => {
                self.pos += 1;
                Ok(Some(plain_lit(unquote(&token.value)?)))
            }
            TokenKind::Hex => {
                self.pos += 1;
                Ok(Some(hex_node(&token.value)?))
            }
            TokenKind::Category => {
                self.pos += 1;
                Ok(Some(category(
                    token
                        .value
                        .trim_matches(|c| c == '{' || c == '}')
                        .to_string(),
                )))
            }
            TokenKind::Ident
                if token.value == "alt"
                    && self.peek_at(1).map(|t| t.value.clone())
                        == Some("from_table".to_string()) =>
            {
                self.pos += 2;
                let table = unquote(&self.expect_string()?)?;
                self.expect_keyword("column")?;
                let column = unquote(&self.expect_string()?)?;
                Ok(Some(Node::leaf("table").with(|n| {
                    n.table = Some(table);
                    n.column = Some(column);
                })))
            }
            TokenKind::Ident => {
                self.pos += 1;
                let mut name = token.value;
                // Qualified references: helper.code (use imports).
                while self.is_punct(".")
                    && self.peek_at(1).map(|t| t.kind) == Some(TokenKind::Ident)
                {
                    self.pos += 1;
                    name.push('.');
                    name.push_str(&self.expect_ident()?);
                }
                Ok(Some(ref_node(name)))
            }
            _ => Ok(None),
        }
    }
}
