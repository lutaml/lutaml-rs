//! Second real pilot: the actual Plurimath `Uncertainty` graph (4 models, 40 rules / 25 hook rules on the
//! root). Typed classes and json codecs are GENERATED from the exact Ruby-exported bundle by the public
//! artifact API; the 25 hooks are HANDWRITTEN (`fixtures/uncertainty/hooks.rs`); the 10 actual source cases
//! are replayed inside the compiled generated consumer. No math writer or evaluator is claimed.
use lutaml_rs::artifacts::Artifacts;
use lutaml_rs::contracts::InputFormat;
use lutaml_rs::lml::load;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

const BUNDLE: &[u8] = include_bytes!("fixtures/uncertainty/bundle.json");
const CASES: &[u8] = include_bytes!("fixtures/uncertainty/cases.json");
const HOOKS: &str = include_str!("fixtures/uncertainty/hooks.rs");
const CONSUMER: &str = include_str!("fixtures/uncertainty/consumer_tests.rs");
const BUNDLE_CHECKSUM: &str =
    "sha256:34354ce6b8ea0e8fd9d3ca91a6e29774e4fc0a65e1bdab92a5586224ea1d154e";

fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[test]
fn inputs_are_the_exact_root_artifacts_and_the_graph_is_the_finite_four_model_one() {
    assert_eq!(
        sha(BUNDLE),
        "a33485b50c90bd55add555470ab6e249da367991d4cc7c021934c6c73442fe10"
    );
    assert_eq!(
        sha(CASES),
        "3d07c837713f0190cbb4aa5062721b5f09a96e580628564de1e38143f7b220a0"
    );
    let snapshot = load(BUNDLE, InputFormat::Json).unwrap();
    assert_eq!(snapshot.checksum(), BUNDLE_CHECKSUM);
    let typed = snapshot.bundle().unwrap();
    assert_eq!(typed.models.len(), 4);
    let root = &typed.models["Plurimath::Math::Function::Uncertainty@default"];
    let json = root.mappings.json.as_ref().unwrap();
    assert_eq!(json.rules.len(), 40);
    assert_eq!(json.rules.iter().filter(|r| r.custom.is_some()).count(), 25);
    // toml/yaml declarations are kept as data (no codec is generated for them).
    assert_eq!(root.mappings.toml.as_ref().unwrap().rules.len(), 40);
    assert_eq!(root.mappings.yaml.as_ref().unwrap().rules.len(), 40);
    for (id, model) in &typed.models {
        let hooks = model
            .mappings
            .json
            .iter()
            .flat_map(|j| &j.rules)
            .filter(|r| r.custom.is_some())
            .count();
        assert_eq!(
            hooks,
            if id.ends_with("Function::Uncertainty@default") {
                25
            } else {
                0
            },
            "{id}"
        );
    }
}

#[test]
fn generated_consumer_with_handwritten_hooks_replays_the_ten_actual_cases() {
    let snapshot = load(BUNDLE, InputFormat::Json).unwrap();
    let artifacts = Artifacts::generate(&snapshot).unwrap();
    assert_eq!(artifacts.manifest.lml_checksum, BUNDLE_CHECKSUM);
    let again = Artifacts::generate(&snapshot).unwrap();
    assert_eq!(artifacts.files, again.files, "generation is deterministic");
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let dir = root.join("target/uncertainty-generated-consumer");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("src")).unwrap();
    for (path, source) in &artifacts.files {
        if path != "manifest.json" {
            std::fs::write(dir.join("src").join(path), source).unwrap();
        }
    }
    std::fs::write(dir.join("src/hooks.rs"), HOOKS).unwrap();
    std::fs::write(dir.join("src/pilot_tests.rs"), CONSUMER).unwrap();
    std::fs::write(dir.join("src/cases.json"), CASES).unwrap();
    let mut lib = std::fs::read_to_string(dir.join("src/mod.rs")).unwrap();
    lib.push_str("pub mod hooks;\n#[cfg(test)]\nmod pilot_tests;\n");
    std::fs::write(dir.join("src/lib.rs"), lib).unwrap();
    std::fs::remove_file(dir.join("src/mod.rs")).unwrap();
    std::fs::write(
        dir.join("Cargo.toml"),
        format!(
            "[package]\nname=\"uncertainty-generated-consumer\"\nversion=\"0.0.0\"\nedition=\"2021\"\n[dependencies]\nlutaml-rs={{path={}}}\nserde={{version=\"=1.0.219\",features=[\"derive\"]}}\nserde_json={{version=\"=1.0.140\",features=[\"arbitrary_precision\",\"preserve_order\"]}}\nnum-bigint=\"=0.4.6\"\nnum-rational=\"=0.4.2\"\nnum-traits=\"=0.2.19\"\n",
            serde_json::to_string(root.to_str().unwrap()).unwrap()
        ),
    )
    .unwrap();
    // Pins of the generated artifact set next to the report.
    let pins: BTreeMap<_, _> = artifacts
        .manifest
        .files
        .iter()
        .map(|f| (f.path.clone(), f.sha256.clone()))
        .collect();
    std::fs::write(
        dir.join("artifact-pins.json"),
        serde_json::to_vec_pretty(&pins).unwrap(),
    )
    .unwrap();
    let result = std::process::Command::new(env!("CARGO"))
        .args(["test", "--offline", "--manifest-path"])
        .arg(dir.join("Cargo.toml"))
        .args(["--", "--nocapture", "--test-threads=1"])
        .env("CARGO_BUILD_JOBS", "1")
        .env(
            "CARGO_TARGET_DIR",
            root.join("target/uncertainty-consumer-build"),
        )
        .output()
        .unwrap();
    let log = [result.stdout.as_slice(), result.stderr.as_slice()].concat();
    std::fs::write(dir.join("test-output.log"), &log).unwrap();
    let stdout = String::from_utf8_lossy(&result.stdout);
    assert!(
        result.status.success(),
        "generated consumer failed:\n{stdout}\n{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(
        stdout.contains("PILOT_REPORT cases=10 tree_equal=10 byte_equal="),
        "{stdout}"
    );
    assert!(stdout.contains("6 passed"), "{stdout}");
}
