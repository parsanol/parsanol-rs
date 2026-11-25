//! Depth-ladder probe for the stack-bound work (P4): parses
//! right-recursive nesting at a given depth and reports the outcome.
//! Run at increasing depths in separate processes (a stack overflow
//! aborts); the deepest OK depth gives the per-level frame cost
//! against the worker's 64 MiB stack.
//!
//! Run with: cargo run --release --example stack_depth_probe -- 400

use parsanol::parg::frontend::envelope;
use parsanol::parg::PargArtifact;
use parsanol::portable::parser::{FailureReporter, ParserCompatibility};

fn load() -> PargArtifact {
    let source = "grammar Budget version \"1.0.0\" edition \"v2\" {\n  main = \"(\" main \")\" / \"x\"\n  entry main: main\n}\n";
    let env = envelope::compile_source(source).expect("grammar compiles");
    let text = serde_json::to_string(&env).expect("envelope serializes");
    PargArtifact::from_json(&text).expect("artifact loads")
}

fn main() {
    let depth: usize = std::env::args()
        .nth(1)
        .and_then(|d| d.parse().ok())
        .expect("usage: stack_depth_probe <depth>");
    let artifact = load();
    let input = format!("{}x{}", "(".repeat(depth), ")".repeat(depth));
    match artifact.parse_shape_with_call_depth(
        "main",
        &input,
        ParserCompatibility::Strict,
        FailureReporter::Default,
        None,
    ) {
        Ok(_) => println!("OK depth {depth}"),
        Err(e) => println!("REFUSED depth {depth}: {e}"),
    }
}
