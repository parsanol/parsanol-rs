fn main() {
    let source = std::fs::read_to_string("/tmp/p103/seq_loops.parg").unwrap();
    let env = parsanol::parg::frontend::envelope::compile_source(&source).expect("compiles");
    let text = serde_json::to_string(&env).unwrap();
    std::fs::write("/tmp/p103/seq_loops.json", &text).unwrap();
    let a = parsanol::parg::PargArtifact::from_json(&text).unwrap();
    println!("parse ok: {}", a.parse_shape("doc", "faxx8819fbb").is_ok());
}
