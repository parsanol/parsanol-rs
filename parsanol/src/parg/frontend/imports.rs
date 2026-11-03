//! Cross-file imports (PN 1): `use <name>` at the top of a .parg file
//! loads `<name>`.parg from an import directory — or from an in-memory
//! name → source map, for consumers that embed their components
//! (expressir's core ext) — prefixes every rule with `"<name>."`
//! (capture names are preserved — embedded grammars keep their capture
//! contract), and merges entries, bindings, and preprocess steps.
//! Cycles and missing files are compile errors. The artifact bakes the
//! expansion: consumers never see imports.

use std::collections::BTreeMap;
use std::path::PathBuf;

use super::compile::CompileError;
use super::parser::{self, Document, Node};

/// Merge every `use` declaration's source into +document+, in
/// declaration order. Sources resolve as `<name>.parg` under
/// +import_dirs+.
pub fn merge(
    document: &mut Document,
    import_dirs: &[PathBuf],
    merged_atomic_rules: &mut Vec<String>,
) -> Result<BTreeMap<String, String>, CompileError> {
    merge_with(document, merged_atomic_rules, &|name| {
        let path = find_file(name, import_dirs)?;
        // The wire is LF: a CRLF checkout's imports normalize at the
        // read boundary, or their docs bake \r remnants.
        std::fs::read_to_string(&path)
            .map(|text| text.replace("\r\n", "\n"))
            .map_err(|error| CompileError {
                message: format!("import `use {name}` unreadable: {error}"),
            })
    })
}

/// Merge every `use` declaration's source into +document+, resolving
/// names from an in-memory map (the key is the `use` name, the value
/// the grammar source — no `.parg` suffix). Embedded-grammar consumers
/// (the expressir core ext) compile from `include_str!` assets and
/// have no directory to hand the resolver.
pub fn merge_sources(
    document: &mut Document,
    sources: &BTreeMap<String, String>,
    merged_atomic_rules: &mut Vec<String>,
) -> Result<BTreeMap<String, String>, CompileError> {
    merge_with(document, merged_atomic_rules, &|name| {
        sources
            .get(name)
            .cloned()
            .map(|text| text.replace("\r\n", "\n"))
            .ok_or_else(|| CompileError {
                message: format!("import `use {name}` not found in the embedded sources"),
            })
    })
}

fn merge_with(
    document: &mut Document,
    merged_atomic_rules: &mut Vec<String>,
    resolve: &dyn Fn(&str) -> Result<String, CompileError>,
) -> Result<BTreeMap<String, String>, CompileError> {
    let mut docs = BTreeMap::new();
    merge_inner(
        document,
        &mut docs,
        merged_atomic_rules,
        resolve,
        &mut Vec::new(),
    )?;
    Ok(docs)
}

fn merge_inner(
    document: &mut Document,
    docs: &mut BTreeMap<String, String>,
    merged_atomic_rules: &mut Vec<String>,
    resolve: &dyn Fn(&str) -> Result<String, CompileError>,
    merging: &mut Vec<String>,
) -> Result<(), CompileError> {
    if document.uses.is_empty() {
        return Ok(());
    }
    for name in document.uses.clone() {
        if merging.contains(&name) {
            return Err(CompileError {
                message: format!(
                    "import cycle: {}",
                    merging
                        .iter()
                        .chain(std::iter::once(&name))
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(" -> ")
                ),
            });
        }
        let text = resolve(&name)?;
        let parsed = parser::parse_grammar(&text).map_err(|error| CompileError {
            message: format!("import `use {name}` failed to parse: {error}"),
        })?;
        let (mut used, mut used_docs) = (parsed.document, parsed.docs);
        let mut used_atomic = parsed.atomic_rules.clone();
        merging.push(name.clone());
        merge_inner(
            &mut used,
            &mut used_docs,
            &mut used_atomic,
            resolve,
            merging,
        )?;
        merging.pop();
        let prefix = format!("{name}.");
        merge_used(document, docs, &used, &used_docs, &prefix)?;
        merged_atomic_rules.extend(used_atomic.iter().map(|rule| format!("{prefix}{rule}")));
    }
    document.uses.clear();
    Ok(())
}

fn find_file(name: &str, import_dirs: &[PathBuf]) -> Result<PathBuf, CompileError> {
    if import_dirs.is_empty() {
        return Err(CompileError {
            message: format!("no import directories configured for `use {name}`"),
        });
    }
    for dir in import_dirs {
        let candidate = dir.join(format!("{name}.parg"));
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    Err(CompileError {
        message: format!("import `use {name}` not found in {import_dirs:?}"),
    })
}

fn merge_used(
    document: &mut Document,
    docs: &mut BTreeMap<String, String>,
    used: &Document,
    used_docs: &BTreeMap<String, String>,
    prefix: &str,
) -> Result<(), CompileError> {
    for (name, node) in &used.rules {
        let qualified = format!("{prefix}{name}");
        if document
            .rules
            .iter()
            .any(|(existing, _)| *existing == qualified)
        {
            return Err(CompileError {
                message: format!("imported rule {qualified:?} collides"),
            });
        }
        document.rules.push((qualified, copy(node, prefix)));
    }
    for (entry, rule) in &used.entries {
        document
            .entries
            .push((format!("{prefix}{entry}"), format!("{prefix}{rule}")));
    }
    for (rule, list) in &used.bindings {
        document
            .bindings
            .insert(format!("{prefix}{rule}"), list.clone());
    }
    for (name, steps) in &used.preprocess {
        document
            .preprocess
            .insert(format!("{prefix}{name}"), steps.clone());
    }
    for (rule, doc) in used_docs {
        docs.insert(format!("{prefix}{rule}"), doc.clone());
    }
    // Tests re-key onto the prefixed entry when the used grammar has
    // exactly one (its tests implicitly rode it).
    let default_used_entry = if used.entries.len() == 1 {
        used.entries.first().map(|(entry, _)| entry.clone())
    } else {
        None
    };
    for mut test in used.tests.clone() {
        if test.entry.is_none() {
            if let Some(entry) = &default_used_entry {
                test.entry = Some(format!("{prefix}{entry}"));
            }
        } else {
            test.entry = test.entry.map(|entry| format!("{prefix}{entry}"));
        }
        document.tests.push(test);
    }
    Ok(())
}

fn copy(node: &Node, prefix: &str) -> Node {
    match node.kind.as_str() {
        "seq" => Node {
            kind: "seq".to_string(),
            items: Some(
                node.items
                    .as_deref()
                    .unwrap_or_default()
                    .iter()
                    .map(|child| copy(child, prefix))
                    .collect(),
            ),
            ..Node::default()
        },
        "alt" => Node {
            kind: "alt".to_string(),
            branches: Some(
                node.branches
                    .as_deref()
                    .unwrap_or_default()
                    .iter()
                    .map(|child| copy(child, prefix))
                    .collect(),
            ),
            ..Node::default()
        },
        "rep" => Node {
            kind: "rep".to_string(),
            child: Some(Box::new(copy(
                node.child.as_deref().expect("rep child"),
                prefix,
            ))),
            min: node.min,
            max: node.max,
            ..Node::default()
        },
        "opt" => Node {
            kind: "opt".to_string(),
            child: Some(Box::new(copy(
                node.child.as_deref().expect("opt child"),
                prefix,
            ))),
            ..Node::default()
        },
        "pred" => Node {
            kind: "pred".to_string(),
            positive: node.positive,
            behind: node.behind,
            child: Some(Box::new(copy(
                node.child.as_deref().expect("pred child"),
                prefix,
            ))),
            ..Node::default()
        },
        "cap" => Node {
            kind: "cap".to_string(),
            name: node.name.clone(),
            child: Some(Box::new(copy(
                node.child.as_deref().expect("cap child"),
                prefix,
            ))),
            ..Node::default()
        },
        // A qualified reference (`other.rule`) already names its own
        // imported grammar — prefixing would double-qualify it
        // (cross-import references compose: the other grammar's rules
        // are prefixed by their own merge).
        "ref" => Node {
            kind: "ref".to_string(),
            name: node.name.as_deref().map(|name| {
                if name.contains('.') {
                    name.to_string()
                } else {
                    format!("{prefix}{name}")
                }
            }),
            ..Node::default()
        },
        _ => node.clone(),
    }
}
