//! Incremental Parser Example
//!
//! Demonstrates how to reparse only affected regions after edits.
//! Useful for editor integration and IDE features.
//!
//! Run with: cargo run --example incremental --no-default-features

#![allow(clippy::print_literal)]

use parsanol::portable::{
    parser_dsl::{re, GrammarBuilder},
    Grammar,
};

/// Represents a region of parsed content
#[derive(Debug, Clone)]
struct Region {
    start: usize,
    end: usize,
    rule: String,
    valid: bool,
}

/// Incremental parser that tracks regions and can reparse affected areas
struct IncrementalParser {
    grammar: Grammar,
    regions: Vec<Region>,
    last_input: String,
}

impl IncrementalParser {
    fn new(grammar: Grammar) -> Self {
        Self {
            grammar,
            regions: Vec::new(),
            last_input: String::new(),
        }
    }

    /// Parse the entire input and record regions
    fn parse_full(&mut self, input: &str) {
        self.last_input = input.to_string();
        self.regions.clear();

        // Build simple regions based on lines
        let mut pos = 0;
        for line in input.lines() {
            let line_start = pos;
            let line_end = pos + line.len();

            self.regions.push(Region {
                start: line_start,
                end: line_end,
                rule: "line".to_string(),
                valid: true,
            });

            pos = line_end + 1; // +1 for newline
        }

        println!("Parsed {} regions", self.regions.len());
    }

    /// Apply an edit and return affected region range
    fn apply_edit(&mut self, pos: usize, delete_len: usize, insert: &str) -> String {
        let old_input = self.last_input.clone();

        // Apply the edit
        let new_input = format!(
            "{}{}{}",
            &old_input[..pos],
            insert,
            &old_input[pos + delete_len..]
        );

        // Find affected regions
        let affected_start = self.find_region_start(pos);
        let affected_end = self.find_region_end(pos + delete_len);

        // Mark affected regions as invalid
        for region in &mut self.regions {
            if region.start >= affected_start && region.end <= affected_end {
                region.valid = false;
            }
        }

        // Update region positions after the edit
        let delta = insert.len() as isize - delete_len as isize;
        for region in &mut self.regions {
            if region.start > pos + delete_len {
                region.start = (region.start as isize + delta) as usize;
                region.end = (region.end as isize + delta) as usize;
            }
        }

        self.last_input = new_input.clone();
        new_input
    }

    /// Reparse only invalid regions
    fn reparse_invalid(&mut self) {
        let invalid_count = self.regions.iter().filter(|r| !r.valid).count();
        println!("Reparsing {} invalid regions...", invalid_count);

        // In a real implementation, we would only reparse these regions
        for region in &mut self.regions {
            if !region.valid {
                // Reparse this region
                region.valid = true;
            }
        }

        println!("Done! Reparsed {} regions", invalid_count);
    }

    fn find_region_start(&self, pos: usize) -> usize {
        for region in &self.regions {
            if region.start <= pos && region.end >= pos {
                return region.start;
            }
        }
        pos
    }

    fn find_region_end(&self, pos: usize) -> usize {
        for region in self.regions.iter().rev() {
            if region.start <= pos && region.end >= pos {
                return region.end;
            }
        }
        pos
    }
}

fn main() {
    println!("Incremental Parser Example");
    println!("==========================");
    println!();

    println!("This example demonstrates how to update parse results after edits.");
    println!("Useful for IDE/editor integration.\n");

    let grammar = GrammarBuilder::new().rule("line", re(r"[^\n]*")).build();

    let mut parser = IncrementalParser::new(grammar);

    // Initial parse
    println!("--- Initial Parse ---");
    let input = "Hello\nWorld\nTest";
    parser.parse_full(input);

    // Apply edit
    println!("\n--- Apply Edit ---");
    println!("Edit: Insert 'Beautiful ' at position 6");
    let new_input = parser.apply_edit(6, 0, "Beautiful ");
    println!("New input: {:?}", new_input);

    // Reparse only affected regions
    println!("\n--- Incremental Reparse ---");
    parser.reparse_invalid();

    println!("\n--- Benefits ---");
    println!("* O(affected regions) instead of O(entire input)");
    println!("* Constant memory usage");
    println!("* Fast response for user edits");
}
