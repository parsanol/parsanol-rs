# Sentence Parser - Rust Implementation

## How to Run

```bash
cargo run --example sentence/basic --no-default-features
```

## Code Walkthrough

### Multi-Language Sentence Detection

The grammar defines patterns for different languages:

```rust
.rule("english_sentence", re("[^.!?]+[.!?]"))
.rule("japanese_sentence", re("[^。]+。"))
.rule("chinese_sentence", re("[^。！？]+[。！？]"))
```

English sentences end with `.`, `!`, or `?`. Japanese uses `。` (U+3002), Chinese uses `。`, `！`, or `？`.

### Unicode Character Detection

Language detection uses Unicode ranges:

```rust
let has_cjk = current.chars().any(|c| {
    ('\u{4E00}'..='\u{9FFF}').contains(&c) ||  // CJK
    ('\u{3040}'..='\u{30FF}').contains(&c) ||  // Japanese
    ('\u{AC00}'..='\u{D7AF}').contains(&c)     // Korean
});

let has_japanese = current.chars().any(|c| {
    ('\u{3040}'..='\u{309F}').contains(&c) ||  // Hiragana
    ('\u{30A0}'..='\u{30FF}').contains(&c)     // Katakana
});
```

Hiragana and Katakana ranges distinguish Japanese from Chinese.

### Sentence Parsing Algorithm

Sentences are parsed by accumulating characters until a terminator:

```rust
for ch in input.chars() {
    current.push(ch);
    let is_terminator = matches!(ch, '.' | '!' | '?' | '。' | '！' | '？');
    if is_terminator && current.len() > 1 {
        // Detect language and create sentence
        sentences.push(Sentence { text, language });
        current.clear();
    }
}
```

This handles mixed-language text by checking each sentence individually.

## Output Types

```rust
pub struct Sentence {
    pub text: String,
    pub language: Language,
}

pub enum Language {
    English,
    Japanese,
    Chinese,
    Unknown,
}
```

The output preserves both the text and detected language for each sentence.

## Design Decisions

### Why Character-Based Parsing?

Character-based iteration handles Unicode correctly. Multi-byte UTF-8 sequences are processed as single characters, avoiding truncation issues.

### Why Separate Language Detection?

Grammar rules alone cannot detect language; they only match patterns. Post-parse analysis using Unicode ranges provides accurate language classification.
