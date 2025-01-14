// examples/json.rs

use std::fs::File;
use sv2_serde_json::parser::JsonParser;

fn main() {
    let file = File::open("./examples/test.json").unwrap();
    let parser = JsonParser::parse(file).unwrap();

    dbg!(parser);
}
