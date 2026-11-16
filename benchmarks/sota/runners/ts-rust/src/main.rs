//! The tree-sitter (rust binding) runner for the industry benchmark suite (rs#526).
//!
//! Usage: run_ts <corpus-dir> <rounds>
//!
//! Same line protocol as run_parsanol: `<doc> <bytes> <ns>` per doc
//! (min-of-N), then `TOTAL <bytes> <ns>`. Verifies each tree is
//! error-free and covers the full document before timing counts.

use std::hint::black_box;
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 {
        eprintln!("usage: run_ts <corpus-dir> <rounds>");
        std::process::exit(2);
    }
    let corpus_dir = std::path::Path::new(&args[1]);
    let rounds: u32 = args[2].parse().expect("rounds");

    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_json::LANGUAGE.into())
        .expect("json grammar loads");

    let mut docs: Vec<(String, String)> = Vec::new();
    let mut total_bytes = 0usize;
    for entry in std::fs::read_dir(corpus_dir).expect("corpus dir") {
        let path = entry.expect("entry").path();
        if path.extension().is_some_and(|x| x == "json") {
            let input = std::fs::read_to_string(&path).expect("doc");
            total_bytes += input.len();
            docs.push((path.file_name().unwrap().to_string_lossy().into_owned(), input));
        }
    }
    docs.sort();
    assert!(!docs.is_empty(), "no corpus documents");

    for (name, input) in &docs {
        let tree = parser
            .parse(input.as_bytes(), None)
            .unwrap_or_else(|| panic!("tree-sitter must parse {name}"));
        let root = tree.root_node();
        assert!(!root.has_error(), "tree-sitter error in {name}");
        assert_eq!(root.end_byte(), input.len(), "must consume {name}");
    }

    let mut total_ns = 0u128;
    for (name, input) in &docs {
        let mut best = u128::MAX;
        for _ in 0..rounds {
            let t = Instant::now();
            let tree = black_box(parser.parse(input.as_bytes(), None)).expect("parse");
            assert!(!tree.root_node().has_error());
            assert_eq!(tree.root_node().end_byte(), input.len());
            let ns = t.elapsed().as_nanos();
            if ns < best {
                best = ns;
            }
        }
        total_ns += best;
        println!("{name} {} {best}", input.len());
    }
    println!("TOTAL {total_bytes} {total_ns}");
}
