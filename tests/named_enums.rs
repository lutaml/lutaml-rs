//! Actual OMML scalar declarations and native XML corpus, not inferred XSD types.
#![cfg(feature = "xml")]
use lutaml_rs::codecs::RustJsonEmitter;
use lutaml_rs::contracts::*;
use lutaml_rs::generator::{model_names, RustEmitter};
use lutaml_rs::xml::RustXmlEmitter;
use serde_json::Value;
use std::io::Write;
use std::process::{Command, Stdio};

#[test]
fn declared_string_enums_generate_and_execute_xml_codecs() {
    replay_enum_corpus(
        include_bytes!("fixtures/omml_enum_bundle.json"),
        include_bytes!("fixtures/omml_enum_corpus.json"),
        "Omml::Models::CTBr@omml",
        &TypeBindings::new(),
    );
    replay_enum_corpus(
        include_bytes!("fixtures/enum_family_bundle.json"),
        include_bytes!("fixtures/enum_family_corpus.json"),
        "OmmlPortableEnumFamilyProbe@omml",
        &TypeBindings::new(),
    );

    // A caller can override XML lexical handling without replacing the
    // declaration or the binding's serde representation.
    let bytes = include_bytes!("fixtures/omml_enum_bundle.json");
    let bundle = lutaml_rs::lml::load_bundle(bytes, InputFormat::Json).unwrap();
    let id = "Omml::Models::STBrType@omml";
    let mut bindings = TypeBindings::from([(
        id.into(),
        TypeBinding {
            rust_type: "crate::lexical::BreakType".into(),
            codec: Some("xml:crate::lexical".into()),
        },
    )]);
    RustJsonEmitter
        .generate_json_codecs(&bundle, &bindings)
        .unwrap();
    let mut corpus: Value =
        serde_json::from_slice(include_bytes!("fixtures/omml_enum_corpus.json")).unwrap();
    let mut row = corpus["rows"][1].clone();
    row["input"] = row["input"]
        .as_str()
        .unwrap()
        .replace("=\"page\"", "=\" page \"")
        .into();
    corpus["rows"] = serde_json::json!([row]);
    replay_enum_corpus(
        bytes,
        &serde_json::to_vec(&corpus).unwrap(),
        "Omml::Models::CTBr@omml",
        &bindings,
    );
    bindings.get_mut(id).unwrap().codec = Some("xml:crate::lexical;invalid".into());
    assert_eq!(
        RustXmlEmitter
            .generate_xml_codecs(&bundle, &bindings)
            .unwrap_err()
            .reason,
        ErrorReason::UnsupportedMapping,
    );
}

fn replay_enum_corpus(
    bundle_bytes: &[u8],
    corpus_bytes: &[u8],
    model_id: &str,
    bindings: &TypeBindings,
) {
    let bundle = lutaml_rs::lml::load_bundle(bundle_bytes, InputFormat::Json).unwrap();
    let original = bundle.clone();
    let models = RustEmitter.generate_models(&bundle, bindings).unwrap();
    let xml = RustXmlEmitter
        .generate_xml_codecs(&bundle, bindings)
        .unwrap();
    assert_eq!(
        bundle, original,
        "compiler realization must not mutate declaration identity"
    );
    let names = model_names(&bundle).unwrap();
    let name = &names[model_id];
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let dir = root.join("target/enum-generated-consumer");
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("src/models.rs"), &models["models.rs"]).unwrap();
    std::fs::write(dir.join("src/xml_codecs.rs"), &xml["xml_codecs.rs"]).unwrap();
    std::fs::write(
        dir.join("src/lib.rs"),
        r#"
pub use lutaml_rs::{contracts, values};
pub use lutaml_rs::xml as xml_rt;
pub mod lexical;
pub mod models;
pub mod xml_codecs;
"#,
    )
    .unwrap();
    std::fs::write(
        dir.join("src/lexical.rs"),
        r#"
#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum BreakType { Page, Column, TextWrapping }
pub fn parse(text: &str) -> Result<Option<BreakType>, serde_json::Error> {
    serde_json::from_value(serde_json::Value::String(text.trim().into())).map(Some)
}
pub fn format(value: &BreakType) -> Result<String, std::convert::Infallible> {
    Ok(match value { BreakType::Page => "page", BreakType::Column => "column",
        BreakType::TextWrapping => "textWrapping" }.into())
}
"#,
    )
    .unwrap();
    let main = format!(
        r#"use enum_consumer::xml_codecs;use serde_json::Value;use std::io::Read;
fn tree(xml:&str)->Value{{
 let doc=enum_consumer::xml_rt::moxml_binding::moxml::document::Document::parse(xml.as_bytes()).unwrap();let rows=doc.element_rows();assert_eq!(rows.len(),1,"empty-element schema");let root=&rows[0];assert!(root.content.is_empty(),"unexpected XML content");
 let mut attrs=root.attributes.iter().map(|a|(a.name.namespace.as_deref().unwrap_or("").to_string(),a.name.local.clone(),a.value.clone())).collect::<Vec<_>>();attrs.sort();serde_json::json!([[root.name.namespace.as_deref().unwrap_or(""),&root.name.local],attrs])
}}
fn main(){{let mut input=String::new();std::io::stdin().read_to_string(&mut input).unwrap();let corpus:Value=serde_json::from_str(&input).unwrap();let mut count=0;
for row in corpus["rows"].as_array().unwrap(){{let model=xml_codecs::read_{name}_xml(row["input"].as_str().unwrap().as_bytes()).unwrap();assert_eq!(serde_json::to_value(&model).unwrap(),row["values"],"{{}} model",row["id"]);let wire=xml_codecs::write_{name}_xml(&model).unwrap();assert_eq!(tree(&wire),tree(row["expected"].as_str().unwrap()),"{{}} XML QName tree",row["id"]);let again=xml_codecs::read_{name}_xml(wire.as_bytes()).unwrap();assert_eq!(model,again,"{{}} reread",row["id"]);count+=1;}}
for row in corpus["refusals"].as_array().unwrap(){{assert!(xml_codecs::read_{name}_xml(row["input"].as_str().unwrap().as_bytes()).is_err());count+=1;}}println!("{{count}} OMML enum cases passed");}}"#
    );
    std::fs::write(dir.join("src/main.rs"), main).unwrap();
    std::fs::write(dir.join("Cargo.toml"),format!("[package]\nname=\"enum-consumer\"\nversion=\"0.0.0\"\nedition=\"2021\"\n[workspace]\n[dependencies]\nlutaml-rs={{path={:?},features=[\"xml\"]}}\nserde={{version=\"=1.0.219\",features=[\"derive\"]}}\nserde_json={{version=\"=1.0.140\",features=[\"arbitrary_precision\"]}}\nnum-bigint=\"=0.4.6\"\nnum-rational=\"=0.4.2\"\nnum-traits=\"=0.2.19\"\n",root.to_str().unwrap())).unwrap();
    let target = root.join("target/choice-consumer-build");
    let build = Command::new(env!("CARGO"))
        .args(["build", "--offline", "--manifest-path"])
        .arg(dir.join("Cargo.toml"))
        .env("CARGO_TARGET_DIR", &target)
        .env("CARGO_BUILD_JOBS", "2")
        .output()
        .unwrap();
    assert!(
        build.status.success(),
        "consumer compile: {}",
        String::from_utf8_lossy(&build.stderr)
    );
    let mut child = Command::new(target.join("debug/enum-consumer"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(corpus_bytes).unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "consumer: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    println!("{}", String::from_utf8_lossy(&out.stdout));
}

#[test]
fn unsupported_named_semantics_still_require_explicit_bindings() {
    let mut bundle = lutaml_rs::lml::load_bundle(
        include_bytes!("fixtures/omml_enum_bundle.json"),
        InputFormat::Json,
    )
    .unwrap();
    bundle
        .value_types
        .values_mut()
        .next()
        .unwrap()
        .facets
        .insert("source_binding_v1".into(), NeutralValue::Boolean(true));
    assert_eq!(
        RustEmitter
            .generate_models(&bundle, &TypeBindings::new())
            .unwrap_err()
            .reason,
        ErrorReason::MissingTypeBinding
    );
}

#[test]
fn malformed_closed_string_enumerations_are_refused() {
    let mut bundle = lutaml_rs::lml::load_bundle(
        include_bytes!("fixtures/omml_enum_bundle.json"),
        InputFormat::Json,
    )
    .unwrap();
    bundle
        .value_types
        .values_mut()
        .next()
        .unwrap()
        .facets
        .insert(
            "enumeration".into(),
            NeutralValue::Array(vec![NeutralValue::Signed(1)]),
        );
    assert_eq!(
        RustEmitter
            .generate_models(&bundle, &TypeBindings::new())
            .unwrap_err()
            .reason,
        ErrorReason::InvalidDocument
    );
}
