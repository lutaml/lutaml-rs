//! Host replay of the same generated codecs. Reads `[{op,set,model,input}]` on
//! stdin and prints one `{ok}` or `{err}` per op, the shape the wasm runner compares.
use mml_wasm::{read_xml_json, write_xml_json};
use serde_json::{json, Value};
use std::io::Read;

fn main() {
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input).unwrap();
    let ops: Vec<Value> = serde_json::from_str(&input).unwrap();
    let results: Vec<Value> = ops
        .iter()
        .map(|op| {
            let (set, model, text) = (
                op["set"].as_str().unwrap(),
                op["model"].as_str().unwrap(),
                op["input"].as_str().unwrap(),
            );
            let result = match op["op"].as_str().unwrap() {
                "read" => read_xml_json(set, model, text),
                "write" => write_xml_json(set, model, text),
                other => panic!("unknown op {other}"),
            };
            match result {
                Ok(text) => json!({"ok": text}),
                Err(e) => json!({"err": serde_json::to_value(&*e).unwrap()}),
            }
        })
        .collect();
    print!("{}", serde_json::to_string(&results).unwrap());
}
