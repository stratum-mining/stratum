use std::{fs::File, io::Write};

use sv2_serde_json::{parser::JsonParser, value::Value};

fn write_value_to_file(value: &Value, file_path: &str) -> std::io::Result<()> {
    let json_bytes = value.to_json_bytes();
    let mut file = File::create(file_path)?;
    file.write_all(&json_bytes)?;
    Ok(())
}

fn main() {
    let file = File::open("./examples/test.json").unwrap();
    let parser = JsonParser::parse(file).unwrap();

    let _ = write_value_to_file(&parser, "./examples/result.json");

    dbg!(parser);
}
