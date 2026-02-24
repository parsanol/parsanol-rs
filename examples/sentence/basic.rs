//! Sentence Parser Example (Unicode/Natural Language)
//!
//! This example demonstrates parsing sentences in natural languages.
//! Shows handling of Unicode characters and non-ASCII punctuation.
//! Based on the Parslet sentence.rb example.
//!
//! Run with: cargo run --example sentence_parser --no-default-features

#![allow(clippy::print_literal)]

use parsanol::portable::{
    parser_dsl::{re, GrammarBuilder},
    AstArena, Grammar, PortableParser,
};

/// Build a sentence grammar for various languages
fn build_sentence_grammar() -> Grammar {
    GrammarBuilder::new()
        // English sentence: ends with . ! or ?
        .rule("english_sentence", re("[^.!?]+[.!?]"))
        // Japanese sentence: ends with 。 (U+3002)
        .rule("japanese_sentence", re("[^。]+。"))
        // Chinese sentence: ends with 。 or ！ or ？
        .rule("chinese_sentence", re("[^。！？]+[。！？]"))
        // Mixed text: any text
        .rule("text", re(".+"))
        .build()
}

/// Parsed sentence
#[derive(Debug, Clone)]
pub struct Sentence {
    pub text: String,
    pub language: Language,
}

#[derive(Debug, Clone, Copy)]
pub enum Language {
    English,
    Japanese,
    Chinese,
    Unknown,
}

impl std::fmt::Display for Sentence {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{:?}] {}", self.language, self.text)
    }
}

/// Parse sentences from text
pub fn parse_sentences(input: &str) -> Vec<Sentence> {
    let mut sentences = Vec::new();
    let mut current = String::new();

    for ch in input.chars() {
        current.push(ch);

        // Check for sentence terminators
        let is_terminator = matches!(ch, '.' | '!' | '?' | '。' | '！' | '？');
        let is_english = matches!(ch, '.' | '!' | '?');
        let is_cjk = matches!(ch, '。' | '！' | '？');

        if is_terminator && current.len() > 1 {
            // Detect language based on characters
            let has_cjk = current.chars().any(|c| {
                ('\u{4E00}'..='\u{9FFF}').contains(&c) ||  // CJK
                ('\u{3040}'..='\u{30FF}').contains(&c) ||  // Japanese
                ('\u{AC00}'..='\u{D7AF}').contains(&c) // Korean
            });

            let language = if is_cjk || has_cjk {
                // Check if it has hiragana/katakana (Japanese)
                let has_japanese = current.chars().any(|c| {
                    ('\u{3040}'..='\u{309F}').contains(&c) ||  // Hiragana
                    ('\u{30A0}'..='\u{30FF}').contains(&c) // Katakana
                });
                if has_japanese {
                    Language::Japanese
                } else {
                    Language::Chinese
                }
            } else if is_english {
                Language::English
            } else {
                Language::Unknown
            };

            let text = current.trim().to_string();
            if !text.is_empty() {
                sentences.push(Sentence { text, language });
            }
            current.clear();
        }
    }

    // Handle remaining text
    let remaining = current.trim();
    if !remaining.is_empty() {
        sentences.push(Sentence {
            text: remaining.to_string(),
            language: Language::Unknown,
        });
    }

    sentences
}

/// Count sentences by language
pub fn count_by_language(sentences: &[Sentence]) -> (usize, usize, usize, usize) {
    let mut english = 0;
    let mut japanese = 0;
    let mut chinese = 0;
    let mut unknown = 0;

    for s in sentences {
        match s.language {
            Language::English => english += 1,
            Language::Japanese => japanese += 1,
            Language::Chinese => chinese += 1,
            Language::Unknown => unknown += 1,
        }
    }

    (english, japanese, chinese, unknown)
}

fn main() {
    println!("Sentence Parser Example (Unicode/Natural Language)");
    println!("===================================================\n");

    // English text
    let english = "Hello world. This is a test! How are you?";
    println!("=== English Text ===");
    println!("Input: {}", english);
    let sentences = parse_sentences(english);
    for s in &sentences {
        println!("  {}", s);
    }
    let (en, ja, zh, unk) = count_by_language(&sentences);
    println!(
        "Count: English={}, Japanese={}, Chinese={}, Unknown={}",
        en, ja, zh, unk
    );
    println!();

    // Japanese text
    let japanese = "こんにちは世界。これはテストです。元気ですか？";
    println!("=== Japanese Text ===");
    println!("Input: {}", japanese);
    let sentences = parse_sentences(japanese);
    for s in &sentences {
        println!("  {}", s);
    }
    let (en, ja, zh, unk) = count_by_language(&sentences);
    println!(
        "Count: English={}, Japanese={}, Chinese={}, Unknown={}",
        en, ja, zh, unk
    );
    println!();

    // Chinese text
    let chinese = "你好世界。这是一个测试！你好吗？";
    println!("=== Chinese Text ===");
    println!("Input: {}", chinese);
    let sentences = parse_sentences(chinese);
    for s in &sentences {
        println!("  {}", s);
    }
    let (en, ja, zh, unk) = count_by_language(&sentences);
    println!(
        "Count: English={}, Japanese={}, Chinese={}, Unknown={}",
        en, ja, zh, unk
    );
    println!();

    // Mixed text
    let mixed = "Hello world. こんにちは。This is mixed! 混合文本。";
    println!("=== Mixed Text ===");
    println!("Input: {}", mixed);
    let sentences = parse_sentences(mixed);
    for s in &sentences {
        println!("  {}", s);
    }
    let (en, ja, zh, unk) = count_by_language(&sentences);
    println!(
        "Count: English={}, Japanese={}, Chinese={}, Unknown={}",
        en, ja, zh, unk
    );

    // Demonstrate grammar parsing
    println!("\n=== Grammar Parsing ===");
    let grammar = build_sentence_grammar();

    let test_cases = vec![
        ("Hello world.", "english_sentence"),
        ("こんにちは。", "japanese_sentence"),
        ("你好。", "chinese_sentence"),
    ];

    for (input, rule) in test_cases {
        let mut arena = AstArena::for_input(input.len());
        let mut parser = PortableParser::new(&grammar, input, &mut arena);
        let result = parser.parse();
        println!("  {} ({}) => {:?}", input, rule, result.is_ok());
    }
}
