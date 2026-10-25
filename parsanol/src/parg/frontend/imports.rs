//! Cross-file imports (PN 1): `use <name>` at the top of a .parg file
//! loads <name>.parg from an import directory, prefixes every rule
//! with "<name>." (capture names are preserved — embedded grammars
//! keep their capture contract), and merges entries, bindings, and
//! preprocess steps. Cycles and missing files are compile errors. The
//! artifact bakes the expansion: consumers never see imports.

use std::path::PathBuf;

use super::compile::CompileError;
use super::parser::{self, Document, Node};

/// Merge every `use` declaration's source into +document+, in
/// declaration order.
pub fn merge(document: &mut Document, import_dirs: &[PathBuf]) -> Result<(), CompileError> {
    merge_inner(document, import_dirs, &mut Vec::new())
}

fn merge_inner(
    document: &mut Document,
    import_dirs: &[PathBuf],
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
        let source = find_file(&name, import_dirs)?;
        let text = std::fs::read_to_string(&source).map_err(|error| CompileError {
            message: format!("import `use {name}` unreadable: {error}"),
        })?;
        let mut used = parser::parse(&text).map_err(|error| CompileError {
            message: format!("import `use {name}` failed to parse: {error}"),
        })?;
        merging.push(name.clone());
        merge_inner(&mut used, import_dirs, merging)?;
        merging.pop();
        merge_used(document, &used, &format!("{name}."))?;
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

fn merge_used(document: &mut Document, used: &Document, prefix: &str) -> Result<(), CompileError> {
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
        "ref" => Node {
            kind: "ref".to_string(),
            name: node.name.as_deref().map(|name| format!("{prefix}{name}")),
            ..Node::default()
        },
        _ => node.clone(),
    }
}
