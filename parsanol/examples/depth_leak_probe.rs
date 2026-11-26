fn main() {
    let grammar_text = r#"{"atoms":[
        {"Re":{"pattern":"[a-z]"}},
        {"Named":{"name":"term","atom":0}},
        {"Repetition":{"atom":1,"min":1,"max":null,"tag":"Repetition"}}
    ],"root":2}"#;
    let g = parsanol::portable::Grammar::from_json(grammar_text).unwrap();
    // The repeated parse: the depth leak = the later parses hit the limit.
    for round in 0..3 {
        let input = "abcdefghij";
        let mut arena = parsanol::portable::AstArena::for_input(input.len());
        arena.set_input(input.to_string());
        let mut p = parsanol::portable::PortableParser::new(&g, input, &mut arena);
        match p.parse() {
            Ok(_) => println!("round {}: ok", round),
            Err(e) => println!("round {}: ERR {:?}", round, e),
        }
    }
}
