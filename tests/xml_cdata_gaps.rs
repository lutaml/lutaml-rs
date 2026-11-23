//! Source parity for the CDATA read shapes the shared oracle does not cover (measured against lutaml-model
//! e4439b37 on Nokogiri): collection `cdata: true` content, repeated scalar elements, non-adjacent CDATA groups
//! and elements the source reads as nil beside other collection items.
#![cfg(feature = "xml")]
mod support;
use serde_json::json;

fn attr(name: &str, collection: bool) -> serde_json::Value {
    json!({"name": name, "type": {"kind":"builtin","name":"string"}, "required": false,
        "collection": if collection { json!({"min":0,"max":null}) } else { json!(false) },
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
            "CoContentC",
            "r",
            vec![attr("text", true)],
            vec![rule("content", None, "text", true)],
        ),
        doc(
            "ScElemP",
            "p",
            vec![attr("v", false)],
            vec![rule("element", Some("v"), "v", false)],
        ),
        doc(
            "ScElemC",
            "p",
            vec![attr("v", false)],
            vec![rule("element", Some("v"), "v", true)],
        ),
        doc(
            "CoElemP",
            "p",
            vec![attr("v", true)],
            vec![rule("element", Some("v"), "v", false)],
        ),
        doc(
            "CoElemC",
            "p",
            vec![attr("v", true)],
            vec![rule("element", Some("v"), "v", true)],
        ),
        doc(
            "ScContentChildC",
            "r",
            vec![attr("text", false), attr("c", false)],
            vec![
                rule("content", None, "text", true),
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

fn list(p: &Presence<Vec<String>>) -> Option<Vec<String>> {
    match p { Presence::Value(v) => Some(v.clone()), _ => None }
}
fn one(p: &Presence<String>) -> Option<String> {
    match p { Presence::Value(v) => Some(v.clone()), _ => None }
}
fn refused<T>(r: Result<T, Box<super::contracts::ContractError>>) -> bool {
    matches!(r, Err(e) if e.reason == ErrorReason::InvalidDocument)
}
fn s(v: &[&str]) -> Option<Vec<String>> { Some(v.iter().map(|x| x.to_string()).collect()) }

#[test]
fn collection_cdata_content_reads_cdata_groups() {
    let r = |x: &str| list(&read_CoContentC_xml(x.as_bytes()).unwrap().text);
    assert_eq!(r("<r><![CDATA[a]]></r>"), s(&["a"]));
    assert_eq!(r("<r>a</r>"), s(&[""]));
    assert_eq!(r("<r></r>"), s(&[""]));
    assert_eq!(r("<r>a<![CDATA[b]]>c</r>"), s(&["b"]));
    assert_eq!(r("<r><![CDATA[a]]><![CDATA[b]]></r>"), s(&["ab"]));
    assert_eq!(r("<r><![CDATA[a]]>x<![CDATA[b]]></r>"), s(&["a", "b"]));
    assert_eq!(r("<r><![CDATA[a]]><!--c--><![CDATA[b]]></r>"), s(&["a", "b"]));
    assert_eq!(r("<r><![CDATA[  ]]></r>"), s(&["  "]));
    assert_eq!(r("<r>a<!--c-->b</r>"), None);
    // Write then read keeps the value.
    let mut m = CoContentC::default();
    m.text = Presence::Value(vec!["a".to_string()]);
    assert_eq!(r(&write_CoContentC_xml(&m).unwrap()), s(&["a"]));
}

#[test]
fn repeated_scalar_elements_are_refused_by_name() {
    assert!(refused(read_ScElemP_xml(b"<p><v>a</v><v>z</v></p>")));
    assert!(refused(read_ScElemC_xml(b"<p><v><![CDATA[a]]></v><v>z</v></p>")));
    // The source lists nothing here: no CDATA group in any element.
    assert_eq!(one(&read_ScElemC_xml(b"<p><v>z</v><v>a</v></p>").unwrap().v), None);
    assert!(refused(read_ScContentChildC_xml(b"<r>a<c>k</c><c>l</c></r>")));
    assert_eq!(one(&read_ScContentChildC_xml(b"<r>a<c>k</c></r>").unwrap().c), Some("k".into()));
}

#[test]
fn nonadjacent_cdata_groups_follow_the_source() {
    let sc = |x: &str| one(&read_ScElemC_xml(x.as_bytes()).unwrap().v);
    assert_eq!(sc("<p><v><![CDATA[a]]>x<![CDATA[b]]></v></p>"), Some("a".into()));
    assert_eq!(sc("<p><v><![CDATA[a]]><!--c--><![CDATA[b]]></v></p>"), Some("a".into()));
    assert_eq!(sc("<p><v><![CDATA[a]]><![CDATA[b]]></v></p>"), Some("ab".into()));
    let co = |x: &str| list(&read_CoElemC_xml(x.as_bytes()).unwrap().v);
    assert_eq!(co("<p><v><![CDATA[a]]>x<![CDATA[b]]></v></p>"), s(&["a"]));
    assert_eq!(co("<p><v><![CDATA[a]]>x<![CDATA[b]]></v><v>z</v></p>"), s(&["a", "b"]));
    assert_eq!(co("<p><v>z</v><v><![CDATA[a]]><?p q?><![CDATA[b]]></v></p>"), s(&["a", "b"]));
}

#[test]
fn text_split_by_a_comment_or_pi_reads_as_nil() {
    let sp = |x: &str| one(&read_ScElemP_xml(x.as_bytes()).unwrap().v);
    assert_eq!(sp("<p><v>a<!--c-->b</v></p>"), None);
    assert_eq!(sp("<p><v>a<?p q?>b</v></p>"), None);
    assert_eq!(sp("<p><v><![CDATA[a]]><!--c--><![CDATA[b]]></v></p>"), None);
    assert_eq!(sp("<p><v>a</v></p>"), Some("a".into()));
}

#[test]
fn nil_items_beside_other_items_are_refused_not_dropped() {
    assert!(refused(read_CoElemP_xml(b"<p><v>a<![CDATA[b]]></v><v>z</v></p>")));
    assert!(refused(read_CoElemP_xml(b"<p><v>z</v><v>a<!--c-->b</v></p>")));
    // A lone nil item is the source's empty list.
    assert_eq!(list(&read_CoElemP_xml(b"<p><v>a<![CDATA[b]]></v></p>").unwrap().v), None);
    assert_eq!(list(&read_CoElemP_xml(b"<p><v>a</v><v>z</v></p>").unwrap().v), s(&["a", "z"]));
}
"##;

#[test]
fn cdata_read_gaps_match_the_source() {
    let run = support::run_consumer(
        "xml-cdata-gaps-consumer",
        &bundle(),
        &[("pilot_tests.rs", CONSUMER)],
        "#[cfg(test)]\nmod pilot_tests;\n",
        "\"xml\"",
        &Default::default(),
    );
    assert!(run.stdout.contains("5 passed"), "{}", run.stdout);
}
