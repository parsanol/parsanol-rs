fn main() {
    let src_path = std::env::args().nth(1).expect("parg source");
    let out_path = std::env::args().nth(2).expect("output artifact");
    let source = std::fs::read_to_string(&src_path).expect("read source");
    let envelope = parsanol::parg::frontend::envelope::compile_source(&source).expect("compile");
    std::fs::write(&out_path, serde_json::to_string_pretty(&envelope).unwrap()).expect("write");
    println!("baked {out_path}");
}
