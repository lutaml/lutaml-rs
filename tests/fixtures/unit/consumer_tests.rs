//! Compiled inside the generated consumer: the ACTUAL Unit graph (11 models, only `Units::Symbol` has XML
//! hooks) and the 13 actual source cases (Ruby oracle: Nokogiri adapter, ee09d6d7). Hooks are handwritten
//! (`hooks.rs`); expected values are the frozen oracle fields, never recomputed here.
use super::hooks::UnitHooks;
use super::models::*;
use super::xml_codecs::*;
use super::xml_rt::moxml_binding::moxml::document::{ContentRow, Document, ElementRow, ParserPolicy};
use serde_json::{json, Value};

const CASES: &str = include_str!("cases.json");

fn tree(rows: &[ElementRow], at: usize) -> Value {
    let r = &rows[at];
    let mut attrs: Vec<Value> = r.attributes.iter().map(|a| json!([a.name.namespace.as_deref().unwrap_or(""), a.name.local, a.value])).collect();
    attrs.sort_by_key(|a| a.to_string());
    let mut kids = Vec::new();
    for c in &r.content {
        match c {
            ContentRow::Element(o) => kids.push(tree(rows, *o)),
            ContentRow::Text(t) | ContentRow::CData(t) if !t.trim().is_empty() => kids.push(json!(t)),
            _ => {}
        }
    }
    json!({"ns": r.name.namespace.as_deref().unwrap_or(""), "name": r.name.local, "attrs": attrs, "kids": kids})
}
/// Namespace-aware tree: (namespace, local), attributes sorted, xmlns declarations dropped, whitespace-only
/// text between elements dropped, text never trimmed.
fn canon(xml: &str) -> Value {
    let doc = Document::parse_with(xml.as_bytes(), &ParserPolicy::default()).unwrap();
    tree(&doc.element_rows(), 0)
}

/// Ruby builds nested models through their constructors, so their defaults are applied; the typed models
/// apply their own defaults, so the consumer applies them down the tree it built.
fn read(model: &str, input: &str) -> Result<Value, String> {
    match model {
        "Unitsml::Model::Unit" => read_Unitsml_Model_Unit_xml_with(input.as_bytes(), &UnitHooks, &mut ()).map(|m| serde_json::to_value(&m).unwrap()).map_err(|e| format!("{e:?}")),
        "Unitsml::Model::Units::Float64ConversionFrom" => read_Unitsml_Model_Units_Float64ConversionFrom_xml_with(input.as_bytes(), &UnitHooks, &mut ()).map(|m| serde_json::to_value(&m).unwrap()).map_err(|e| format!("{e:?}")),
        "Unitsml::Model::Units::Name" => read_Unitsml_Model_Units_Name_xml_with(input.as_bytes(), &UnitHooks, &mut ()).map(|m| serde_json::to_value(&m).unwrap()).map_err(|e| format!("{e:?}")),
        "Unitsml::Model::Units::RootUnits" => read_Unitsml_Model_Units_RootUnits_xml_with(input.as_bytes(), &UnitHooks, &mut ()).map(|m| serde_json::to_value(&m).unwrap()).map_err(|e| format!("{e:?}")),
        "Unitsml::Model::Units::Symbol" => read_Unitsml_Model_Units_Symbol_xml_with(input.as_bytes(), &UnitHooks, &mut ()).map(|m| serde_json::to_value(&m).unwrap()).map_err(|e| format!("{e:?}")),
        "Unitsml::Model::Units::System" => read_Unitsml_Model_Units_System_xml_with(input.as_bytes(), &UnitHooks, &mut ()).map(|m| serde_json::to_value(&m).unwrap()).map_err(|e| format!("{e:?}")),
        other => Err(format!("unknown model {other}")),
    }
}

/// Builds the model from the oracle's portable JSON, applies defaults (Ruby constructor behavior) and writes.
fn construct_and_write(model: &str, portable: &Value) -> Result<String, String> {
    macro_rules! build {
        ($ty:ty, $write:ident, $apply:expr) => {{
            let mut m: $ty = serde_json::from_value(portable.clone()).map_err(|e| e.to_string())?;
            $apply(&mut m);
            $write(&m, &UnitHooks, &mut ()).map_err(|e| format!("{e:?}"))
        }};
    }
    match model {
        "Unitsml::Model::Unit" => build!(Unitsml_Model_Unit, write_Unitsml_Model_Unit_xml_with, |m: &mut Unitsml_Model_Unit| {
            m.apply_defaults().unwrap();
            if let Presence::Value(name) = &mut m.name {
                name.apply_defaults().unwrap();
            }
            if let Presence::Value(systems) = &mut m.system {
                for s in systems {
                    s.apply_defaults().unwrap();
                }
            }
        }),
        "Unitsml::Model::Units::Name" => build!(Unitsml_Model_Units_Name, write_Unitsml_Model_Units_Name_xml_with, |m: &mut Unitsml_Model_Units_Name| m.apply_defaults().unwrap()),
        "Unitsml::Model::Units::System" => build!(Unitsml_Model_Units_System, write_Unitsml_Model_Units_System_xml_with, |m: &mut Unitsml_Model_Units_System| m.apply_defaults().unwrap()),
        "Unitsml::Model::Units::Symbol" => build!(Unitsml_Model_Units_Symbol, write_Unitsml_Model_Units_Symbol_xml_with, |_: &mut Unitsml_Model_Units_Symbol| ()),
        "Unitsml::Model::Units::Float64ConversionFrom" => build!(Unitsml_Model_Units_Float64ConversionFrom, write_Unitsml_Model_Units_Float64ConversionFrom_xml_with, |_: &mut Unitsml_Model_Units_Float64ConversionFrom| ()),
        "Unitsml::Model::Units::RootUnits" => build!(Unitsml_Model_Units_RootUnits, write_Unitsml_Model_Units_RootUnits_xml_with, |_: &mut Unitsml_Model_Units_RootUnits| ()),
        other => Err(format!("unknown model {other}")),
    }
}

/// parse -> write -> parse: the written XML of a read model, as text.
fn rewrite(model: &str, input: &str) -> Result<String, String> {
    macro_rules! rt {
        ($read:ident, $write:ident) => {{
            let m = $read(input.as_bytes(), &UnitHooks, &mut ()).map_err(|e| format!("{e:?}"))?;
            $write(&m, &UnitHooks, &mut ()).map_err(|e| format!("{e:?}"))
        }};
    }
    match model {
        "Unitsml::Model::Unit" => rt!(read_Unitsml_Model_Unit_xml_with, write_Unitsml_Model_Unit_xml_with),
        "Unitsml::Model::Units::Name" => rt!(read_Unitsml_Model_Units_Name_xml_with, write_Unitsml_Model_Units_Name_xml_with),
        "Unitsml::Model::Units::System" => rt!(read_Unitsml_Model_Units_System_xml_with, write_Unitsml_Model_Units_System_xml_with),
        "Unitsml::Model::Units::Symbol" => rt!(read_Unitsml_Model_Units_Symbol_xml_with, write_Unitsml_Model_Units_Symbol_xml_with),
        "Unitsml::Model::Units::Float64ConversionFrom" => rt!(read_Unitsml_Model_Units_Float64ConversionFrom_xml_with, write_Unitsml_Model_Units_Float64ConversionFrom_xml_with),
        "Unitsml::Model::Units::RootUnits" => rt!(read_Unitsml_Model_Units_RootUnits_xml_with, write_Unitsml_Model_Units_RootUnits_xml_with),
        other => Err(format!("unknown model {other}")),
    }
}

#[test]
fn the_thirteen_actual_unit_cases() {
    let fixture: Value = serde_json::from_str(CASES).unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 13);
    let mut rows = Vec::new();
    let (mut construct_tree, mut construct_bytes, mut read_data, mut rewrite_tree, mut rewrite_bytes, mut identity) = (0, 0, 0, 0, 0, 0);
    let (mut pass, mut refused, mut mismatch) = (0, 0, 0);
    for case in cases {
        let id = case["id"].as_str().unwrap();
        let model = case["model"].as_str().unwrap();
        let input = case.get("input_xml").or(case.get("ruby_xml")).unwrap().as_str().unwrap();
        let mut row = json!({"id": id, "model": model});
        let mut ok = true;
        // read: the oracle input parses to the oracle's parsed data
        match read(model, input) {
            Ok(data) => {
                let same = data == case["parsed_portable_json"];
                read_data += same as u32;
                ok &= same;
                row["read_data_equal"] = json!(same);
            }
            Err(e) => {
                refused += 1;
                row["refused"] = json!(e);
                rows.push(row);
                continue;
            }
        }
        // construction -> write (only cases that carry portable_json)
        if let Some(portable) = case.get("portable_json") {
            match construct_and_write(model, portable) {
                Ok(xml) => {
                    let tree_eq = canon(&xml) == canon(case["ruby_xml"].as_str().unwrap());
                    let byte_eq = xml == case["ruby_xml"].as_str().unwrap();
                    construct_tree += tree_eq as u32;
                    construct_bytes += byte_eq as u32;
                    ok &= tree_eq;
                    row["construct_tree_equal"] = json!(tree_eq);
                    row["construct_bytes_equal"] = json!(byte_eq);
                }
                Err(e) => {
                    ok = false;
                    row["construct_error"] = json!(e);
                }
            }
        }
        // reserialization of the parsed model, and parse-write-parse identity
        match rewrite(model, input) {
            Ok(xml) => {
                let want = case["reserialized_xml"].as_str().unwrap();
                let tree_eq = canon(&xml) == canon(want);
                rewrite_tree += tree_eq as u32;
                rewrite_bytes += (xml == want) as u32;
                let again = read(model, &xml).ok();
                let first = read(model, input).ok();
                let same = again.is_some() && again == first;
                identity += same as u32;
                ok &= tree_eq && same;
                row["reserialized_tree_equal"] = json!(tree_eq);
                row["reserialized_bytes_equal"] = json!(xml == want);
                row["parse_write_parse_identity"] = json!(same);
            }
            Err(e) => {
                ok = false;
                row["rewrite_error"] = json!(e);
            }
        }
        if ok {
            pass += 1;
        } else {
            mismatch += 1;
        }
        row["verdict"] = json!(if ok { "pass" } else { "mismatch" });
        rows.push(row);
    }
    let summary = json!({"cases": cases.len(), "pass": pass, "refused": refused, "mismatch": mismatch,
        "read_data_equal": read_data, "construct_tree_equal": construct_tree, "construct_bytes_equal": construct_bytes,
        "reserialized_tree_equal": rewrite_tree, "reserialized_bytes_equal": rewrite_bytes, "identity": identity});
    println!("UNIT_REPORT {}", serde_json::to_string(&json!({"summary": summary, "rows": rows})).unwrap());
    assert_eq!((pass, refused, mismatch), (13, 0, 0), "{summary}");
}

#[test]
fn the_symbol_raw_markup_is_inserted_unescaped_and_a_missing_handler_fails_by_name() {
    let fixture: Value = serde_json::from_str(CASES).unwrap();
    assert_eq!(fixture["symbol_raw_markup_checks"], json!({"markup_not_escaped": true, "ampersand_preserved": true}));
    let mut symbol = Unitsml_Model_Units_Symbol::default();
    symbol.type_ = Presence::Value("HTML".into());
    symbol.content = Presence::Value("<i>m &amp; n</i>".into());
    let out = write_Unitsml_Model_Units_Symbol_xml_with(&symbol, &UnitHooks, &mut ()).unwrap();
    // Tree-level (bytes differ by design: the writer prefixes the Unit namespace instead of a default xmlns).
    let t = canon(&out);
    assert_eq!(t["kids"][0]["name"], "i", "{out}");
    assert_eq!(t["kids"][0]["ns"], "", "plain markup stays in no namespace: {out}");
    assert_eq!(t["kids"][0]["kids"][0], "m & n");
    assert!(out.contains("m &amp; n") && !out.contains("&lt;"), "{out}");
    let error = write_Unitsml_Model_Units_Symbol_xml(&symbol).unwrap_err();
    assert_eq!(error.reason, super::contracts::ErrorReason::CustomSerialization);
    let error = read_Unitsml_Model_Units_Symbol_xml(br#"<UnitSymbol xmlns="https://schema.unitsml.org/unitsml/1.0" type="HTML">m</UnitSymbol>"#).unwrap_err();
    assert_eq!(error.reason, super::contracts::ErrorReason::CustomSerialization);
}
