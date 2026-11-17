//! The parsanol runner for the industry benchmark suite (rs#526).
//!
//! Usage: run_parsanol <artifact.json> <corpus-dir> <rounds> <vm|interpreter>
//!
//! Parses every corpus document `rounds` times with the selected engine,
//! prints one line per document: `<doc> <bytes> <ns_per_parse>` (the
//! minimum over the rounds), then `TOTAL <bytes> <total_ns>`. The driver
//! aggregates the totals into the published table.
//!
//! Both engines must fully consume each document; any failure exits
//! non-zero (a bench that silently short-parses is worthless — rs#496).

use parsanol::portable::bytecode::compiler::compile;
use parsanol::portable::bytecode::parse_with_vm_diag;
use parsanol::portable::{AstArena, Grammar, PortableParser};
use std::hint::black_box;
use std::time::Instant;

fn collect_shape_offsets(node: &serde_json::Value, out: &mut Vec<String>) {
    // Every node with an offset field is a recovered structure node,
    // emitted as a `start:end` byte span (the span-aware scorer
    // matches recovered spans by containment); recurse into nested
    // objects and arrays, dedup+sort.
    if let Some(off) = node.get("offset").and_then(|v| v.as_u64()) {
        let len = node.get("length").and_then(|v| v.as_u64()).unwrap_or(0);
        out.push(format!("{}:{}", off, off + len));
    }
    match node {
        serde_json::Value::Object(map) => {
            for (_k, v) in map {
                collect_shape_offsets(v, out);
            }
        }
        serde_json::Value::Array(items) => {
            for v in items {
                collect_shape_offsets(v, out);
            }
        }
        _ => {}
    }
}

fn recovery_mode(corpus_dir: &str) {
    // Per corrupted case: `<file> <covered> <len> <first_err>` — covered
    // is the byte count parsed into structure (total minus the consumed
    // junk spans, the same accounting as tree-sitter's non-ERROR bytes),
    // so the scorer joins both engines symmetrically. A clean full parse
    // prints covered=0 with ok=true (column 4 absent, like the old
    // protocol's ok rows).
    let corpus = std::path::Path::new(corpus_dir);
    let artifact_text = std::fs::read_to_string("json-artifact.json").expect("artifact");
    let artifact = parsanol::PargArtifact::from_json(&artifact_text).expect("artifact loads");

    let mut paths: Vec<std::path::PathBuf> = std::fs::read_dir(corpus)
        .expect("corpus dir")
        .map(|e| e.expect("entry").path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    paths.sort();
    for path in paths {
        let input = std::fs::read_to_string(&path).expect("doc");
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        match artifact.parse_shape_recovering("main", &input) {
            Err(_) => println!("{name} {} 0 false 0", input.len()),
            Ok(envelope) => {
                if envelope["ok"].as_bool() == Some(true) {
                    println!("{name} {} 0 true", input.len());
                    continue;
                }
                let mut junk_end: usize = 0;
                if let Some(spans) = envelope["resyncs"].as_array() {
                    for span in spans {
                        let off = span["offset"].as_u64().unwrap_or(0) as usize;
                        let resumed = span["resumed_at"].as_u64().unwrap_or(off as u64) as usize;
                        junk_end += resumed.saturating_sub(off);
                    }
                }
                // Coverage is only what the parse actually consumed past
                // its junk spans; without resync rounds (non-spine
                // grammars, rs#539) nothing is consumed and the parse
                // recovered nothing — covered 0, with the failure offset
                // reported separately for the diagnostic metric.
                let consumed = envelope["consumed_upto"].as_u64().unwrap_or(0) as usize;
                // The resync rounds resume past junk: their furthest
                // recovered span end is the coverage bound, not the
                // original parse's consumption.
                let mut covered_bound = consumed;
                if let Some(spans) = envelope["recovered_spans"].as_array() {
                    if let Some(last) = spans.last() {
                        let end = last["end"].as_u64().unwrap_or(0) as usize;
                        covered_bound = covered_bound.max(end);
                    }
                }
                let covered = covered_bound.saturating_sub(junk_end).min(input.len());
                let first_err = envelope["error"]["offset"].as_u64().unwrap_or(0) as usize;
                let mut offsets = Vec::new();
                if let Some(captures) = envelope.get("captures") {
                    collect_shape_offsets(captures, &mut offsets);
                }
                // The recovered iteration spans ride "recovered_spans"
                // (engine-side, offset-exact) as `start:end` pairs.
                if let Some(spans) = envelope["recovered_spans"].as_array() {
                    for span in spans {
                        let off = span["offset"].as_u64().unwrap_or(0);
                        let end = span["end"].as_u64().unwrap_or(off);
                        offsets.push(format!("{off}:{end}"));
                    }
                }
                offsets.sort_unstable();
                offsets.dedup();
                let joined: Vec<String> = offsets.iter().map(|o| o.to_string()).collect();
                println!(
                    "{name} {} {} false {} {}",
                    input.len(),
                    covered,
                    first_err,
                    joined.join(",")
                );
            }
        }
    }
}

fn variant_name(n: &parsanol::portable::AstNode) -> &'static str {
    use parsanol::portable::AstNode as N;
    match n {
        N::Nil => "Nil",
        N::Bool(_) => "Bool",
        N::Int(_) => "Int",
        N::Float(_) => "Float",
        N::StringRef { .. } => "StringRef",
        N::InputRef { .. } => "InputRef",
        N::Array { .. } => "Array",
        N::Hash { .. } => "Hash",
        _ => "Other",
    }
}

fn deep_eq_dbg(
    a: parsanol::portable::AstNode,
    aa: &AstArena,
    b: parsanol::portable::AstNode,
    ab: &AstArena,
    path: &str,
    depth: usize,
) -> bool {
    use parsanol::portable::AstNode as N;
    if variant_name(&a) != variant_name(&b) {
        eprintln!(
            "MISMATCH at [{}] {} vs {}",
            path,
            variant_name(&a),
            variant_name(&b)
        );
        return false;
    }
    if depth > 40 {
        eprintln!("MISMATCH at [{}] depth limit", path);
        return false;
    }
    match (&a, &b) {
        (N::Nil, N::Nil) => true,
        (N::Bool(x), N::Bool(y)) => x == y,
        (N::Int(x), N::Int(y)) => x == y,
        (N::Float(x), N::Float(y)) => x == y,
        (N::StringRef { pool_index: x }, N::StringRef { pool_index: y }) => {
            let eq = aa.get_string(*x as usize) == ab.get_string(*y as usize);
            if !eq {
                eprintln!(
                    "MISMATCH at [{}] StringRef {:?} vs {:?}",
                    path,
                    aa.get_string(*x as usize),
                    ab.get_string(*y as usize)
                );
            }
            eq
        }
        (
            N::InputRef {
                offset: o1,
                length: l1,
            },
            N::InputRef {
                offset: o2,
                length: l2,
            },
        ) => {
            let i1 = aa.get_input();
            let i2 = ab.get_input();
            let l1 = *l1 as usize;
            let l2 = *l2 as usize;
            let o1 = *o1 as usize;
            let o2 = *o2 as usize;
            let eq = o1 == o2 && l1 == l2 && i1.get(o1..o1 + l1) == i2.get(o2..o2 + l2);
            if !eq {
                eprintln!(
                    "MISMATCH at [{}] InputRef {}+{} vs {}+{} ({:?} vs {:?})",
                    path,
                    o1,
                    l1,
                    o2,
                    l2,
                    i1.get(o1..o1 + l1),
                    i2.get(o2..o2 + l2)
                );
            }
            eq
        }
        (
            N::Array {
                pool_index: p1,
                length: l1,
            },
            N::Array {
                pool_index: p2,
                length: l2,
            },
        ) => {
            if l1 != l2 {
                let dump = |arena: &AstArena, node: &N, inlen: usize| -> String {
                    match node {
                        N::Array { pool_index, length } => {
                            let items = arena.get_array(*pool_index as usize, *length as usize);
                            items
                                .iter()
                                .map(|it| match it {
                                    N::InputRef { offset, length } => {
                                        format!(
                                            "[{}..{}]={:?}",
                                            offset,
                                            offset + length,
                                            &arena.get_input()[*offset as usize..(*offset as usize + *length as usize).min(inlen)]
                                        )
                                    }
                                    other => format!("{:?}", variant_name(other)),
                                })
                                .collect::<Vec<_>>()
                                .join(" ")
                        }
                        _ => "?".to_string(),
                    }
                };
                eprintln!(
                    "MISMATCH at [{}] Array len {} vs {}\n  incr: {}\n  fresh: {}",
                    path,
                    l1,
                    l2,
                    dump(aa, &a, aa.get_input().len()),
                    dump(ab, &b, ab.get_input().len())
                );
                return false;
            }
            let a_items = aa.get_array(*p1 as usize, *l1 as usize);
            let b_items = ab.get_array(*p2 as usize, *l2 as usize);
            for (i, (x, y)) in a_items.into_iter().zip(b_items).enumerate() {
                let sub = format!("{}.{}", path, i);
                if !deep_eq_dbg(x, aa, y, ab, &sub, depth + 1) {
                    return false;
                }
            }
            true
        }
        (
            N::Hash {
                pool_index: p1,
                length: l1,
            },
            N::Hash {
                pool_index: p2,
                length: l2,
            },
        ) => {
            if l1 != l2 {
                eprintln!("MISMATCH at [{}] Hash len {} vs {}", path, l1, l2);
                return false;
            }
            let a_items = aa.get_hash_items(*p1 as usize, *l1 as usize);
            let b_items = ab.get_hash_items(*p2 as usize, *l2 as usize);
            for (i, ((k1, x), (k2, y))) in a_items.into_iter().zip(b_items).enumerate() {
                if k1 != k2 {
                    eprintln!("MISMATCH at [{}] Hash key {} vs {}", path, k1, k2);
                    return false;
                }
                let sub = format!("{}.{}", path, i);
                if !deep_eq_dbg(x, aa, y, ab, &sub, depth + 1) {
                    return false;
                }
            }
            true
        }
        _ => {
            eprintln!(
                "MISMATCH at [{}] unmatched variants {} {}",
                path,
                variant_name(&a),
                variant_name(&b)
            );
            false
        }
    }
}

fn incremental_mode(artifact_path: &str, trace_path: &str, source_path: &str) {
    use parsanol::portable::incremental::{Edit, IncrementalParser};
    use std::time::Instant;

    let artifact_text = std::fs::read_to_string(artifact_path).expect("artifact");
    let artifact = parsanol::PargArtifact::from_json(&artifact_text).expect("artifact loads");
    let grammar = artifact.grammar("main").expect("the entry decodes");

    let trace: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(trace_path).expect("trace"))
            .expect("trace json");
    let mut text = std::fs::read_to_string(source_path).expect("source");
    let edits = trace["edits"].as_array().expect("edits").clone();

    let mut incr = IncrementalParser::new(&grammar);
    let _ = incr.parse_retained(&text).expect("initial parse");
    println!("0 0 identity_ok");

    let mut latencies: Vec<u128> = Vec::new();
    let mut fresh_latencies: Vec<u128> = Vec::new();
    let mut identity_failures = 0usize;
    let mut total_reused = 0usize;
    let mut total_invalidated = 0usize;
    for (i, e) in edits.iter().enumerate() {
        let pos = e["pos"].as_u64().expect("pos") as usize;
        let del = e["del"].as_u64().expect("del") as usize;
        let ins = e["text"].as_str().expect("text").to_string();
        let mut next = String::with_capacity(text.len() + ins.len());
        next.push_str(&text[..pos]);
        next.push_str(&ins);
        next.push_str(&text[pos + del..]);

        let t = Instant::now();
        let result = incr
            .parse_with_edit_retained(&next, Edit::new(pos, del, ins.len()))
            .expect("incremental parse");
        let ns = t.elapsed().as_nanos();
        latencies.push(ns);
        total_reused += result.reused_cache_entries;
        total_invalidated += result.invalidated_cache_entries;

        let incr_root = result.ast;
        let incr_arena_snapshot: &AstArena = incr.retained_arena();
        let mut scratch = AstArena::for_input(next.len());
        scratch.set_input(next.clone());
        let mut fresh = parsanol::portable::PortableParser::new(&grammar, &next, &mut scratch);
        let tf = Instant::now();
        let fresh_res = fresh.parse_with_end_pos().expect("fresh parse");
        let fresh_ns = tf.elapsed().as_nanos();
        fresh_latencies.push(fresh_ns);
        let ok = fresh_res.end_pos == next.len()
            && deep_eq_dbg(
                incr_root,
                incr_arena_snapshot,
                fresh_res.value,
                &scratch,
                "root",
                0,
            );
        if !ok {
            identity_failures += 1;
        }
        println!(
            "{} {} {}",
            i + 1,
            ns,
            if ok { "identity_ok" } else { "IDENTITY_FAIL" }
        );
        text = next;
    }

    latencies.sort();
    fresh_latencies.sort();
    let n = latencies.len();
    let p = |q: f64, v: &[u128]| v[((q * (v.len() - 1) as f64).round()) as usize];
    println!(
        "SUMMARY steps {} incr_p50 {} incr_p95 {} incr_max {} fresh_p50 {} fresh_p95 {} fresh_max {} identity_failures {} reused {} invalidated {} spliced {}",
        n,
        p(0.50, &latencies),
        p(0.95, &latencies),
        latencies[n - 1],
        p(0.50, &fresh_latencies),
        p(0.95, &fresh_latencies),
        fresh_latencies[n - 1],
        identity_failures,
        total_reused,
        total_invalidated,
        incr.spliced_parses(),
    );
}

/// TED scorer emission (rs#527): per corrupted case, the recovered
/// shape tree (`partial`) plus the clean document's shape tree — the
/// windowed tree-edit-distance inputs. One JSON line per case:
/// {"name":..., "recovered":..., "clean":...}
fn trees_mode(corpus_dir: &str, clean_dir: &str, manifest_path: &str, artifact_path: &str) {
    let artifact_text = std::fs::read_to_string(artifact_path).expect("artifact");
    let artifact = parsanol::PargArtifact::from_json(&artifact_text).expect("artifact loads");
    let manifest: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(manifest_path).expect("manifest"))
            .expect("manifest json");

    for case in manifest["cases"].as_array().expect("cases") {
        let name = case["file"].as_str().expect("file");
        let source = case["source"].as_str().expect("source");
        let recovered = std::fs::read_to_string(std::path::Path::new(corpus_dir).join(name))
            .expect("corrupted doc");
        let clean = std::fs::read_to_string(std::path::Path::new(clean_dir).join(source))
            .expect("clean doc");

        let recovered_tree = match artifact.parse_shape_recovering("main", &recovered) {
            Ok(envelope) => envelope["partial"].clone(),
            Err(_) => serde_json::Value::Null,
        };
        let clean_tree = match artifact.parse_shape("main", &clean) {
            Ok(shape) => shape,
            Err(_) => serde_json::Value::Null,
        };
        println!(
            "{}",
            serde_json::json!({
                "name": name,
                "recovered": recovered_tree,
                "clean": clean_tree,
            })
        );
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() == 5 && args[1] == "incremental" {
        incremental_mode(&args[2], &args[3], &args[4]);
        return;
    }
    if args.len() == 3 && args[1] == "recovery" {
        recovery_mode(&args[2]);
        return;
    }
    if args.len() == 6 && args[1] == "trees" {
        trees_mode(&args[2], &args[3], &args[4], &args[5]);
        return;
    }
    if args.len() != 6 {
        eprintln!(
            "usage: run_parsanol <artifact.json> <entry> <corpus-dir> <rounds> <vm|interpreter>"
        );
        std::process::exit(2);
    }
    let artifact_text = std::fs::read_to_string(&args[1]).expect("artifact");
    let entry = args[2].clone();
    let corpus_dir = std::path::Path::new(&args[3]);
    let rounds: u32 = args[4].parse().expect("rounds");
    let engine = args[5].clone();
    assert!(
        engine == "vm" || engine == "interpreter",
        "engine = vm|interpreter"
    );

    let artifact = parsanol::PargArtifact::from_json(&artifact_text).expect("artifact loads");
    let grammar = artifact.grammar(&entry).expect("the entry decodes");
    let program = compile(grammar.clone()).expect("the program compiles");

    let mut docs: Vec<(String, String)> = Vec::new();
    let mut total_bytes = 0usize;
    for entry in std::fs::read_dir(corpus_dir).expect("corpus dir") {
        let path = entry.expect("entry").path();
        if path.extension().is_some_and(|x| x == "json") {
            let input = std::fs::read_to_string(&path).expect("doc");
            total_bytes += input.len();
            docs.push((
                path.file_name().unwrap().to_string_lossy().into_owned(),
                input,
            ));
        }
    }
    docs.sort();
    assert!(!docs.is_empty(), "no corpus documents");

    // warmup: one full pass, verifying both engines fully consume
    for (_, input) in &docs {
        let mut arena = AstArena::for_input(input.len());
        let mut parser = PortableParser::new(&grammar, input, &mut arena);
        let end = parser
            .parse_with_end_pos()
            .unwrap_or_else(|e| panic!("the interpreter must parse every doc: {e:?}"));
        assert_eq!(
            end.end_pos,
            input.len(),
            "the interpreter must consume the doc"
        );
        let mut arena = AstArena::for_input(input.len());
        let (vm, _) = parse_with_vm_diag(&program, input, &mut arena);
        let vm = vm.expect("the VM must parse every doc");
        assert_eq!(vm.end_pos, input.len(), "the VM must consume the doc");
    }

    let mut total_ns = 0u128;
    for (name, input) in &docs {
        let mut best = u128::MAX;
        for _ in 0..rounds {
            let t = Instant::now();
            match engine.as_str() {
                "interpreter" => {
                    let mut arena = AstArena::for_input(input.len());
                    let mut parser = PortableParser::new(&grammar, input, &mut arena);
                    let end = black_box(parser.parse_with_end_pos()).expect("parse");
                    assert_eq!(end.end_pos, input.len());
                }
                _ => {
                    let mut arena = AstArena::for_input(input.len());
                    let (vm, _) = parse_with_vm_diag(&program, input, &mut arena);
                    let vm = black_box(vm).expect("parse");
                    assert_eq!(vm.end_pos, input.len());
                }
            }
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
