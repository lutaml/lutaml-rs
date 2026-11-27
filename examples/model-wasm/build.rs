// Actual 08c/08e emitters compile the fixture; no mirrored Rust model is stored.
#[allow(dead_code)]
#[path = "../../src/codecs/mod.rs"]
mod codecs;
#[allow(dead_code)]
#[path = "../../src/contracts.rs"]
mod contracts;
#[allow(dead_code)]
#[path = "../../src/generator/mod.rs"]
mod generator;
#[allow(dead_code)]
#[path = "../../src/lml/mod.rs"]
mod lml;

#[allow(dead_code)]
#[path = "../../src/values/mod.rs"]
mod values;
use contracts::*;
use std::{env, fs, path::PathBuf};
fn main() {
    for path in [
        "fixture.json",
        "../../src/generator",
        "../../src/codecs",
        "../../src/lml",
    ] {
        println!("cargo:rerun-if-changed={path}");
    }
    let input = fs::read("fixture.json").unwrap();
    let snapshot = lml::load(&input, InputFormat::Json).unwrap();
    let bundle = snapshot.bundle().unwrap();
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    for (path, source) in generator::RustEmitter
        .generate_models(&bundle, &TypeBindings::new())
        .unwrap()
        .into_iter()
        .chain(
            codecs::RustJsonEmitter
                .generate_json_codecs(&bundle, &TypeBindings::new())
                .unwrap(),
        )
    {
        assert!(!path.contains('/') && !path.contains('\\'));
        fs::write(out.join(path), source).unwrap();
    }
    fs::write(out.join("wiring.rs"), format!(
        "#[allow(dead_code)]\n#[path = {:?}] mod models;\n#[allow(dead_code)]\n#[path = {:?}] mod json_codecs;\n",
        out.join("models.rs"), out.join("json_codecs.rs")
    )).unwrap();
    fs::write(
        out.join("identity.json"),
        serde_json::to_vec(&serde_json::json!({"lml_checksum":snapshot.checksum()})).unwrap(),
    )
    .unwrap();
    println!("cargo:rustc-env=LML_CHECKSUM={}", snapshot.checksum());
}
