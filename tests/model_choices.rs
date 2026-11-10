//! Source-produced class-choice oracle; validation is explicit, codecs defer it.
#![cfg(feature = "xml")]
use lutaml_rs::codecs::RustJsonEmitter;
use lutaml_rs::contracts::*;
use lutaml_rs::generator::{model_names, RustEmitter};
use lutaml_rs::xml::RustXmlEmitter;
use serde_json::Value;
use std::io::Write;
use std::process::{Command, Stdio};

#[test]
fn generated_choices_preserve_source_validation_boundaries() {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/model_choices.json")).unwrap();
    let bytes = serde_json::to_vec(&fixture["bundle"]).unwrap();
    let bundle = lutaml_rs::lml::load_bundle(&bytes, InputFormat::Json).unwrap();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let dir = root.join("target/choice-generated-consumer");
    std::fs::create_dir_all(dir.join("src")).unwrap();
    let bindings = TypeBindings::new();
    for (name, files) in [
        (
            "models.rs",
            RustEmitter.generate_models(&bundle, &bindings).unwrap(),
        ),
        (
            "json_codecs.rs",
            RustJsonEmitter
                .generate_json_codecs(&bundle, &bindings)
                .unwrap(),
        ),
        (
            "xml_codecs.rs",
            RustXmlEmitter
                .generate_xml_codecs(&bundle, &bindings)
                .unwrap(),
        ),
    ] {
        std::fs::write(dir.join("src").join(name), &files[name]).unwrap();
    }
    let names = model_names(&bundle).unwrap();
    let mut arms = String::new();
    for (id, name) in &names {
        arms += &format!(
            r#"{id:?} => {{
            let mut m:models::{name}=serde_json::from_value(row["input"].clone()).unwrap();m.apply_defaults().unwrap();
            let codes=|m:&models::{name}|match m.validate(){{Ok(())=>vec![],Err(e)=>vec![e.reason]}};
            assert_eq!(serde_json::to_value(codes(&m)).unwrap(),row["expected"]["construct"],"construct {{}}",row["id"]);
            let raw=json_codecs::read_{name}_json(&row["input"].to_string()).unwrap();
            assert_eq!(serde_json::to_value(codes(&raw)).unwrap(),row["expected"]["json_input"],"JSON read {{}}",row["id"]);
            let wire=json_codecs::write_{name}_json(&m).unwrap();
            assert_eq!(serde_json::from_str::<Value>(&wire).unwrap(),row["expected"]["json_wire"],"JSON wire {{}}",row["id"]);
            let reread=json_codecs::read_{name}_json(&wire).unwrap();
            assert_eq!(serde_json::to_value(codes(&reread)).unwrap(),row["expected"]["json_roundtrip"],"JSON reread {{}}",row["id"]);
            let xml=xml_codecs::write_{name}_xml(&m).unwrap();
            let reread=xml_codecs::read_{name}_xml(row["expected"]["xml_wire"].as_str().unwrap().as_bytes()).unwrap();
            assert_eq!(serde_json::to_value(codes(&reread)).unwrap(),row["expected"]["xml_roundtrip"],"XML reread {{}}",row["id"]);
            let own=xml_codecs::read_{name}_xml(xml.as_bytes()).unwrap();
            assert_eq!(serde_json::to_value(&own).unwrap(),serde_json::to_value(&reread).unwrap(),"XML wire {{}}",row["id"]);
        }},"#
        );
    }
    let main = format!(
        r#"use choice_consumer::{{models,json_codecs,xml_codecs}};use serde_json::Value;use std::io::Read;
fn main(){{let mut input=String::new();std::io::stdin().read_to_string(&mut input).unwrap();let rows:Vec<Value>=serde_json::from_str(&input).unwrap();for row in &rows{{match row["model_id"].as_str().unwrap(){{{arms}_=>panic!("unknown model")}}}}println!("{{}} cases passed",rows.len());}}"#
    );
    std::fs::write(dir.join("src/main.rs"), main).unwrap();
    std::fs::write(dir.join("src/lib.rs"),"pub mod contracts {pub use lutaml_rs::contracts::*;}\npub mod values {pub use lutaml_rs::values::*;}\npub mod xml_rt {pub use lutaml_rs::xml::*;}\npub mod models;pub mod json_codecs;pub mod xml_codecs;\n").unwrap();
    std::fs::write(dir.join("Cargo.toml"),format!("[package]\nname=\"choice-consumer\"\nversion=\"0.0.0\"\nedition=\"2021\"\n[workspace]\n[dependencies]\nlutaml-rs={{path={:?},features=[\"xml\"]}}\nserde={{version=\"=1.0.219\",features=[\"derive\"]}}\nserde_json={{version=\"=1.0.140\",features=[\"arbitrary_precision\"]}}\nnum-bigint=\"=0.4.6\"\nnum-rational=\"=0.4.2\"\nnum-traits=\"=0.2.19\"\n",root.to_str().unwrap())).unwrap();
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
        "consumer build: {}",
        String::from_utf8_lossy(&build.stderr)
    );
    let mut child = Command::new(target.join("debug/choice-consumer"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(fixture["cases"].to_string().as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "consumer failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    println!("{}", String::from_utf8_lossy(&out.stdout));
}

#[test]
fn choice_metadata_is_strict_and_capability_gated() {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/model_choices.json")).unwrap();
    let read =
        |v: &Value| lutaml_rs::lml::load_bundle(&serde_json::to_vec(v).unwrap(), InputFormat::Json);
    let original = &fixture["bundle"];
    for (mutation, reason) in [
        ("version", ErrorReason::UnsupportedVersion),
        ("phase", ErrorReason::InvalidDocument),
        ("member", ErrorReason::DanglingReference),
        ("capability", ErrorReason::CapabilityMissing),
    ] {
        let mut bundle = original.clone();
        if mutation == "capability" {
            bundle["capabilities"]
                .as_array_mut()
                .unwrap()
                .retain(|v| v != "model.choices.v1");
        } else {
            let model = bundle["models"]
                .as_object_mut()
                .unwrap()
                .values_mut()
                .find(|m| m.get("choices").is_some())
                .unwrap();
            match mutation {
                "version" => model["choices"]["version"] = 2.into(),
                "phase" => model["choices"]["validation"] = "constructor".into(),
                _ => {
                    model["choices"]["groups"][0]["members"][0] =
                        serde_json::json!({"attribute":"missing"})
                }
            }
        }
        assert_eq!(read(&bundle).unwrap_err().reason, reason, "{mutation}");
    }
}
