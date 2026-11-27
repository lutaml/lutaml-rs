//! Compiled inside the generated consumer: shared tiny XML hook fixture.
use super::contracts::{ContractError, ErrorReason};
use super::hooks::{Real, State};
use super::models::*;
use super::xml_codecs::*;
use super::xml_rt::moxml_binding::moxml::document::{ContentRow, Document, ElementRow, ParserPolicy};
use serde_json::{json, Value};

/// Namespace-aware canonical tree: attributes sorted, xmlns declarations dropped, whitespace-only text
/// between elements dropped, text never trimmed.
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
fn canon(xml: &str) -> Value {
    let doc = Document::parse_with(xml.as_bytes(), &ParserPolicy::default()).unwrap();
    tree(&doc.element_rows(), 0)
}
fn reason(e: &ContractError) -> (ErrorReason, Option<String>) {
    (e.reason.clone(), e.rule.clone())
}

const MIXED: &str = r#"<h:mixedHooks xmlns:h="urn:lml:xmlhooks"><title>T</title><h:extra>E</h:extra></h:mixedHooks>"#;

#[test]
fn element_hooks_read_declared_and_hook_only_values_and_write_them_back() {
    let mut state = State::default();
    let model = read_LmlXmlHookFixtures_Mixed_xml_with(MIXED.as_bytes(), &Real, &mut state).unwrap();
    assert_eq!(model.title, Presence::Value("T".to_string()));
    assert_eq!(state.extra.as_deref(), Some("E"));
    let out = write_LmlXmlHookFixtures_Mixed_xml_with(&model, &Real, &mut state).unwrap();
    assert_eq!(canon(&out), canon(MIXED), "{out}");
    // parse-write-parse identity
    let again = read_LmlXmlHookFixtures_Mixed_xml_with(out.as_bytes(), &Real, &mut State::default()).unwrap();
    assert_eq!(again, model);
}

#[test]
fn a_missing_handler_fails_custom_serialization_for_the_attempted_direction_only() {
    let error = read_LmlXmlHookFixtures_Mixed_xml(MIXED.as_bytes()).unwrap_err();
    assert_eq!(error.reason, ErrorReason::CustomSerialization);
    let mut model = LmlXmlHookFixtures_Mixed::default();
    model.title = Presence::Value("T".into());
    let error = write_LmlXmlHookFixtures_Mixed_xml(&model).unwrap_err();
    assert_eq!(reason(&error).0, ErrorReason::CustomSerialization);
    // nothing of the rule is written without its handler: never the ordinary value, never a silent drop
    // A document with no hook-owned rule present reads without any handler.
    read_LmlXmlHookFixtures_Mixed_xml(br#"<h:mixedHooks xmlns:h="urn:lml:xmlhooks"/>"#).unwrap();
    // A read never runs a write hook.
    let mut state = State::default();
    read_LmlXmlHookFixtures_Mixed_xml_with(MIXED.as_bytes(), &Real, &mut state).unwrap();
    assert_eq!(state.calls, ["title_from_xml", "extra_from_xml"]);
    let mut state = State::default();
    let model = read_LmlXmlHookFixtures_Mixed_xml_with(MIXED.as_bytes(), &Real, &mut State::default()).unwrap();
    write_LmlXmlHookFixtures_Mixed_xml_with(&model, &Real, &mut state).unwrap();
    assert_eq!(state.calls, ["title_to_xml", "extra_to_xml"]);
}

#[test]
fn raw_markup_is_inserted_unescaped_in_no_namespace() {
    let mut model = LmlXmlHookFixtures_SymbolLike::default();
    model.type_ = Presence::Value("HTML".into());
    model.content = Presence::Value("<i>m &amp; n</i>".into());
    let out = write_LmlXmlHookFixtures_SymbolLike_xml_with(&model, &Real, &mut State::default()).unwrap();
    let t = canon(&out);
    assert_eq!(t["name"], "UnitSymbol");
    assert_eq!(t["ns"], "urn:lml:xmlhooks");
    assert_eq!(t["kids"][0]["name"], "i");
    assert_eq!(t["kids"][0]["ns"], "", "plain markup stays in no namespace: {out}");
    assert_eq!(t["kids"][0]["kids"][0], "m & n");
    assert!(out.contains("m &amp; n") && !out.contains("&lt;i&gt;"), "{out}");
    // the source read hook is a no-op: parsed content is missing
    let read = read_LmlXmlHookFixtures_SymbolLike_xml_with(out.as_bytes(), &Real, &mut State::default()).unwrap();
    assert!(read.content.is_absent());
    assert_eq!(read.type_, Presence::Value("HTML".to_string()));
    // no handler: the write fails, it does not escape the markup as text
    let error = write_LmlXmlHookFixtures_SymbolLike_xml(&model).unwrap_err();
    assert_eq!(error.reason, ErrorReason::CustomSerialization);
}
