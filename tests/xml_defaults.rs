//! XML default provenance: the same matrix as the key-value codecs, for marked and unmarked models and a
//! nested model. A default `apply_defaults` supplied is suppressed under `render default: false` and written
//! under `render default: true`; a value the caller provided (decoded, assigned, constructed) is always
//! written. Ruby: `new` -> default applied; `new(flag: false)` -> explicit (`using_default?` false).
#![cfg(feature = "xml")]
mod support;
use serde_json::json;

fn attr(name: &str, ty: serde_json::Value, default: serde_json::Value) -> serde_json::Value {
    json!({"name": name, "type": ty, "required": false, "collection": false, "default": {"present": true, "value": default}, "initialize_empty": false})
}
fn builtin(name: &str) -> serde_json::Value {
    json!({"kind": "builtin", "name": name})
}
fn rule(kind: &str, name: &str, to: &str, render_default: bool) -> serde_json::Value {
    json!({"kind": kind, "name": [name], "to": to, "namespace": {"mode": "inherit"}, "render": {"default": render_default, "empty": "omit", "nil": "omit"}})
}
/// `inner`: the nested model id, when the model has a child element.
fn model(
    name: &str,
    element: &str,
    marked: bool,
    render_default: bool,
    inner: Option<&str>,
) -> (String, serde_json::Value) {
    let mut attributes = vec![
        attr("flag", builtin("boolean"), json!(false)),
        attr("count", builtin("integer"), json!(0)),
        attr("tag", builtin("string"), json!("d")),
    ];
    let mut rules = vec![
        rule("attribute", "flag", "flag", render_default),
        rule("attribute", "count", "count", render_default),
        rule("element", "tag", "tag", render_default),
    ];
    if let Some(inner) = inner {
        attributes.push(json!({"name": "inner", "type": {"kind": "model", "id": format!("{inner}@default")}, "required": false, "collection": false, "default": {"present": false}, "initialize_empty": false}));
        rules.push(rule("element", "inner", "inner", render_default));
    }
    let mut doc = json!({"lml": 2, "model": name, "register": "default", "attributes": attributes,
        "mappings": {"xml": {"root": {"element": element}, "rules": rules}}});
    if marked {
        doc["custom_initialize"] = json!(true);
    }
    (format!("{name}@default"), doc)
}
fn bundle() -> Vec<u8> {
    let models: serde_json::Map<String, serde_json::Value> = [
        model("Inner", "inner", false, false, None),
        model("InnerLoud", "inner", false, true, None),
        model("Quiet", "q", false, false, Some("Inner")),
        model("QuietMarked", "q", true, false, Some("Inner")),
        model("Loud", "q", false, true, Some("InnerLoud")),
        model("LoudMarked", "q", true, true, Some("InnerLoud")),
    ]
    .into_iter()
    .collect();
    serde_json::to_vec(&json!({"lml_bundle": 1, "roots": ["Quiet@default"], "models": models, "value_types": {}, "capabilities": ["xml"]})).unwrap()
}

const CONSUMER: &str = r##"
use super::models::*;
use super::xml_codecs::*;
use super::xml_rt::moxml_binding::moxml::document::{ContentRow, Document, ElementRow, ParserPolicy};
use serde_json::{json, Value};

fn tree(rows: &[ElementRow], at: usize) -> Value {
    let r = &rows[at];
    let mut attrs: Vec<Value> = r.attributes.iter().map(|a| json!([a.name.local, a.value])).collect();
    attrs.sort_by_key(|a| a.to_string());
    let kids: Vec<Value> = r.content.iter().filter_map(|c| match c {
        ContentRow::Element(o) => Some(tree(rows, *o)),
        ContentRow::Text(t) if !t.trim().is_empty() => Some(json!(t)),
        _ => None,
    }).collect();
    json!({"name": r.name.local, "attrs": attrs, "kids": kids})
}
fn canon(xml: &str) -> Value {
    let doc = Document::parse_with(xml.as_bytes(), &ParserPolicy::default()).unwrap();
    tree(&doc.element_rows(), 0)
}

macro_rules! matrix {
    ($name:ident, $read:ident, $write:ident, $ty:ident, $inner:ident, $render_default:expr) => {
        #[test]
        fn $name() {
            let rd: bool = $render_default;
            let w = |m: &$ty| canon(&$write(m).unwrap());
            // Explicit values provided on decode are written, with or without apply_defaults (also nested).
            let explicit = r#"<q flag="false" count="0"><tag>d</tag><inner flag="false" count="0"><tag>d</tag></inner></q>"#;
            let mut m = $read(explicit.as_bytes()).unwrap();
            assert_eq!(w(&m), canon(explicit), "decoded explicit");
            m.apply_defaults().unwrap();
            if let Presence::Value(inner) = &mut m.inner { inner.apply_defaults().unwrap(); }
            assert_eq!(w(&m), canon(explicit), "explicit survives apply_defaults, nested too");
            // Omitted then applied: suppressed (render default false) or written (true).
            let mut m = $read(b"<q><inner/></q>").unwrap();
            m.apply_defaults().unwrap();
            if let Presence::Value(inner) = &mut m.inner { inner.apply_defaults().unwrap(); }
            let defaulted = r#"<q flag="false" count="0"><tag>d</tag><inner flag="false" count="0"><tag>d</tag></inner></q>"#;
            if rd { assert_eq!(w(&m), canon(defaulted), "applied default written"); } else { assert_eq!(w(&m), canon("<q><inner/></q>"), "applied default suppressed"); }
            // apply_defaults recurses into present nested models (their own provenance), never creating absent ones.
            let mut m = $read(b"<q><inner/></q>").unwrap();
            m.apply_defaults().unwrap();
            if rd { assert_eq!(w(&m), canon(defaulted), "nested defaults applied by recursion"); } else { assert_eq!(w(&m), canon("<q><inner/></q>")); }
            // apply_defaults is idempotent.
            let before = w(&m);
            m.apply_defaults().unwrap();
            assert_eq!(w(&m), before);
            // Never applied: absent stays absent.
            assert_eq!(w(&$read(b"<q/>").unwrap()), canon("<q/>"));
            // Constructed: an assigned default-equal value is explicit; an applied one is not.
            let mut built = $ty::default();
            built.flag = Presence::Value(false);
            built.apply_defaults().unwrap();
            let expected = if rd { r#"<q flag="false" count="0"><tag>d</tag></q>"# } else { r#"<q flag="false"/>"# };
            assert_eq!(w(&built), canon(expected), "constructed");
            // A changed applied value is written; null keeps its policy (omit); empty keeps its policy (omit).
            let mut changed = $ty::default();
            changed.apply_defaults().unwrap();
            changed.flag = Presence::Value(true);
            assert!(w(&changed)["attrs"].to_string().contains("true"));
            let mut nulled = $ty::default();
            nulled.tag = Presence::Null;
            assert_eq!(w(&nulled), canon("<q/>"));
            let mut empty = $ty::default();
            empty.tag = Presence::Value(String::new());
            assert_eq!(w(&empty), canon("<q/>"));
            // Provenance is not part of equality, and not of plain serde output.
            let mut twin = $read(b"<q/>").unwrap();
            twin.apply_defaults().unwrap();
            let mut explicit_twin = $read(b"<q/>").unwrap();
            explicit_twin.flag = Presence::Value(false);
            explicit_twin.count = Presence::Value(serde_json::from_str("\"0\"").unwrap());
            explicit_twin.tag = Presence::Value("d".into());
            assert_eq!(twin, explicit_twin);
            assert!(serde_json::to_value(&twin).unwrap().get("__lml_applied").is_none());
            let _: Option<$inner> = None;
        }
    };
}
matrix!(quiet, read_Quiet_xml, write_Quiet_xml, Quiet, Inner, false);
matrix!(quiet_marked, read_QuietMarked_xml, write_QuietMarked_xml, QuietMarked, Inner, false);
matrix!(loud, read_Loud_xml, write_Loud_xml, Loud, InnerLoud, true);
matrix!(loud_marked, read_LoudMarked_xml, write_LoudMarked_xml, LoudMarked, InnerLoud, true);
"##;

#[test]
fn xml_default_provenance_matrix_for_marked_and_unmarked_models() {
    let run = support::run_consumer(
        "xml-defaults-consumer",
        &bundle(),
        &[("pilot_tests.rs", CONSUMER)],
        "#[cfg(test)]\nmod pilot_tests;\n",
        "\"xml\"",
        &Default::default(),
    );
    assert!(run.stdout.contains("4 passed"), "{}", run.stdout);
}

/// A parent with NO defaults of its own: `apply_defaults` must reach the child (single and collection), keep
/// per-model provenance, and the root write must honor it (explicit equal values stay, applied ones follow
/// `render default`).
fn nested_bundle() -> Vec<u8> {
    let parent = |name: &str, inner: &str| {
        let attrs = vec![
            json!({"name": "inner", "type": {"kind": "model", "id": format!("{inner}@default")}, "required": false, "collection": false, "default": {"present": false}, "initialize_empty": false}),
            json!({"name": "items", "type": {"kind": "model", "id": format!("{inner}@default")}, "required": false, "collection": {"min": 0, "max": null}, "default": {"present": false}, "initialize_empty": false}),
            json!({"name": "absent_child", "type": {"kind": "model", "id": format!("{inner}@default")}, "required": false, "collection": false, "default": {"present": false}, "initialize_empty": false}),
        ];
        let rules = vec![
            rule("element", "inner", "inner", false),
            rule("element", "items", "items", false),
            rule("element", "absent_child", "absent_child", false),
        ];
        (
            format!("{name}@default"),
            json!({"lml": 2, "model": name, "register": "default", "attributes": attrs,
            "mappings": {"xml": {"root": {"element": "p"}, "rules": rules}}}),
        )
    };
    let inner = |name: &str, render_default: bool| {
        let (id, mut doc) = model(name, "i", false, render_default, None);
        doc["attributes"] = json!([attr("flag", builtin("boolean"), json!(false))]);
        doc["mappings"]["xml"]["rules"] =
            json!([rule("attribute", "flag", "flag", render_default)]);
        (id, doc)
    };
    let models: serde_json::Map<String, serde_json::Value> = [
        inner("Inner", false),
        inner("InnerLoud", true),
        parent("Parent", "Inner"),
        parent("ParentLoud", "InnerLoud"),
    ]
    .into_iter()
    .collect();
    serde_json::to_vec(&json!({"lml_bundle": 1, "roots": ["Parent@default"], "models": models, "value_types": {}, "capabilities": ["xml"]})).unwrap()
}

const NESTED: &str = r##"
use super::models::*;
use super::xml_codecs::*;
use super::xml_rt::moxml_binding::moxml::document::{ContentRow, Document, ElementRow, ParserPolicy};
use serde_json::{json, Value};
fn tree(rows: &[ElementRow], at: usize) -> Value {
    let r = &rows[at];
    let mut attrs: Vec<Value> = r.attributes.iter().map(|a| json!([a.name.local, a.value])).collect();
    attrs.sort_by_key(|a| a.to_string());
    let kids: Vec<Value> = r.content.iter().filter_map(|c| match c { ContentRow::Element(o) => Some(tree(rows, *o)), _ => None }).collect();
    json!({"name": r.name.local, "attrs": attrs, "kids": kids})
}
fn canon(xml: &str) -> Value { let d = Document::parse_with(xml.as_bytes(), &ParserPolicy::default()).unwrap(); tree(&d.element_rows(), 0) }

macro_rules! nested {
    ($name:ident, $read:ident, $write:ident, $render_default:expr) => {
        #[test]
        fn $name() {
            let rd: bool = $render_default;
            let mut m = $read(b"<p><inner/><items/><items flag=\"false\"/></p>").unwrap();
            m.apply_defaults().unwrap();
            // defaults reached the present child and each collection item; only omitted fields are marked
            let Presence::Value(inner) = &m.inner else { panic!("inner") };
            assert_eq!(inner.flag, Presence::Value(false));
            assert!(inner.__lml_applied.contains("flag"));
            let Presence::Value(items) = &m.items else { panic!("items") };
            assert!(items[0].__lml_applied.contains("flag"));
            assert!(!items[1].__lml_applied.contains("flag"), "an explicit equal value stays explicit");
            // absent children are never created
            assert!(m.absent_child.is_absent());
            let expected = if rd {
                r#"<p><inner flag="false"/><items flag="false"/><items flag="false"/></p>"#
            } else {
                r#"<p><inner/><items/><items flag="false"/></p>"#
            };
            assert_eq!(canon(&$write(&m).unwrap()), canon(expected));
            // idempotent
            let before = $write(&m).unwrap();
            m.apply_defaults().unwrap();
            assert_eq!(before, $write(&m).unwrap());
            // plain serde never shows provenance
            assert!(!serde_json::to_string(&m).unwrap().contains("__lml_applied"));
        }
    };
}
nested!(quiet_nested, read_Parent_xml, write_Parent_xml, false);
nested!(loud_nested, read_ParentLoud_xml, write_ParentLoud_xml, true);
"##;

#[test]
fn apply_defaults_reaches_nested_models_in_a_parent_without_defaults() {
    let run = support::run_consumer(
        "xml-nested-defaults-consumer",
        &nested_bundle(),
        &[("pilot_tests.rs", NESTED)],
        "#[cfg(test)]\nmod pilot_tests;\n",
        "\"xml\"",
        &Default::default(),
    );
    assert!(run.stdout.contains("2 passed"), "{}", run.stdout);
}
