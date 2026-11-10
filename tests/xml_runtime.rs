//! Native XML codecs over moxml-rs (09c): generated codecs are compiled into a
//! consumer crate and replayed against the Ruby oracle (shared with the TS
//! runtime), plus writer, namespace, safety and generation-refusal behavior.
#![cfg(feature = "xml")]
use lutaml_rs::contracts::*;
use lutaml_rs::generator::{model_names, RustEmitter};
use lutaml_rs::xml::moxml_binding as rt;
use lutaml_rs::xml::RustXmlEmitter;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::OnceLock;

const ORACLE: &str = include_str!("fixtures/xml_oracle.json");
const PERSON: &str = "XmlOracle::Person@default";

fn oracle() -> Value {
    serde_json::from_str(ORACLE).unwrap()
}

fn str_attr(name: &str) -> Value {
    json!({"name": name, "type": {"kind": "builtin", "name": "string"}, "required": false,
        "collection": false, "default": {"present": false}, "initialize_empty": false})
}

fn doc_model(name: &str, xml: Value, attributes: Vec<Value>) -> Value {
    json!({"lml": 2, "model": name, "register": "default", "attributes": attributes, "mappings": {"xml": xml}})
}

fn element(name: &str, to: &str, namespace: Value) -> Value {
    json!({"kind": "element", "name": [name], "to": to, "namespace": namespace})
}

/// The oracle bundle plus hand-written models whose rule namespaces the Ruby DSL cannot declare.
fn full_bundle() -> Bundle {
    let mut raw = oracle()["bundle"].clone();
    let models = raw["models"].as_object_mut().unwrap();
    models.insert(
        "Doc@default".into(),
        doc_model(
            "Doc",
            json!({"root": {"element": "doc"}, "namespace": {"mode": "uri", "uri": "urn:doc", "prefix": "d"},
                "rules": [
                    element("a", "a", json!({"mode": "inherit"})),
                    element("b", "b", json!({"mode": "uri", "uri": "urn:b", "prefix": "bb"})),
                    element("c", "c", json!({"mode": "unqualified"})),
                    {"kind": "attribute", "name": ["x"], "to": "x", "namespace": {"mode": "uri", "uri": "urn:attr", "prefix": "at"}},
                    {"kind": "attribute", "name": ["y"], "to": "y", "namespace": {"mode": "inherit"}}]}),
            ["a", "b", "c", "x", "y"].map(str_attr).to_vec(),
        ),
    );
    models.insert(
        "Any@default".into(),
        doc_model(
            "Any",
            json!({"root": {"element": "n"}, "namespace": {"mode": "uri", "uri": "d"},
                "rules": [
                    {"kind": "attribute", "name": ["id"], "to": "id", "namespace": {"mode": "inherit"}},
                    {"kind": "any_attribute", "to": "other"}]}),
            vec![
                str_attr("id"),
                json!({"name": "other", "type": {"kind": "builtin", "name": "hash"}, "required": false,
                    "collection": false, "default": {"present": false}, "initialize_empty": false}),
            ],
        ),
    );
    models.insert(
        "QDoc@default".into(),
        doc_model(
            "QDoc",
            json!({"root": {"element": "doc"}, "rules": [element("q", "q", json!({"mode": "inherit"}))]}),
            vec![json!({"name": "q", "type": {"kind": "builtin", "name": "qname"}, "required": false,
                "collection": false, "default": {"present": false}, "initialize_empty": false})],
        ),
    );
    models.insert(
        "NoPrefix@default".into(),
        doc_model(
            "NoPrefix",
            json!({"root": {"element": "doc"}, "rules": [
                {"kind": "attribute", "name": ["x"], "to": "x", "namespace": {"mode": "uri", "uri": "urn:attr"}}]}),
            vec![str_attr("x")],
        ),
    );
    serde_json::from_value(raw).unwrap()
}

struct Consumer {
    binary: std::path::PathBuf,
}

fn consumer() -> &'static Consumer {
    static C: OnceLock<Consumer> = OnceLock::new();
    C.get_or_init(build_consumer)
}

fn build_consumer() -> Consumer {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let dir = root.join("target/xml-generated-consumer");
    std::fs::create_dir_all(dir.join("src")).unwrap();
    let bundle = full_bundle();
    let models = RustEmitter
        .generate_models(&bundle, &TypeBindings::new())
        .unwrap();
    let codecs = RustXmlEmitter
        .generate_xml_codecs(&bundle, &TypeBindings::new())
        .unwrap();
    let names = model_names(&bundle).unwrap();
    let mut read_arms = String::new();
    let mut write_arms = String::new();
    for (id, model) in &bundle.models {
        if model.mappings.xml.is_none() {
            continue;
        }
        let n = &names[id];
        read_arms += &format!("{id:?} => res(xml_codecs::read_{n}_xml(xml.as_bytes()).and_then(|m| serde_json::to_value(&m).map_err(|e| Box::new(ContractError {{ reason: ErrorReason::NotSerializable, model_id: None, attribute: None, format: Some(\"xml\".into()), rule: None, location: String::new(), klass: None, detail: Some(e.to_string()) }})))),\n");
        write_arms += &format!("{id:?} => match serde_json::from_value::<models::{n}>(op[\"data\"].clone()) {{ Ok(m) => res(xml_codecs::write_{n}_xml(&m).map(Value::String)), Err(e) => json!({{\"bad_data\": e.to_string()}}) }},\n");
    }
    let main = format!(
        "use generated_xml_consumer::{{contracts::*, models, xml_codecs}};\nuse serde_json::{{json, Value}};\nuse std::io::Read;\n\
fn res(r: Result<Value, Box<ContractError>>) -> Value {{ match r {{ Ok(v) => json!({{\"ok\": v}}), Err(e) => json!({{\"err\": serde_json::to_value(&*e).unwrap()}}) }} }}\n\
fn main() {{\n let mut input = String::new();\n std::io::stdin().read_to_string(&mut input).unwrap();\n let ops: Vec<Value> = serde_json::from_str(&input).unwrap();\n let mut out = Vec::new();\n for op in ops {{\n  let model = op[\"model\"].as_str().unwrap().to_string();\n  let xml = op[\"xml\"].as_str().unwrap_or(\"\").to_string();\n  let v = if op[\"op\"] == \"read\" {{ match model.as_str() {{\n{read_arms} other => panic!(\"no model {{other}}\") }} }} else {{ match model.as_str() {{\n{write_arms} other => panic!(\"no model {{other}}\") }} }};\n  out.push(v);\n }}\n print!(\"{{}}\", serde_json::to_string(&out).unwrap());\n}}\n"
    );
    std::fs::write(dir.join("src/main.rs"), main).unwrap();
    std::fs::write(dir.join("src/models.rs"), &models["models.rs"]).unwrap();
    std::fs::write(dir.join("src/xml_codecs.rs"), &codecs["xml_codecs.rs"]).unwrap();
    std::fs::write(
        dir.join("src/lib.rs"),
        "pub mod contracts { pub use lutaml_rs::contracts::*; }\npub mod values { pub use lutaml_rs::values::*; }\npub mod xml_rt { pub use lutaml_rs::xml::*; }\npub mod models;\npub mod xml_codecs;\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("Cargo.toml"),
        format!(
            "[package]\nname=\"generated-xml-consumer\"\nversion=\"0.0.0\"\nedition=\"2021\"\n[workspace]\n[dependencies]\nlutaml-rs={{path={}, features=[\"xml\"]}}\nserde={{version=\"=1.0.219\",features=[\"derive\"]}}\nserde_json={{version=\"=1.0.140\",features=[\"arbitrary_precision\"]}}\nnum-bigint=\"=0.4.6\"\nnum-rational=\"=0.4.2\"\nnum-traits=\"=0.2.19\"\n",
            serde_json::to_string(root.to_str().unwrap()).unwrap()
        ),
    )
    .unwrap();
    let target = root.join("target/xml-consumer-build");
    let result = Command::new(env!("CARGO"))
        .args(["build", "--offline", "--manifest-path"])
        .arg(dir.join("Cargo.toml"))
        .env("CARGO_BUILD_JOBS", "1")
        .env("CARGO_TARGET_DIR", &target)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "generated consumer failed to compile:\n{}",
        String::from_utf8_lossy(&result.stderr)
    );
    Consumer {
        binary: target.join("debug/generated-xml-consumer"),
    }
}

fn run(ops: Vec<Value>) -> Vec<Value> {
    let mut child = Command::new(&consumer().binary)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(serde_json::to_string(&ops).unwrap().as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "consumer died: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}

fn read(model: &str, xml: &str) -> Value {
    run(vec![json!({"op": "read", "model": model, "xml": xml})]).remove(0)
}

fn write(model: &str, data: &Value) -> Value {
    run(vec![json!({"op": "write", "model": model, "data": data})]).remove(0)
}

fn ok(v: &Value) -> &Value {
    assert!(v.get("ok").is_some(), "expected ok, got {v}");
    &v["ok"]
}

fn err(v: &Value) -> &Value {
    assert!(v.get("err").is_some(), "expected an error, got {v}");
    &v["err"]
}

/// Neutral dump: every declared attribute in order, null for absent, empty lists
/// as null except the ordered-content token list, which stays `[]`.
fn dump(bundle: &Bundle, id: &str, value: &Value) -> Value {
    let Some(obj) = value.as_object() else {
        return value.clone();
    };
    let model = &bundle.models[id];
    let mut out = serde_json::Map::new();
    for a in &model.attributes {
        let target = a.reference.clone().or(match &a.value_type {
            TypeRef::Model { id } => Some(id.clone()),
            _ => None,
        });
        let v = obj.get(&a.name).cloned().unwrap_or(Value::Null);
        out.insert(a.name.clone(), dump_member(bundle, target.as_deref(), &v));
    }
    for (k, v) in obj {
        if !out.contains_key(k) && k != "__lml_content" {
            out.insert(k.clone(), v.clone());
        }
    }
    if let Some(tokens) = obj.get("__lml_content") {
        out.insert("__lml_content".into(), tokens.clone());
    }
    Value::Object(out)
}

fn dump_member(bundle: &Bundle, target: Option<&str>, v: &Value) -> Value {
    match (v, target) {
        (Value::Array(l), _) if l.is_empty() => Value::Null,
        (Value::Array(l), t) => Value::Array(l.iter().map(|x| dump_member(bundle, t, x)).collect()),
        (Value::Object(_), Some(t)) => dump(bundle, t, v),
        _ => v.clone(),
    }
}

fn overrides() -> BTreeMap<&'static str, Value> {
    let nulls = json!({"id": null, "lang": null, "name": null, "nick": null, "tags": null, "addr": null, "age": null, "ni": null, "nb": null, "ee": null});
    BTreeMap::from([
        ("person-prefixed-attribute-lenient", nulls),
        ("person-wrong-root", json!({"error": "invalid_document"})),
        (
            "person-wrong-namespace",
            json!({"error": "invalid_document"}),
        ),
        (
            "person-bad-integer",
            json!({"error": "invalid_document", "attribute": "age"}),
        ),
        (
            "kennel-poly-unknown-kind",
            json!({"error": "unknown_discriminator"}),
        ),
    ])
}

#[test]
fn wasm_replay_overrides_match_the_native_table() {
    let text = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/wasm_xml/overrides.mjs"),
    )
    .unwrap();
    let start = text.find("export const OVERRIDES = {").unwrap();
    let block = &text[start..start + text[start..].find("\n};").unwrap()];
    let wasm: std::collections::BTreeSet<&str> = block
        .lines()
        .filter_map(|l| l.trim().strip_prefix('\'')?.split('\'').next())
        .collect();
    let native: std::collections::BTreeSet<&str> = overrides().keys().copied().collect();
    assert_eq!(
        wasm, native,
        "tests/wasm_xml/overrides.mjs drifted from overrides()"
    );
}

#[test]
fn oracle_fixture_is_a_valid_bundle_and_projects() {
    let bundle = full_bundle();
    assert_eq!(oracle()["cases"].as_array().unwrap().len(), 32);
    for id in bundle.models.keys() {
        lutaml_rs::xml::projection::compile_projection(&bundle, id).unwrap();
    }
}

#[test]
fn emitter_output_is_deterministic_and_single_file() {
    let bundle = full_bundle();
    let a = RustXmlEmitter
        .generate_xml_codecs(&bundle, &TypeBindings::new())
        .unwrap();
    let b = RustXmlEmitter
        .generate_xml_codecs(&bundle.clone(), &TypeBindings::new())
        .unwrap();
    assert_eq!(a.keys().collect::<Vec<_>>(), vec!["xml_codecs.rs"]);
    assert_eq!(a, b);
    let text = &a["xml_codecs.rs"];
    let names = model_names(&bundle).unwrap();
    for n in [
        "Person", "Plain", "Para", "Ord", "Kennel", "Animal", "Dog", "Addr",
    ] {
        let n = &names[&format!("XmlOracle::{n}@default")];
        assert!(text.contains(&format!("pub fn read_{n}_xml(")), "{n}");
        assert!(text.contains(&format!("pub fn write_{n}_xml(")), "{n}");
        assert!(
            text.contains(&format!("impl XmlWriter for XmlCodec_{n}")),
            "{n}"
        );
    }
    assert!(!text.contains("unsafe") && !text.contains("eval"));
}

/// Ruby (wave1 worktree, `bundle exec ruby`) reads `<pet><name>Tom</name></pet>` as
/// the base class with `kind` nil, and writes it back with no kind attribute and no
/// kind member. The generated union must not invent the base tag on the way out.
#[test]
fn absent_polymorphic_tag_reads_the_base_case_with_no_tag_like_ruby() {
    let bundle = full_bundle();
    let kennel = "XmlOracle::Kennel@default";
    let read_back = read(kennel, "<kennel><pet><name>Tom</name></pet></kennel>");
    assert_eq!(
        dump(&bundle, kennel, ok(&read_back)),
        json!({"pet": {"kind": null, "name": "Tom"}, "owner": null})
    );
    let written = write(kennel, ok(&read_back));
    let text = ok(&written).as_str().unwrap();
    assert_eq!(
        text.split_whitespace().collect::<String>(),
        "<kennel><pet><name>Tom</name></pet></kennel>"
    );
}

#[test]
fn ruby_oracle_replay_reads_all_32_cases() {
    let bundle = full_bundle();
    let cases = oracle()["cases"].as_array().unwrap().clone();
    let over = overrides();
    let ops: Vec<Value> = cases
        .iter()
        .map(|c| json!({"op": "read", "model": c["model_id"], "xml": c["input"]}))
        .collect();
    let results = run(ops);
    assert_eq!(results.len(), 32);
    let mut passed = 0;
    for (c, r) in cases.iter().zip(&results) {
        let id = c["id"].as_str().unwrap();
        let model_id = c["model_id"].as_str().unwrap();
        let o = over.get(id);
        let expects_error =
            c.get("expected_error").is_some() || o.is_some_and(|o| o.get("error").is_some());
        if expects_error {
            let e = err(r);
            assert_eq!(e["format"], "xml", "{id}");
            assert!(e["model_id"].is_string(), "{id}: {e}");
            if let Some(o) = o {
                assert_eq!(e["reason"], o["error"], "{id}: {e}");
                if let Some(attr) = o.get("attribute") {
                    assert_eq!(&e["attribute"], attr, "{id}: {e}");
                    assert_eq!(&e["rule"], attr, "{id}: {e}");
                }
            }
        } else {
            let expected = o.cloned().unwrap_or_else(|| c["expected_data"].clone());
            assert_eq!(dump(&bundle, model_id, ok(r)), expected, "{id}");
        }
        passed += 1;
    }
    assert_eq!(passed, 32);
}

#[test]
fn writer_round_trips_every_ruby_read_value() {
    let bundle = full_bundle();
    let cases = oracle()["cases"].as_array().unwrap().clone();
    let over = overrides();
    let eligible: Vec<&Value> = cases
        .iter()
        .filter(|c| {
            let id = c["id"].as_str().unwrap();
            c.get("expected_data").is_some() && !over.contains_key(id)
        })
        .collect();
    let reads = run(eligible
        .iter()
        .map(|c| json!({"op": "read", "model": c["model_id"], "xml": c["input"]}))
        .collect());
    let writes = run(eligible
        .iter()
        .zip(&reads)
        .map(|(c, r)| json!({"op": "write", "model": c["model_id"], "data": ok(r)}))
        .collect());
    let texts: Vec<&str> = writes.iter().map(|w| ok(w).as_str().unwrap()).collect();
    let back = run(eligible
        .iter()
        .zip(&texts)
        .map(|(c, t)| json!({"op": "read", "model": c["model_id"], "xml": t}))
        .collect());
    for ((c, text), r) in eligible.iter().zip(&texts).zip(&back) {
        let id = c["id"].as_str().unwrap();
        let mut expected = c["expected_data"].clone();
        if c["model_id"] == PERSON {
            // An empty attribute is omitted; as_nil/as_blank write an empty element that reads back "".
            for m in ["id", "lang"] {
                if expected[m] == "" {
                    expected[m] = Value::Null;
                }
            }
            for m in ["ni", "nb"] {
                expected[m] = if expected[m].is_null() {
                    json!("")
                } else {
                    Value::Null
                };
            }
        }
        assert_eq!(
            dump(&bundle, c["model_id"].as_str().unwrap(), ok(r)),
            expected,
            "{id}: {text}"
        );
    }
    assert!(eligible.len() >= 24, "{}", eligible.len());
}

#[test]
fn writer_emits_namespaces_nil_policies_mixed_ordered_and_cdata() {
    let person = ok(&read(
        PERSON,
        r#"<person xmlns="urn:ex" id="1"><name>N</name><addr xmlns="urn:addr"><city>C</city></addr></person>"#,
    ))
    .clone();
    let text = ok(&write(PERSON, &person)).as_str().unwrap().to_string();
    assert!(text.contains(r#"xmlns:ex="urn:ex""#), "{text}");
    assert!(text.contains("<ex:name>N</ex:name>"), "{text}");
    assert!(text.contains(r#"xmlns:ad="urn:addr""#), "{text}");
    assert!(text.contains("<ad:addr"), "{text}");

    let empty = ok(&read(PERSON, r#"<person xmlns="urn:ex"><ee/></person>"#)).clone();
    let text = ok(&write(PERSON, &empty)).as_str().unwrap().to_string();
    assert!(
        text.contains("<ex:ni") && text.contains(r#"nil="true""#),
        "{text}"
    );
    assert!(text.contains("<ex:nb"), "{text}");
    assert!(text.contains("<ex:ee"), "{text}");
    assert!(!text.contains("<ex:age"), "{text}");

    let para = "XmlOracle::Para@default";
    let mixed = "<p>a<b>x</b>c<i>y</i>d<b>z</b></p>";
    let data = ok(&read(para, mixed)).clone();
    assert_eq!(ok(&write(para, &data)), mixed);
    let ord = "XmlOracle::Ord@default";
    let ordered = "<ord><c>1</c><a>2</a><c>3</c><a>4</a></ord>";
    let data = ok(&read(ord, ordered)).clone();
    assert_eq!(ok(&write(ord, &data)), ordered);
    let cdata = "<p>a<![CDATA[raw]]></p>";
    let data = ok(&read(para, cdata)).clone();
    assert_eq!(ok(&write(para, &data)), cdata);
}

#[test]
fn ordered_writer_refuses_a_token_list_that_drops_a_mapped_value() {
    let ord = "XmlOracle::Ord@default";
    let mut data = ok(&read(ord, "<ord><c>1</c><a>2</a></ord>")).clone();
    data["__lml_content"].as_array_mut().unwrap().pop();
    let e = write(ord, &data);
    assert_eq!(err(&e)["reason"], "invalid_ordered_content");
}

#[test]
fn doctype_and_external_entities_are_refused() {
    let plain = "XmlOracle::Plain@default";
    for input in [
        r#"<!DOCTYPE plain [<!ENTITY e "x">]><plain><v>&e;</v></plain>"#,
        r#"<!DOCTYPE plain [<!ENTITY x SYSTEM "file:///etc/passwd">]><plain><v>&x;</v></plain>"#,
        r#"<!DOCTYPE plain SYSTEM "http://127.0.0.1:9/evil.dtd"><plain/>"#,
    ] {
        let r = read(plain, input);
        let e = err(&r);
        assert_eq!(e["reason"], "invalid_document", "{input}");
        assert_eq!(e["format"], "xml");
        assert!(e["detail"].as_str().unwrap().contains("DOCTYPE"), "{e}");
    }
    let e = rt::parse(PERSON, b"<!DOCTYPE a><a/>").err().unwrap();
    assert_eq!(e.reason, ErrorReason::InvalidDocument);
    let big = vec![b' '; rt::MAX_INPUT + 1];
    assert_eq!(
        rt::parse(PERSON, &big).err().unwrap().reason,
        ErrorReason::InvalidDocument
    );
}

#[test]
fn error_context_names_model_format_and_location() {
    let e = read(PERSON, r#"<person xmlns="urn:ex""#);
    let e = err(&e);
    assert_eq!(e["reason"], "invalid_document");
    assert_eq!(e["model_id"], PERSON);
    assert_eq!(e["format"], "xml");
    assert_eq!(e["location"], "/input");
    let e = read(PERSON, "<other/>");
    let d = err(&e)["detail"].as_str().unwrap().to_string();
    assert!(
        d.contains("expected root element person") && d.contains("found other"),
        "{d}"
    );
    let e = read(
        "XmlOracle::Kennel@default",
        r#"<kennel><pet kind="cat"><name>T</name></pet></kennel>"#,
    );
    let e = err(&e);
    assert_eq!(e["reason"], "unknown_discriminator");
    assert_eq!(e["model_id"], "XmlOracle::Kennel@default");
    assert_eq!(e["location"], "/kennel/pet[0]/@kind");
}

#[test]
fn hand_written_rule_namespaces_read_and_write() {
    let bundle = full_bundle();
    let xml = "<d:doc xmlns:d=\"urn:doc\" xmlns:at=\"urn:attr\" xmlns:o=\"urn:other\" at:x=\"X\" y=\"Y\">\
<d:a>A</d:a><bb:b xmlns:bb=\"urn:b\">B</bb:b><c>C</c></d:doc>";
    let r = read("Doc@default", xml);
    let want = json!({"a": "A", "b": "B", "c": "C", "x": "X", "y": "Y"});
    assert_eq!(dump(&bundle, "Doc@default", ok(&r)), want);
    let wrong = "<d:doc xmlns:d=\"urn:doc\" xmlns:at=\"urn:attr\" x=\"no\" at:y=\"no\"><d:b>no</d:b><d:c>no</d:c></d:doc>";
    let r = read("Doc@default", wrong);
    assert_eq!(
        dump(&bundle, "Doc@default", ok(&r)),
        json!({"a": null, "b": null, "c": null, "x": null, "y": null})
    );
    let written = write("Doc@default", ok(&read("Doc@default", xml)));
    let text = ok(&written).as_str().unwrap();
    assert!(text.contains(r#"xmlns:bb="urn:b""#), "{text}");
    assert!(text.contains("at:x="), "{text}");
    let back = read("Doc@default", text);
    assert_eq!(dump(&bundle, "Doc@default", ok(&back)), want);
}

#[test]
fn qname_values_resolve_through_in_scope_prefixes() {
    let r = read(
        "QDoc@default",
        r#"<doc xmlns:p="urn:p"><q>p:thing</q></doc>"#,
    );
    assert_eq!(ok(&r)["q"], json!({"namespace": "urn:p", "local": "thing"}));
    let bad = read("QDoc@default", "<doc><q>zz:thing</q></doc>");
    let e = err(&bad);
    assert_eq!(e["reason"], "invalid_document");
    assert_eq!(e["attribute"], "q");
    assert_eq!(e["rule"], "q");
}

#[test]
fn unprefixed_namespaced_attribute_writer_is_refused_by_name() {
    let r = read("NoPrefix@default", r#"<doc xmlns:q="urn:attr" q:x="1"/>"#);
    assert_eq!(ok(&r)["x"], "1");
    let w = write("NoPrefix@default", ok(&r));
    let e = err(&w);
    assert_eq!(e["reason"], "unsupported_mapping");
    assert_eq!(e["format"], "xml");
    assert!(
        e["detail"].as_str().unwrap().contains("declares no prefix"),
        "{e}"
    );
}

fn refusal(edit: impl FnOnce(&mut Bundle)) -> ContractError {
    let mut b = full_bundle();
    edit(&mut b);
    *RustXmlEmitter
        .generate_xml_codecs(&b, &TypeBindings::new())
        .unwrap_err()
}

#[test]
fn generation_refusals_name_the_mapping() {
    let e = refusal(|b| {
        let mut list = str_attr("t");
        list["collection"] = json!({"min": 0, "max": null});
        let model = doc_model(
            "ListAttr",
            json!({"root": {"element": "doc"}, "rules": [{"kind": "attribute", "name": ["t"], "to": "t"}]}),
            vec![list],
        );
        b.models.insert(
            "ListAttr@default".into(),
            serde_json::from_value(model).unwrap(),
        );
    });
    assert_eq!(e.reason, ErrorReason::UnsupportedMapping);
    assert_eq!(e.attribute.as_deref(), Some("t"), "{e:?}");

    let e = refusal(|b| {
        b.models
            .get_mut("XmlOracle::Para@default")
            .unwrap()
            .ordered_content = None;
    });
    assert_eq!(e.reason, ErrorReason::InvalidOrderedContent);

    let e = refusal(|b| {
        let m = b.models.get_mut("Doc@default").unwrap();
        m.attributes[0].value_type = TypeRef::Builtin {
            name: BuiltinType::Date,
        };
    });
    // The shared model generator refuses a lexical-codec type before XML planning starts.
    assert_eq!(e.reason, ErrorReason::UnsupportedType);
    assert!(e.format.is_some() || e.model_id.is_some(), "{e:?}");
}

/// 02g golden: Ruby reads `<n xmlns:x="u" xmlns="d" id="1" x:a="2" b="3" xml:lang="en"/>`
/// (model `id` mapped, `other` catch-all) to `{"a"=>"2","b"=>"3","lang"=>"en"}`
/// (prefix stripped, xmlns excluded) and writes each entry back as an unprefixed
/// attribute after the mapped ones.
#[test]
fn any_attribute_reads_and_writes_like_ruby() {
    const ID: &str = "Any@default";
    let xml = r#"<n xmlns:x="u" xmlns="d" id="1" x:a="2" b="3" xml:lang="en"/>"#;
    let read_back = read(ID, xml);
    let model = ok(&read_back);
    assert_eq!(model["id"], "1");
    assert_eq!(model["other"], json!({"a": "2", "b": "3", "lang": "en"}));
    assert_eq!(
        serde_json::to_string(&model["other"]).unwrap(),
        r#"{"a":"2","b":"3","lang":"en"}"#
    );
    let written = write(ID, model);
    assert_eq!(
        ok(&written),
        r#"<n xmlns="d" id="1" a="2" b="3" lang="en"/>"#
    );
    // Nothing unclaimed: the member stays unset, not an empty hash.
    let none = read(ID, r#"<n xmlns="d" id="1"/>"#);
    assert!(ok(&none).get("other").is_none_or(Value::is_null), "{none}");
    // A repeated local name keeps its first position and the later value.
    let dup = read(ID, r#"<n xmlns="d" xmlns:x="u" a="1" b="2" x:a="3"/>"#);
    assert_eq!(
        serde_json::to_string(&ok(&dup)["other"]).unwrap(),
        r#"{"a":"3","b":"2"}"#
    );
    // A hash entry that is not a legal attribute name is refused, not dropped.
    let bad = write(ID, &json!({"id": "1", "other": {"a b": "x"}}));
    assert_eq!(err(&bad)["reason"], "not_serializable", "{bad}");
}

#[test]
fn any_attribute_projection_refuses_what_ruby_refuses() {
    let refused = |edit: &dyn Fn(&mut Value)| {
        let mut raw = serde_json::to_value(full_bundle()).unwrap();
        edit(&mut raw["models"]["Any@default"]);
        let b: Bundle = serde_json::from_value(raw).unwrap();
        lutaml_rs::xml::projection::compile_projection(&b, "Any@default").unwrap_err()
    };
    let e = refused(&|m| {
        let rule = m["mappings"]["xml"]["rules"][1].clone();
        m["mappings"]["xml"]["rules"]
            .as_array_mut()
            .unwrap()
            .push(rule);
    });
    assert_eq!(e.reason, ErrorReason::UnsupportedMapping);
    let e = refused(&|m| m["mappings"]["xml"]["rules"][1]["to"] = json!("id"));
    assert_eq!(e.reason, ErrorReason::UnsupportedMapping);
    let e = refused(&|m| m["mappings"]["xml"]["rules"][1]["name"] = json!(["x"]));
    assert_eq!(e.reason, ErrorReason::UnsupportedMapping);
}

#[test]
fn moxml_binding_probe_finds_every_required_face() {
    rt::probe().unwrap();
}
