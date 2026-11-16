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

fn named_node_offsets(node: tree_sitter::Node, out: &mut Vec<u32>) {
    // Sorted start offsets of NAMED non-error nodes (the structure the
    // recovery metric compares against the ground-truth value spans;
    // anonymous tokens and ERROR/missing nodes are excluded).
    let mut stack = vec![node];
    while let Some(n) = stack.pop() {
        if !n.is_error() && !n.is_missing() && n.is_named() {
            out.push(n.start_byte() as u32);
        }
        let mut cursor = n.walk();
        if cursor.goto_first_child() {
            loop {
                stack.push(cursor.node());
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
        }
    }
    out.sort_unstable();
    out.dedup();
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
        let mut offsets = Vec::new();
        named_node_offsets(root, &mut offsets);
        let joined: Vec<String> = offsets.iter().map(|o| o.to_string()).collect();
        println!(
            "{name} {} {} {} false {}",
            input.len(),
            covered,
            first_err.unwrap_or(0),
            joined.join(",")
        );
    }
}

fn tree_hash(node: tree_sitter::Node, hasher: &mut std::collections::hash_map::DefaultHasher) {
    use std::hash::{Hash, Hasher};
    let mut cursor = node.walk();
    loop {
        let n = cursor.node();
        n.kind().hash(hasher);
        n.start_byte().hash(hasher);
        n.end_byte().hash(hasher);
        n.is_missing().hash(hasher);
        if cursor.goto_first_child() {
            continue;
        }
        if cursor.goto_next_sibling() {
            continue;
        }
        loop {
            if !cursor.goto_parent() {
                return;
            }
            if cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

fn point_at(text: &str, offset: usize) -> tree_sitter::Point {
    let bytes = text.as_bytes();
    let mut row = 0usize;
    let mut col = 0usize;
    for &b in &bytes[..offset] {
        if b == b'\n' {
            row += 1;
            col = 0;
        } else {
            col += 1;
        }
    }
    tree_sitter::Point::new(row, col)
}

fn incremental_mode(trace_path: &str, source_path: &str) {
    use std::time::Instant;

    let trace: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(trace_path).expect("trace")).expect("trace json");
    let mut text = std::fs::read_to_string(source_path).expect("source");
    let edits = trace["edits"].as_array().expect("edits").clone();

    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_json::LANGUAGE.into())
        .expect("json grammar loads");

    let mut old_tree = parser.parse(text.as_bytes(), None).expect("initial parse");
    println!("0 0 identity_ok");

    let mut latencies: Vec<u128> = Vec::new();
    let mut fresh_latencies: Vec<u128> = Vec::new();
    let mut identity_failures = 0usize;
    for (i, e) in edits.iter().enumerate() {
        let pos = e["pos"].as_u64().expect("pos") as usize;
        let del = e["del"].as_u64().expect("del") as usize;
        let ins = e["text"].as_str().expect("text").to_string();
        let mut next = String::with_capacity(text.len() + ins.len());
        next.push_str(&text[..pos]);
        next.push_str(&ins);
        next.push_str(&text[pos + del..]);

        // The editor's bookkeeping: splice the text, then mark the old
        // tree's changed range before reuse (without this, reused
        // boundaries stay stale and the tree is wrong).
        old_tree.edit(&tree_sitter::InputEdit {
            start_byte: pos,
            old_end_byte: pos + del,
            new_end_byte: pos + ins.len(),
            start_position: point_at(&text, pos),
            old_end_position: point_at(&text, pos + del),
            new_end_position: point_at(&next, pos + ins.len()),
        });

        let t = Instant::now();
        let new_tree = parser
            .parse(next.as_bytes(), Some(&old_tree))
            .expect("incremental parse");
        let ns = t.elapsed().as_nanos();
        latencies.push(ns);

        let tf = Instant::now();
        let fresh_tree = parser.parse(next.as_bytes(), None).expect("fresh parse");
        let fresh_ns = tf.elapsed().as_nanos();
        fresh_latencies.push(fresh_ns);
        let mut h1 = std::collections::hash_map::DefaultHasher::new();
        let mut h2 = std::collections::hash_map::DefaultHasher::new();
        tree_hash(new_tree.root_node(), &mut h1);
        tree_hash(fresh_tree.root_node(), &mut h2);
        let ok = !new_tree.root_node().has_error()
            && std::hash::Hasher::finish(&h1) == std::hash::Hasher::finish(&h2);
        if !ok {
            identity_failures += 1;
        }
        println!("{} {} {}", i + 1, ns, if ok { "identity_ok" } else { "IDENTITY_FAIL" });
        old_tree = new_tree;
        text = next;
    }

    latencies.sort();
    fresh_latencies.sort();
    let n = latencies.len();
    let p = |q: f64, v: &[u128]| v[((q * (v.len() - 1) as f64).round()) as usize];
    println!(
        "SUMMARY steps {} incr_p50 {} incr_p95 {} incr_max {} fresh_p50 {} fresh_p95 {} fresh_max {} identity_failures {}",
        n,
        p(0.50, &latencies),
        p(0.95, &latencies),
        latencies[n - 1],
        p(0.50, &fresh_latencies),
        p(0.95, &fresh_latencies),
        fresh_latencies[n - 1],
        identity_failures,
    );
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() == 4 && args[1] == "incremental" {
        incremental_mode(&args[2], &args[3]);
        return;
    }
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
