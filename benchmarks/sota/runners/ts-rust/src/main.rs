//! The tree-sitter (rust binding) runner for the industry benchmark suite (rs#526).
//!
//! Usage: run_ts <corpus-dir> <rounds>
//!
//! Same line protocol as run_parsanol: `<doc> <bytes> <ns>` per doc
//! (min-of-N), then `TOTAL <bytes> <ns>`. Verifies each tree is
//! error-free and covers the full document before timing counts.

use std::hint::black_box;
use std::time::Instant;

fn count_error_bytes(root: tree_sitter::Node) -> (usize, Option<usize>) {
    // Bytes under error/missing structure, crediting recovered non-error
    // subtrees INSIDE error regions (tree-sitter wraps recovered content
    // in ERROR parents — e.g. a root ERROR containing recovered pairs).
    // A byte is unrecovered iff it lies in an error region and not in a
    // non-error node below that region.
    let mut err_region = 0usize;
    let mut recovered_inside = 0usize;
    let mut first_err: Option<usize> = None;
    let mut stack: Vec<(tree_sitter::Node, usize)> = vec![(root, 0)]; // (node, error_depth)
    while let Some((n, err_depth)) = stack.pop() {
        let is_bad = n.is_error() || n.is_missing();
        let mut cursor = n.walk();
        let has_children = cursor.goto_first_child();
        if is_bad {
            if err_depth == 0 {
                err_region += n.end_byte().saturating_sub(n.start_byte());
                if first_err.is_none() {
                    first_err = Some(n.start_byte());
                }
            }
        } else if err_depth > 0 && !has_children {
            recovered_inside += n.end_byte().saturating_sub(n.start_byte());
        }
        let child_depth = if is_bad { err_depth + 1 } else { err_depth };
        if has_children {
            loop {
                stack.push((cursor.node(), child_depth));
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
        }
    }
    (err_region.saturating_sub(recovered_inside), first_err)
}

fn recovery_mode(corpus_dir: &str) {
    let corpus = std::path::Path::new(corpus_dir);
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_json::LANGUAGE.into())
        .expect("json grammar loads");

    let mut paths: Vec<std::path::PathBuf> = std::fs::read_dir(corpus)
        .expect("corpus dir")
        .map(|e| e.expect("entry").path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    paths.sort();
    for path in paths {
        let input = std::fs::read_to_string(&path).expect("doc");
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let tree = parser.parse(input.as_bytes(), None).expect("parse returns a tree");
        let root = tree.root_node();
        if !root.has_error() {
            println!("{name} {} 0 true", input.len());
            continue;
        }
        let (err_bytes, first_err) = count_error_bytes(root);
        let covered = input.len() - err_bytes.min(input.len());
        println!(
            "{name} {} {} {} false",
            input.len(),
            covered,
            first_err.unwrap_or(0)
        );
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() == 3 && args[1] == "recovery" {
        recovery_mode(&args[2]);
        return;
    }
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
