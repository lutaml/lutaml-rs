//! Hostile bundle strings in every string slot must reach the generated crate as
//! data: they compile, and read/write/default values round-trip unchanged.
#![cfg(feature = "generator")]
use lutaml_rs::{artifacts::Artifacts, lml, InputFormat};
use serde_json::{json, Value};
use std::{fs, path::Path, process::Command};

const HOSTILE: [&str; 7] = [
    "a\"#",
    "*/ fn evil(){}",
    "\\u{41}",
    "line1\nline2\r\n",
    "nul\u{0}byte",
    "\"; evil(); \"",
    "{}{0}%s \\ ' //",
];

#[test]
fn hostile_strings_in_keys_enum_values_and_defaults_round_trip_as_data() {
    let mut attributes = Vec::new();
    let mut rules = Vec::new();
    for i in 0..HOSTILE.len() {
        let name = format!("s{i}");
        attributes.push(
            json!({"name":name,"type":{"kind":"builtin","name":"string"},"required":false,
            "collection":false,"default":{"present":true,"value":HOSTILE[(i + 1) % HOSTILE.len()]},
            "initialize_empty":false,"values":HOSTILE}),
        );
        rules.push(json!({"key":[HOSTILE[i]],"to":name}));
    }
    let wire = json!({"lml_bundle":1,"roots":["Host@default"],"models":{"Host@default":{"lml":2,"model":"Host",
        "register":"default","attributes":attributes,"mappings":{"json":{"rules":rules}}}},
        "value_types":{},"capabilities":["json"]});
    let snapshot = lml::load(&serde_json::to_vec(&wire).unwrap(), InputFormat::Json).unwrap();
    let artifacts = Artifacts::generate(&snapshot).unwrap();

    let temp = std::env::temp_dir().join(format!("lutaml-hostile-{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp);
    fs::create_dir_all(temp.join("src")).unwrap();
    artifacts.write(&temp.join("src/generated")).unwrap();
    // Expected data: each key carries its default; also an explicit input per key.
    let defaults: serde_json::Map<String, Value> = (0..HOSTILE.len())
        .map(|i| {
            (
                HOSTILE[i].to_string(),
                json!(HOSTILE[(i + 1) % HOSTILE.len()]),
            )
        })
        .collect();
    let explicit: serde_json::Map<String, Value> = (0..HOSTILE.len())
        .map(|i| {
            (
                HOSTILE[i].to_string(),
                json!(HOSTILE[(i + 2) % HOSTILE.len()]),
            )
        })
        .collect();
    fs::write(
        temp.join("defaults.json"),
        serde_json::to_vec(&defaults).unwrap(),
    )
    .unwrap();
    fs::write(
        temp.join("explicit.json"),
        serde_json::to_vec(&explicit).unwrap(),
    )
    .unwrap();
    let path = serde_json::to_string(env!("CARGO_MANIFEST_DIR")).unwrap();
    fs::write(temp.join("Cargo.toml"), format!("[package]\nname=\"hostile-consumer\"\nversion=\"0.0.0\"\nedition=\"2021\"\n[dependencies]\nlutaml-rs={{path={path},default-features=false,features=[\"runtime\"]}}\nserde={{version=\"=1.0.219\",features=[\"derive\"]}}\nserde_json={{version=\"=1.0.140\",features=[\"arbitrary_precision\"]}}\n")).unwrap();
    fs::write(
        temp.join("src/main.rs"),
        r##"mod generated;
use generated::json_codecs::{read_Host_json, write_Host_json};
fn norm(s: &str) -> serde_json::Value { serde_json::from_str(s).unwrap() }
fn main() {
    let defaults = include_str!("../defaults.json");
    let explicit = include_str!("../explicit.json");
    let mut d = read_Host_json("{}").unwrap();
    d.apply_defaults().unwrap();
    // The writer elides values equal to their default, so compare against the model
    // read from the same data written out explicitly.
    assert_eq!(d, read_Host_json(defaults).unwrap(), "defaults");
    let e = read_Host_json(explicit).unwrap();
    assert_eq!(norm(&write_Host_json(&e).unwrap()), norm(explicit), "explicit");
}
"##,
    )
    .unwrap();
    let result = Command::new(env!("CARGO"))
        .args(["run", "--offline", "--quiet"])
        .current_dir(&temp)
        .env(
            "CARGO_TARGET_DIR",
            Path::new(env!("CARGO_MANIFEST_DIR")).join("target/cli-consumer"),
        )
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&result.stderr).to_string();
    let _ = fs::remove_dir_all(&temp);
    assert!(result.status.success(), "{stderr}");
}
