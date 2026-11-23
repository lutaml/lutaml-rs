//! XML hook structure and order: READ hooks of element rules receive the parsed element (name, namespace,
//! attributes, children, text) through a borrowed typed view; WRITE hooks run in declared rule order among
//! ordinary element/content output; attributes written by a hook land in the start tag.
#![cfg(feature = "xml")]
mod support;
use serde_json::json;

fn string_attr(name: &str, collection: bool) -> serde_json::Value {
    json!({"name": name, "type": {"kind":"builtin","name":"string"}, "required": false,
        "collection": if collection { json!({"min":0,"max":null}) } else { json!(false) },
        "default": {"present": false}, "initialize_empty": false})
}
fn render() -> serde_json::Value {
    json!({"default": false, "empty": "omit", "nil": "omit"})
}
fn rule(
    kind: &str,
    name: Option<&str>,
    to: Option<&str>,
    custom: Option<serde_json::Value>,
) -> serde_json::Value {
    let mut r = json!({"kind": kind, "namespace": {"mode": "inherit"}, "render": render()});
    if let Some(n) = name {
        r["name"] = json!([n]);
    }
    if let Some(t) = to {
        r["to"] = json!(t);
    }
    if let Some(c) = custom {
        r["custom"] = c;
    }
    r
}
fn bundle(models: Vec<(&str, &str, Vec<serde_json::Value>, Vec<serde_json::Value>)>) -> Vec<u8> {
    let docs: serde_json::Map<String, serde_json::Value> = models
        .into_iter()
        .map(|(name, root, attrs, rules)| {
            (
                format!("{name}@default"),
                json!({"lml":2,"model":name,"register":"default","attributes":attrs,
                    "mappings":{"xml":{"root":{"element":root},"namespace":{"mode":"uri","uri":"urn:t","prefix":"t"},"rules":rules}}}),
            )
        })
        .collect();
    serde_json::to_vec(
        &json!({"lml_bundle":1,"roots":[],"models":docs,"value_types":{},"capabilities":["xml"]}),
    )
    .unwrap()
}

const COMMON: &str = r##"
use super::contracts::{ContractResult, ErrorReason};
use super::models::*;
use super::xml_codecs::*;
use super::xml_rt::moxml_binding::moxml::document::{ContentRow, Document, ElementRow, ElementSpec, ParserPolicy, QName};
use super::xml_rt::moxml_binding::{XmlElement, XmlParent};
use serde_json::{json, Value};

fn tree(rows: &[ElementRow], at: usize) -> Value {
    let r = &rows[at];
    let mut attrs: Vec<Value> = r.attributes.iter().map(|a| json!([a.name.namespace.as_deref().unwrap_or(""), a.name.local, a.value])).collect();
    attrs.sort_by_key(|a| a.to_string());
    let kids: Vec<Value> = r.content.iter().filter_map(|c| match c {
        ContentRow::Element(o) => Some(tree(rows, *o)),
        ContentRow::Text(t) | ContentRow::CData(t) if !t.trim().is_empty() => Some(json!(t)),
        _ => None,
    }).collect();
    json!({"ns": r.name.namespace.as_deref().unwrap_or(""), "name": r.name.local, "attrs": attrs, "kids": kids})
}
fn canon(xml: &str) -> Value {
    let doc = Document::parse_with(xml.as_bytes(), &ParserPolicy::default()).unwrap();
    tree(&doc.element_rows(), 0)
}
"##;

const STRUCTURE: &str = r##"
#[derive(Default, Debug)]
pub struct State { pub seen: Vec<Value> }
pub struct Hooks;
impl CatalogXmlHooks<State> for Hooks {
    fn entry_from_xml(&self, _model: &mut Catalog, entry: &XmlElement<'_>, state: &mut State) -> ContractResult<()> {
        let parts: Vec<Value> = entry.children().iter().map(|c| json!({
            "name": c.local_name(), "ns": c.namespace(), "id": c.attribute("id", None), "text": c.text(),
            "nested": c.child("deep", None).map(|d| d.attribute("v", None).map(str::to_string)),
        })).collect();
        state.seen.push(json!({
            "name": entry.local_name(), "ns": entry.namespace(), "prefix": entry.prefix(),
            "kind": entry.attribute("kind", None), "text": entry.text(), "parts": parts,
            "attrs": entry.attributes().len(),
            "decls": entry.namespace_declarations().iter().map(|(p, u)| json!([p, u])).collect::<Vec<_>>(),
        }));
        Ok(())
    }
}
#[test]
fn element_read_hooks_see_attributes_children_and_namespaces() {
    // two entries with EQUAL text but different attributes and structure
    let xml = r#"<t:catalog xmlns:t="urn:t"><t:entry kind="a" other="1">same<part id="p1">x<deep v="9"/></part></t:entry><t:entry kind="b">same<part id="p2">y</part></t:entry></t:catalog>"#;
    let mut state = State::default();
    read_Catalog_xml_with(xml.as_bytes(), &Hooks, &mut state).unwrap();
    assert_eq!(state.seen.len(), 2);
    assert_eq!(state.seen[0]["text"], state.seen[1]["text"], "equal flattened text");
    assert_ne!(state.seen[0], state.seen[1], "distinct handler results");
    assert_eq!(state.seen[0]["kind"], "a");
    assert_eq!(state.seen[1]["kind"], "b");
    assert_eq!(state.seen[0]["attrs"], 2);
    assert_eq!(state.seen[0]["name"], "entry");
    assert_eq!(state.seen[0]["ns"], "urn:t");
    assert_eq!(state.seen[0]["prefix"], "t");
    assert_eq!(state.seen[0]["parts"][0]["id"], "p1");
    assert_eq!(state.seen[0]["parts"][0]["nested"], json!("9"));
    assert_eq!(state.seen[1]["parts"][0]["id"], "p2");
    assert_eq!(state.seen[1]["parts"][0]["nested"], Value::Null);
    // the namespace declaration lives on the root, not the entry; the child's namespace is none
    assert_eq!(state.seen[0]["decls"], json!([]));
    assert_eq!(state.seen[0]["parts"][0]["ns"], Value::Null);
}
#[test]
fn a_missing_element_read_handler_fails_custom_serialization_only_when_the_rule_is_reached() {
    let xml = r#"<t:catalog xmlns:t="urn:t"><t:entry kind="a"/></t:catalog>"#;
    assert_eq!(read_Catalog_xml(xml.as_bytes()).unwrap_err().reason, ErrorReason::CustomSerialization);
    read_Catalog_xml(br#"<t:catalog xmlns:t="urn:t"/>"#).unwrap();
}
"##;

const LIFETIME: &str = r##"
use super::contracts::ContractResult;
use super::models::*;
use super::xml_codecs::*;
use super::xml_rt::moxml_binding::XmlElement;
pub struct State { pub kept: Option<XmlElement<'static>> }
pub struct Hooks;
impl CatalogXmlHooks<State> for Hooks {
    fn entry_from_xml(&self, _model: &mut Catalog, entry: &XmlElement<'_>, state: &mut State) -> ContractResult<()> {
        state.kept = Some(*entry); // the view borrows the parsed document: it cannot outlive the call
        Ok(())
    }
}
"##;

const ORDER: &str = r##"
#[derive(Default)]
pub struct State { pub order: Vec<&'static str> }
pub struct Hooks;
impl SequenceXmlHooks<State> for Hooks {
    fn mid_to_xml(&self, model: &Sequence, parent: &mut XmlParent, state: &mut State) -> ContractResult<()> {
        state.order.push("mid");
        if let Presence::Value(_) = &model.before {
            // raw markup (entities and Unicode preserved) and a namespaced element built by hand
            parent.add_xml_fragment("<raw a=\"1\">t &amp; u é 日本</raw>")?;
            parent.add_element(ElementSpec::new(QName::ns(Some("x"), "ext", "urn:x")).text("n"));
        }
        Ok(())
    }
    fn flag_to_xml(&self, model: &Sequence, parent: &mut XmlParent, _state: &mut State) -> ContractResult<()> {
        if let Presence::Value(_) = &model.after {
            parent.add_attribute(QName::local("flag"), "yes");
        }
        Ok(())
    }
    fn between_to_xml(&self, _model: &Sequence, parent: &mut XmlParent, _state: &mut State) -> ContractResult<()> {
        parent.add_text("T");
        Ok(())
    }
}
#[test]
fn hook_output_runs_in_declared_rule_order_among_ordinary_output() {
    let mut m = Sequence::default();
    m.before = Presence::Value("B".into());
    m.after = Presence::Value("A".into());
    let mut state = State::default();
    let out = write_Sequence_xml_with(&m, &Hooks, &mut state).unwrap();
    let t = canon(&out);
    let names: Vec<_> = t["kids"].as_array().unwrap().iter().map(|k| k["name"].as_str().map(str::to_string).unwrap_or_else(|| k.to_string())).collect();
    // ordinary `before`, hook element (raw + ext), hook text between rules, ordinary `after`
    assert_eq!(names, ["before", "raw", "ext", "\"T\"", "after"], "{out}");
    // the attribute rule's hook output is in the start tag, not in the content
    assert_eq!(t["attrs"], json!([["", "flag", "yes"]]), "{out}");
    // raw text, entities and Unicode survive; the extension element keeps its namespace; plain markup stays in no namespace
    assert_eq!(t["kids"][1]["ns"], "");
    assert_eq!(t["kids"][1]["kids"][0], "t & u é 日本");
    assert_eq!(t["kids"][1]["attrs"], json!([["", "a", "1"]]));
    assert_eq!(t["kids"][2]["ns"], "urn:x");
    assert!(out.contains("t &amp; u é 日本"), "{out}");
    assert_eq!(state.order, ["mid"]);
    // a missing write handler never falls back to the ordinary value or drops the rule
    assert_eq!(write_Sequence_xml(&m).unwrap_err().reason, ErrorReason::CustomSerialization);
}
"##;

fn structure_bundle() -> Vec<u8> {
    bundle(vec![(
        "Catalog",
        "catalog",
        vec![],
        vec![rule(
            "element",
            Some("entry"),
            None,
            Some(json!({"from": "entry_from_xml"})),
        )],
    )])
}
fn order_bundle() -> Vec<u8> {
    bundle(vec![(
        "Sequence",
        "seq",
        vec![string_attr("before", false), string_attr("after", false)],
        vec![
            rule("element", Some("before"), Some("before"), None),
            rule(
                "element",
                Some("mid"),
                None,
                Some(json!({"to": "mid_to_xml"})),
            ),
            rule("content", None, None, Some(json!({"to": "between_to_xml"}))),
            rule("element", Some("after"), Some("after"), None),
            rule(
                "attribute",
                Some("flag"),
                None,
                Some(json!({"to": "flag_to_xml"})),
            ),
        ],
    )])
}

#[test]
fn element_read_hooks_receive_the_parsed_element() {
    let run = support::run_consumer(
        "xml-structure-consumer",
        &structure_bundle(),
        &[("pilot_tests.rs", &format!("{COMMON}{STRUCTURE}"))],
        "#[cfg(test)]\nmod pilot_tests;\n",
        "\"xml\"",
        &Default::default(),
    );
    assert!(run.stdout.contains("2 passed"), "{}", run.stdout);
}

#[test]
fn a_read_view_cannot_outlive_the_call() {
    let out = support::consumer_output(
        "xml-lifetime-consumer",
        &structure_bundle(),
        &[("pilot_tests.rs", LIFETIME)],
        "#[cfg(test)]\nmod pilot_tests;\n",
        "\"xml\"",
        &Default::default(),
    );
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("lifetime")
            || stderr.contains("does not live long enough")
            || stderr.contains("may not live long enough"),
        "{stderr}"
    );
}

#[test]
fn write_hooks_keep_rule_order_and_start_tag_attributes() {
    let run = support::run_consumer(
        "xml-order-consumer",
        &order_bundle(),
        &[("pilot_tests.rs", &format!("{COMMON}{ORDER}"))],
        "#[cfg(test)]\nmod pilot_tests;\n",
        "\"xml\"",
        &Default::default(),
    );
    assert!(run.stdout.contains("1 passed"), "{}", run.stdout);
}
