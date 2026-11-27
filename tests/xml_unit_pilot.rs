//! Third real pilot: the actual Unit graph (11 models; only `Units::Symbol` has XML hooks). Typed classes and
//! XML codecs are GENERATED from the exact Ruby-exported bundle; the Symbol hook is HANDWRITTEN; the 13 actual
//! source cases are replayed inside the compiled generated consumer.
#![cfg(feature = "xml")]
mod support;
use sha2::{Digest, Sha256};

const BUNDLE: &[u8] = include_bytes!("fixtures/unit/bundle.json");
const CASES: &[u8] = include_bytes!("fixtures/unit/cases.json");
const BUNDLE_CHECKSUM: &str =
    "sha256:ab95bbb6b621843c834a81646378c72c2c1f2b0cd4fed9bb09ba3ea568cec9ad";

fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[test]
fn inputs_are_the_exact_root_artifacts() {
    assert_eq!(
        sha(BUNDLE),
        "75723ce48adc41160d7a5d604c1843f67bf4d982ef43ed79420db742765a38e8"
    );
    assert_eq!(
        sha(CASES),
        "b630fafade58f04a242be82fc517178a5581acc65e8113f680a387f6a247664d"
    );
    let snapshot = lutaml_rs::lml::load(BUNDLE, lutaml_rs::contracts::InputFormat::Json).unwrap();
    assert_eq!(snapshot.checksum(), BUNDLE_CHECKSUM);
    let typed = snapshot.bundle().unwrap();
    assert_eq!(typed.models.len(), 11);
    let hooked: Vec<_> = typed
        .models
        .iter()
        .filter(|(_, m)| {
            m.mappings
                .xml
                .iter()
                .flat_map(|x| &x.rules)
                .any(|r| r.custom.is_some())
        })
        .map(|(id, _)| id.as_str())
        .collect();
    assert_eq!(hooked, ["Unitsml::Model::Units::Symbol@default"]);
}

#[test]
fn generated_consumer_with_the_handwritten_symbol_hook_replays_the_thirteen_cases() {
    let run = support::run_consumer(
        "unit-generated-consumer",
        BUNDLE,
        &[
            ("hooks.rs", include_str!("fixtures/unit/hooks.rs")),
            (
                "pilot_tests.rs",
                include_str!("fixtures/unit/consumer_tests.rs"),
            ),
            ("cases.json", include_str!("fixtures/unit/cases.json")),
        ],
        "pub mod hooks;\n#[cfg(test)]\nmod pilot_tests;\n",
        "\"xml\"",
        &unit_bindings(),
    );
    assert!(run.stdout.contains("UNIT_REPORT"), "{}", run.stdout);
    assert!(run.stdout.contains("2 passed"), "{}", run.stdout);
}

/// Trusted value-type bindings for the two source W3C types (plain strings; no NCName/lang rule is claimed).
fn unit_bindings() -> lutaml_rs::contracts::TypeBindings {
    ["xml:id", "xml:lang"]
        .into_iter()
        .map(|id| {
            (
                id.to_string(),
                lutaml_rs::contracts::TypeBinding {
                    rust_type: "String".into(),
                    codec: None,
                },
            )
        })
        .collect()
}
