//! Scalar CDATA output and the scalar read shape: element/content, single/collection, literal entities,
//! Unicode, rule order, `]]>` splitting that round-trips, and named refusal of scalar content scattered
//! around mapped children.
#![cfg(feature = "xml")]
mod support;
use serde_json::json;

fn string_attr(name: &str, collection: bool) -> serde_json::Value {
    json!({"name": name, "type": {"kind":"builtin","name":"string"}, "required": false,
        "collection": if collection { json!({"min":0,"max":null}) } else { json!(false) },
        "default": {"present": false}, "initialize_empty": false})
}
fn bundle() -> Vec<u8> {
    let rule = |kind: &str, name: Option<&str>, to: &str, cdata: bool| {
        let mut r = json!({"kind": kind, "to": to, "namespace": {"mode":"inherit"}, "cdata": cdata, "render": {"default": false, "empty": "omit", "nil": "omit"}});
        if let Some(n) = name {
            r["name"] = json!([n]);
        }
        r
    };
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
            "Cd",
            "cd",
            vec![
                string_attr("a", false),
                string_attr("bs", true),
                string_attr("plain", false),
                string_attr("z", false),
            ],
            vec![
                rule("element", Some("a"), "a", true),
                rule("element", Some("b"), "bs", true),
                rule("element", Some("plain"), "plain", false),
                rule("element", Some("z"), "z", true),
            ],
        ),
        doc(
            "CdContent",
            "cc",
            vec![string_attr("body", false)],
            vec![rule("content", None, "body", true)],
        ),
        doc(
            "CdContents",
            "ccs",
            vec![string_attr("parts", true)],
            vec![rule("content", None, "parts", true)],
        ),
        doc(
            "CdScatter",
            "cs",
            vec![string_attr("body", false), string_attr("kid", false)],
            vec![
                rule("content", None, "body", false),
                rule("element", Some("kid"), "kid", false),
            ],
        ),
    ]
    .into_iter()
    .collect();
    serde_json::to_vec(&json!({"lml_bundle":1,"roots":["Cd@default"],"models":models,"value_types":{},"capabilities":["xml"]})).unwrap()
}
const CONSUMER: &str = r##"
use super::contracts::ErrorReason;
use super::models::*;
use super::xml_codecs::*;

fn cd(a: &str, bs: &[&str], plain: &str) -> Cd {
    let mut m = Cd::default();
    m.a = Presence::Value(a.into());
    m.bs = Presence::Value(bs.iter().map(|s| s.to_string()).collect());
    m.plain = Presence::Value(plain.into());
    m
}

#[test]
fn scalar_and_collection_elements_are_written_as_cdata_in_rule_order() {
    let out = write_Cd_xml(&cd("a<b>&amp; é 日本 x", &["one", "t<w>o"], "p&q")).unwrap();
    // entities inside CDATA stay literal; ordinary text is escaped; rule order a, b, b, plain
    assert_eq!(
        out,
        "<cd><a><![CDATA[a<b>&amp; é 日本 x]]></a><b><![CDATA[one]]></b><b><![CDATA[t<w>o]]></b><plain>p&amp;q</plain></cd>"
    );
    let back = read_Cd_xml(out.as_bytes()).unwrap();
    assert_eq!(back.a, Presence::Value("a<b>&amp; é 日本 x".to_string()));
    assert_eq!(back.bs, Presence::Value(vec!["one".to_string(), "t<w>o".to_string()]));
    assert_eq!(back.plain, Presence::Value("p&q".to_string()));
}

#[test]
fn a_cdata_terminator_is_split_across_sections_and_round_trips() {
    let out = write_Cd_xml(&cd("has ]]> inside", &[], "x")).unwrap();
    assert!(out.contains("<![CDATA[has ]]]]><![CDATA[> inside]]>"), "{out}");
    assert_eq!(read_Cd_xml(out.as_bytes()).unwrap().a, Presence::Value("has ]]> inside".to_string()));
}

#[test]
fn content_is_written_as_cdata_single_and_collection() {
    let mut c = CdContent::default();
    c.body = Presence::Value("<raw> & é".into());
    let out = write_CdContent_xml(&c).unwrap();
    assert_eq!(out, "<cc><![CDATA[<raw> & é]]></cc>");
    assert_eq!(read_CdContent_xml(out.as_bytes()).unwrap().body, c.body);
    let mut cs = CdContents::default();
    cs.parts = Presence::Value(vec!["x<1>".into(), "y".into()]);
    assert_eq!(write_CdContents_xml(&cs).unwrap(), "<ccs><![CDATA[x<1>]]><![CDATA[y]]></ccs>");
}

#[test]
fn cdata_rules_keep_only_cdata_and_ordinary_rules_keep_ordinary_text() {
    // source (evidence/cdata-ruby-oracle.json): a cdata rule reads CDATA sections only
    let m = read_Cd_xml(b"<cd><a><![CDATA[x<y]]></a><b>q</b><b><![CDATA[<r>]]></b></cd>").unwrap();
    assert_eq!((m.a, m.bs), (Presence::Value("x<y".to_string()), Presence::Value(vec!["<r>".to_string()])));
    // ordinary text under a cdata rule is nil; CDATA among ordinary text keeps the CDATA piece
    assert!(read_Cd_xml(b"<cd><a>plain</a></cd>").unwrap().a.is_absent());
    assert_eq!(read_Cd_xml(b"<cd><a>one<![CDATA[two]]>three</a></cd>").unwrap().a, Presence::Value("two".to_string()));
    // an ordinary element rule: CDATA-only reads, ordinary+CDATA mixed reads as nil
    assert_eq!(read_Cd_xml(b"<cd><plain><![CDATA[x]]></plain></cd>").unwrap().plain, Presence::Value("x".to_string()));
    assert!(read_Cd_xml(b"<cd><plain>o<![CDATA[t]]>h</plain></cd>").unwrap().plain.is_absent());
}

#[test]
fn scalar_content_beside_a_mapped_child_is_refused_by_name() {
    // source: ordinary scalar content next to a mapped child reads as an Array (invalid for a scalar)
    for xml in [&b"<cs>a<kid>k</kid>b</cs>"[..], b"<cs>a<kid>k</kid></cs>", b"<cs><kid>k</kid>a</cs>", b"<cs>\n  text\n  <kid>k</kid>\n</cs>"] {
        let error = read_CdScatter_xml(xml).unwrap_err();
        assert_eq!(error.reason, ErrorReason::InvalidDocument);
        assert!(error.detail.unwrap().contains("reads as an array in the source"));
    }
    // an empty or layout-only list has no value: the source writes nothing for it
    for xml in [&b"<cs><kid>k</kid></cs>"[..], b"<cs>\n  <kid>k</kid>\n</cs>", b"<cs><!--x--><kid>k</kid></cs>", b"<cs><![CDATA[a]]><kid>k</kid></cs>"] {
        assert!(read_CdScatter_xml(xml).unwrap().body.is_absent());
    }
    // an unmapped child never splits the scalar
    assert_eq!(read_CdScatter_xml(b"<cs>a<other/>b</cs>").unwrap().body, Presence::Value("ab".to_string()));
}
"##;

#[test]
fn scalar_cdata_matrix() {
    let run = support::run_consumer(
        "xml-cdata-consumer",
        &bundle(),
        &[("pilot_tests.rs", CONSUMER)],
        "#[cfg(test)]\nmod pilot_tests;\n",
        "\"xml\"",
        &Default::default(),
    );
    for line in run.stdout.lines().filter(|l| l.contains("PROBE")) {
        println!("{line}");
    }
}
