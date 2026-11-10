// Actual 09a/09c emitters compile the fixtures; no mirrored Rust model is stored.
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
// The xml module tree is spelled out in build_xml.rs because its `xml`/`generator`
// feature gates belong to the library package, not to this build script.
#[allow(dead_code)]
#[path = "build_xml.rs"]
mod xml;
use contracts::*;
use generator::model_names;
use sha2::{Digest, Sha256};
use std::{env, fmt::Write, fs, path::PathBuf};

/// Set name, fixture file. Each set is its own module so same-named models never collide.
const SETS: [(&str, &str); 3] = [
    ("oracle", "fixtures/oracle.json"),
    ("mml_v3", "fixtures/mml-v3-mi-mixed.json"),
    ("mml_v4", "fixtures/mml-v4-mi-mixed.json"),
];

fn main() {
    println!("cargo:rerun-if-changed=fixtures");
    for path in [
        "../../src/generator",
        "../../src/codecs",
        "../../src/lml",
        "../../src/xml",
    ] {
        println!("cargo:rerun-if-changed={path}");
    }
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let mut wiring = String::new();
    let mut read_arms = String::new();
    let mut write_arms = String::new();
    let mut identity = String::new();
    let mut sets_json = serde_json::Map::new();
    for (set, file) in SETS {
        println!("cargo:rerun-if-changed={file}");
        let input = fs::read(file).unwrap();
        let snapshot = lml::load(&input, InputFormat::Json).unwrap();
        let bundle = snapshot.bundle().unwrap();
        let models = generator::RustEmitter
            .generate_models(&bundle, &TypeBindings::new())
            .unwrap();
        let codecs = xml::RustXmlEmitter
            .generate_xml_codecs(&bundle, &TypeBindings::new())
            .unwrap();
        for (name, source) in [
            ("models.rs", &models["models.rs"]),
            ("xml_codecs.rs", &codecs["xml_codecs.rs"]),
        ] {
            fs::write(out.join(format!("{set}_{name}")), source).unwrap();
        }
        writeln!(wiring,
            "#[allow(dead_code, unused, clippy::all)]\npub mod {set} {{\n    pub mod contracts {{ pub use lutaml_rs::contracts::*; }}\n    pub mod values {{ pub use lutaml_rs::values::*; }}\n    pub mod xml_rt {{ pub use lutaml_rs::xml::*; }}\n    #[path = {:?}] pub mod models;\n    #[path = {:?}] pub mod xml_codecs;\n}}",
            out.join(format!("{set}_models.rs")), out.join(format!("{set}_xml_codecs.rs"))).unwrap();
        let names = model_names(&bundle).unwrap();
        for (id, model) in &bundle.models {
            if model.mappings.xml.is_none() {
                continue;
            }
            let n = &names[id];
            writeln!(read_arms, "({set:?}, {id:?}) => {set}::xml_codecs::read_{n}_xml(xml).and_then(|m| to_json(&m)),").unwrap();
            writeln!(write_arms, "({set:?}, {id:?}) => from_json::<{set}::models::{n}>(data).and_then(|m| {set}::xml_codecs::write_{n}_xml(&m)),").unwrap();
        }
        writeln!(identity, "{set}={}", snapshot.checksum()).unwrap();
        sets_json.insert(
            set.into(),
            serde_json::Value::String(snapshot.checksum().into()),
        );
    }
    let combined = format!("sha256:{:x}", Sha256::digest(identity.as_bytes()));
    writeln!(wiring, "pub const LML_CHECKSUM: &str = {combined:?};").unwrap();
    writeln!(
        wiring,
        "pub const SET_CHECKSUMS: &str = {:?};",
        serde_json::to_string(&serde_json::Value::Object(sets_json)).unwrap()
    )
    .unwrap();
    writeln!(wiring, "fn dispatch_read(set: &str, model: &str, xml: &[u8]) -> Option<ContractResult<String>> {{\n    Some(match (set, model) {{\n{read_arms}        _ => return None,\n    }})\n}}").unwrap();
    writeln!(wiring, "fn dispatch_write(set: &str, model: &str, data: &str) -> Option<ContractResult<String>> {{\n    Some(match (set, model) {{\n{write_arms}        _ => return None,\n    }})\n}}").unwrap();
    fs::write(out.join("wiring.rs"), wiring).unwrap();
    fs::write(
        out.join("identity.json"),
        serde_json::to_vec(&serde_json::json!({"lml_checksum": combined})).unwrap(),
    )
    .unwrap();
}
