//! Shared harness: generate a consumer crate from a bundle with the public artifact API, add handwritten
//! files, compile and run its tests against the pinned moxml facade.
use lutaml_rs::artifacts::Artifacts;
use lutaml_rs::contracts::InputFormat;
use lutaml_rs::lml::load;

pub struct Run {
    pub stdout: String,
}

/// `extra`: (file name, text) written under `src/`; `tail`: appended to the generated `mod.rs` (the consumer
/// crate root). Returns the test binary's stdout; asserts a clean exit.
pub fn run_consumer(
    crate_name: &str,
    bundle: &[u8],
    extra: &[(&str, &str)],
    tail: &str,
    features: &str,
    bindings: &lutaml_rs::contracts::TypeBindings,
) -> Run {
    let result = consumer_output(crate_name, bundle, extra, tail, features, bindings);
    let stdout = String::from_utf8_lossy(&result.stdout).to_string();
    assert!(
        result.status.success(),
        "generated consumer failed:\n{stdout}\n{}",
        String::from_utf8_lossy(&result.stderr)
    );
    Run { stdout }
}

/// As [`run_consumer`] but returns the raw process output (for expected compile failures).
pub fn consumer_output(
    crate_name: &str,
    bundle: &[u8],
    extra: &[(&str, &str)],
    tail: &str,
    features: &str,
    bindings: &lutaml_rs::contracts::TypeBindings,
) -> std::process::Output {
    let snapshot = load(bundle, InputFormat::Json).unwrap();
    let artifacts = Artifacts::generate_with(&snapshot, bindings).unwrap();
    assert_eq!(
        artifacts.files,
        Artifacts::generate_with(&snapshot, bindings).unwrap().files,
        "deterministic"
    );
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let dir = root.join("target").join(crate_name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("src")).unwrap();
    for (path, source) in &artifacts.files {
        if path != "manifest.json" && path != "mod.rs" {
            std::fs::write(dir.join("src").join(path), source).unwrap();
        }
    }
    for (name, text) in extra {
        std::fs::write(dir.join("src").join(name), text).unwrap();
    }
    let mut lib = artifacts.files["mod.rs"].clone();
    lib.push_str(tail);
    std::fs::write(dir.join("src/lib.rs"), lib).unwrap();
    std::fs::write(
        dir.join("Cargo.toml"),
        format!(
            "[package]\nname=\"{crate_name}\"\nversion=\"0.0.0\"\nedition=\"2021\"\n[dependencies]\nlutaml-rs={{path={},features=[{features}]}}\nserde={{version=\"=1.0.219\",features=[\"derive\"]}}\nserde_json={{version=\"=1.0.140\",features=[\"arbitrary_precision\",\"preserve_order\"]}}\nnum-bigint=\"=0.4.6\"\nnum-rational=\"=0.4.2\"\nnum-traits=\"=0.2.19\"\n",
            serde_json::to_string(root.to_str().unwrap()).unwrap()
        ),
    )
    .unwrap();
    let pins: Vec<(String, String)> = artifacts
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
            root.join("target/xml-hooks-consumer-build"),
        )
        .output()
        .unwrap();
    let log = [result.stdout.as_slice(), result.stderr.as_slice()].concat();
    std::fs::write(dir.join("test-output.log"), &log).unwrap();
    result
}
