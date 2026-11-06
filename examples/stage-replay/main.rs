//! Replays a plurimath-testsuite stage generation's raw-tree rows on the
//! Rust engine (plurimath#540).
//!
//!   cargo run --example stage-replay -- <artifact.json> <payload file or dir> [--language NAME]
//!
//! NAME is latex (default), asciimath or unicode (languages.rs). Payloads
//! are the generation's *.yaml files converted to JSON (*.json), for
//! example with
//! `ruby -ryaml -rjson -e 'puts JSON.generate(YAML.load_file(ARGV[0]))'`;
//! only the raw-tree ones are replayed.
//!
//! For a language whose gem applies its bindings, they are read from the
//! artifact's sibling `<name>.bindings.rs.json` when present; unicode reads
//! its wrapper table from `../grammars/<name>.wrappers.json`. Exits 1 when
//! any row differs.

mod languages;
mod replay;

fn main() {
    let mut language = "latex".to_string();
    let mut args: Vec<String> = Vec::new();
    let mut argv = std::env::args().skip(1);
    while let Some(arg) = argv.next() {
        if arg == "--language" {
            language = argv.next().unwrap_or_default();
        } else {
            args.push(arg);
        }
    }
    if args.len() != 2 {
        eprintln!("usage: stage-replay <artifact.json> <payload file or dir> [--language NAME]");
        std::process::exit(2);
    }
    let report = replay::replay_stage(args[0].as_ref(), args[1].as_ref(), &language);
    print!("{}", report.format());
    std::process::exit(if report.differ() == 0 { 0 } else { 1 });
}
