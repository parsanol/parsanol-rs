//! Replays a plurimath-testsuite stage generation's raw-tree rows on the
//! Rust engine (plurimath#540).
//!
//!   cargo run --example stage-replay -- <latex.json> <payload file or dir>
//!
//! Payloads are the generation's raw-*.yaml files converted to JSON
//! (raw-*.json), for example with
//! `ruby -ryaml -rjson -e 'puts JSON.generate(YAML.load_file(ARGV[0]))'`.
//!
//! The declared bindings are read from the artifact's sibling
//! `<name>.bindings.rs.json` when present. Exits 1 when any row differs.

mod replay;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() != 2 {
        eprintln!("usage: stage-replay <artifact.json> <payload file or dir>");
        std::process::exit(2);
    }
    let report = replay::replay_stage(args[0].as_ref(), args[1].as_ref());
    print!("{}", report.format());
    std::process::exit(if report.differ() == 0 { 0 } else { 1 });
}
