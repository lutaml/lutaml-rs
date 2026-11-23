//! Source parity for scalar content/element CDATA: every row of the shared Ruby answer set
//! (evidence/cdata-ruby-oracle.json, lutaml-model e4439b37, Nokogiri) is read and written by the generated
//! Rust codecs and compared. A source value that is an Array for a `:string` attribute cannot be held by a typed
//! scalar: those rows must be refused by name (`invalid_document`), and are listed in the report.
#![cfg(feature = "xml")]
mod support;
use serde_json::json;
use sha2::{Digest, Sha256};

const ORACLE: &str = include_str!("fixtures/cdata/oracle.json");

fn attr(name: &str) -> serde_json::Value {
    json!({"name": name, "type": {"kind":"builtin","name":"string"}, "required": false, "collection": false,
        "default": {"present": false}, "initialize_empty": false})
}
fn rule(kind: &str, name: Option<&str>, to: &str, cdata: bool) -> serde_json::Value {
    let mut r = json!({"kind": kind, "to": to, "namespace": {"mode":"inherit"}, "cdata": cdata,
        "render": {"default": false, "empty": "omit", "nil": "omit"}});
    if let Some(n) = name {
        r["name"] = json!([n]);
    }
    r
}
fn bundle() -> Vec<u8> {
    let doc =
        |name: &str, root: &str, attrs: Vec<serde_json::Value>, rules: Vec<serde_json::Value>| {
            (
                format!("{name}@default"),
                json!({"lml":2,"model":name,"register":"default","attributes":attrs,
            "mappings":{"xml":{"root":{"element":root},"rules":rules}}}),
            )
        };
    let models: serde_json::Map<String, serde_json::Value> = [
        doc(
            "OrdinaryContent",
            "r",
            vec![attr("text")],
            vec![rule("content", None, "text", false)],
        ),
        doc(
            "CdataContent",
            "r",
            vec![attr("text")],
            vec![rule("content", None, "text", true)],
        ),
        doc(
            "OrdinaryElement",
            "p",
            vec![attr("v")],
            vec![rule("element", Some("v"), "v", false)],
        ),
        doc(
            "CdataElement",
            "p",
            vec![attr("v")],
            vec![rule("element", Some("v"), "v", true)],
        ),
        doc(
            "ContentWithMappedChild",
            "r",
            vec![attr("text"), attr("c")],
            vec![
                rule("content", None, "text", false),
                rule("element", Some("c"), "c", false),
            ],
        ),
    ]
    .into_iter()
    .collect();
    serde_json::to_vec(
        &json!({"lml_bundle":1,"roots":[],"models":models,"value_types":{},"capabilities":["xml"]}),
    )
    .unwrap()
}

const CONSUMER: &str = r##"
use super::contracts::ErrorReason;
use super::models::*;
use super::xml_codecs::*;
use serde_json::{json, Value};

const ORACLE: &str = include_str!("oracle.json");

fn text(p: &Presence<String>) -> Option<String> {
    match p { Presence::Value(v) => Some(v.clone()), _ => None }
}
/// Ok(None) = the member is unset (source nil); Err = refused by name.
fn read(model: &str, xml: &str) -> Result<Option<String>, String> {
    let named = |e: Box<super::contracts::ContractError>| if e.reason == ErrorReason::InvalidDocument { "invalid_document".to_string() } else { format!("{:?}", e.reason) };
    match model {
        "OrdinaryContent" => read_OrdinaryContent_xml(xml.as_bytes()).map(|m| text(&m.text)).map_err(named),
        "CdataContent" => read_CdataContent_xml(xml.as_bytes()).map(|m| text(&m.text)).map_err(named),
        "OrdinaryElement" => read_OrdinaryElement_xml(xml.as_bytes()).map(|m| text(&m.v)).map_err(named),
        "CdataElement" => read_CdataElement_xml(xml.as_bytes()).map(|m| text(&m.v)).map_err(named),
        "ContentWithMappedChild" => read_ContentWithMappedChild_xml(xml.as_bytes()).map(|m| text(&m.text)).map_err(named),
        other => panic!("{other}"),
    }
}
fn write(model: &str, value: &str) -> String {
    let v = Presence::Value(value.to_string());
    match model {
        "OrdinaryContent" => { let mut m = OrdinaryContent::default(); m.text = v; write_OrdinaryContent_xml(&m).unwrap() }
        "CdataContent" => { let mut m = CdataContent::default(); m.text = v; write_CdataContent_xml(&m).unwrap() }
        "OrdinaryElement" => { let mut m = OrdinaryElement::default(); m.v = v; write_OrdinaryElement_xml(&m).unwrap() }
        "CdataElement" => { let mut m = CdataElement::default(); m.v = v; write_CdataElement_xml(&m).unwrap() }
        other => panic!("{other}"),
    }
}
/// The oracle is pretty-printed; Rust writes compact. Layout whitespace between tags is not data.
fn compact(xml: &str) -> String {
    let mut out = String::new();
    let mut rest = xml;
    while let Some(i) = rest.find('>') {
        out.push_str(&rest[..=i]);
        rest = &rest[i + 1..];
        if rest.trim_start().starts_with('<') { rest = rest.trim_start(); }
    }
    out.push_str(rest);
    out
}

#[test]
fn every_oracle_row_is_read_and_written_with_source_semantics() {
    let oracle: Value = serde_json::from_str(ORACLE).unwrap();
    let (mut read_pass, mut read_refused, mut read_mismatch) = (0, 0, 0);
    let (mut write_tree, mut write_bytes) = (0, 0);
    let mut rows = Vec::new();
    for row in oracle["reads"].as_array().unwrap() {
        let (model, case, xml) = (row["model"].as_str().unwrap(), row["case"].as_str().unwrap(), row["xml"].as_str().unwrap());
        let expected = &row["read"]["value"];
        let got = read(model, xml);
        let verdict = match (expected, &got) {
            (Value::String(s), Ok(Some(g))) if s == g => "pass",
            (Value::Null, Ok(None)) => "pass",
            (Value::Array(a), Ok(None)) if a.is_empty() => "pass",
            (Value::Array(a), Err(e)) if !a.is_empty() && e == "invalid_document" => "refused",
            _ => "mismatch",
        };
        match verdict { "pass" => read_pass += 1, "refused" => read_refused += 1, _ => read_mismatch += 1 }
        rows.push(json!({"model": model, "case": case, "source": expected, "rust": format!("{got:?}"), "verdict": verdict}));
    }
    for row in oracle["writes"].as_array().unwrap() {
        let (model, value) = (row["model"].as_str().unwrap(), row["value"].as_str().unwrap());
        let want = compact(row["write"]["value"].as_str().unwrap());
        let got = write(model, value);
        // The value must survive: reading what Rust wrote gives the value back (empty is omitted, as in source).
        let tree_ok = if value.is_empty() { read(model, &got) == read(model, &want) } else { read(model, &got) == read(model, &want) && read(model, &got).is_ok() };
        write_tree += tree_ok as u32;
        write_bytes += (got == want) as u32;
        rows.push(json!({"model": model, "value": value, "source": want, "rust": got, "tree": tree_ok, "bytes": got == want}));
    }
    println!("CDATA_REPORT {}", json!({"read_pass": read_pass, "read_refused_by_name": read_refused, "read_mismatch": read_mismatch,
        "write_rows": oracle["writes"].as_array().unwrap().len(), "write_semantic_equal": write_tree, "write_bytes_equal": write_bytes, "rows": rows}));
    assert_eq!(read_mismatch, 0);
    assert_eq!(write_tree, 20);
    assert_eq!(write_bytes, 20, "compact output equals the oracle's bytes for every write row");
    assert_eq!((read_pass, read_refused), (46, 9));
}
"##;

#[test]
fn the_shared_ruby_answer_set_is_the_acceptance_table() {
    let digest = format!("{:x}", Sha256::digest(ORACLE.as_bytes()));
    assert_eq!(
        digest,
        "b6f7d6201fab446a652fd2322d2446d2bcc21e7b90d1f014e323461c94734094"
    );
    let run = support::run_consumer(
        "xml-cdata-oracle-consumer",
        &bundle(),
        &[("pilot_tests.rs", CONSUMER), ("oracle.json", ORACLE)],
        "#[cfg(test)]\nmod pilot_tests;\n",
        "\"xml\"",
        &Default::default(),
    );
    assert!(run.stdout.contains("CDATA_REPORT"), "{}", run.stdout);
    assert!(run.stdout.contains("1 passed"), "{}", run.stdout);
}
