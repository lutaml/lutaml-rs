use lutaml_rs::codecs::RustJsonEmitter;
use lutaml_rs::contracts::*;
use lutaml_rs::generator::RustEmitter;
#[test]
fn rust_reads_ruby_portable_output() {
    // Golden owned by lutaml-model (spec/fixtures/lml_conformance/ruby_portable_interop.json),
    // copied byte-identical: an Lml bundle plus Lml.portable_json text of three records.
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/ruby_portable_interop.json")).unwrap();
    let bundle = lutaml_rs::lml::load_bundle(
        serde_json::to_string(&fixture["bundle"])
            .unwrap()
            .as_bytes(),
        InputFormat::Json,
    )
    .unwrap();
    let outputs: Vec<String> = fixture["outputs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o.as_str().unwrap().to_string())
        .collect();
    assert_eq!(outputs.len(), 3);
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let dir = root.join("target/ruby-interop-consumer");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("src")).unwrap();
    let models = RustEmitter
        .generate_models(&bundle, &TypeBindings::new())
        .unwrap();
    let json = RustJsonEmitter
        .generate_json_codecs(&bundle, &TypeBindings::new())
        .unwrap();
    let codecs = &json["json_codecs.rs"];
    let read = codecs
        .split("fn read_")
        .skip(1)
        .map(|s| s.split('(').next().unwrap().to_string())
        .find(|n| n.contains("Rec") && n.ends_with("_json"))
        .expect("no Rec reader");
    let name = read.trim_end_matches("_json").to_string();
    println!("reader read_{read}");
    std::fs::write(dir.join("src/models.rs"), &models["models.rs"]).unwrap();
    std::fs::write(dir.join("src/json_codecs.rs"), codecs).unwrap();
    std::fs::write(
        dir.join("src/values.rs"),
        include_str!("../src/values/mod.rs"),
    )
    .unwrap();
    let mut tail =
        String::from("#[cfg(test)] mod t { use super::json_codecs::*; #[test] fn interop() {\n");
    for o in &outputs {
        tail += &format!("{{ let w={:?}; let m=read_{name}_json(w).unwrap(); let back=write_{name}_json(&m).unwrap(); assert_eq!(back, w); }}\n", o);
    }
    tail += &format!("assert!(read_{name}_json(r#\"{{\"count\":5}}\"#).is_err()); }} }}\n");
    std::fs::write(dir.join("src/lib.rs"), format!("pub mod contracts {{ pub use lutaml_rs::contracts::*; }}\npub mod values;\npub mod models;\npub mod json_codecs;\n{tail}")).unwrap();
    std::fs::write(dir.join("Cargo.toml"), format!("[package]\nname=\"ruby-interop-consumer\"\nversion=\"0.0.0\"\nedition=\"2021\"\n[dependencies]\nlutaml-rs={{path={}}}\nserde={{version=\"=1.0.219\",features=[\"derive\"]}}\nserde_json={{version=\"=1.0.140\",features=[\"arbitrary_precision\"]}}\nnum-bigint=\"=0.4.6\"\nnum-rational=\"=0.4.2\"\nnum-traits=\"=0.2.19\"\n", serde_json::to_string(root.to_str().unwrap()).unwrap())).unwrap();
    let r = std::process::Command::new(env!("CARGO"))
        .args(["test", "--offline", "--manifest-path"])
        .arg(dir.join("Cargo.toml"))
        .env("CARGO_BUILD_JOBS", "1")
        .env("CARGO_TARGET_DIR", root.join("target/json-consumer-build"))
        .output()
        .unwrap();
    let out = format!(
        "{}\n{}",
        String::from_utf8_lossy(&r.stdout),
        String::from_utf8_lossy(&r.stderr)
    );
    assert!(r.status.success(), "{out}");
    assert!(out.contains("1 passed"), "{out}");
}
