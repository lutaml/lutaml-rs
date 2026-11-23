//! XML serialization hooks on the shared tiny fixture (SymbolLike content hook, Mixed element hooks).
#![cfg(feature = "xml")]
mod support;

const BUNDLE: &[u8] = include_bytes!("fixtures/xml_hooks/bundle.json");
const CHECKSUM: &str = "sha256:a8521cab2de342127d6e109d21b6ad531cd946893133afc4710666f27756d613";

#[test]
fn the_shared_fixture_has_the_producer_checksum() {
    let snapshot = lutaml_rs::lml::load(BUNDLE, lutaml_rs::contracts::InputFormat::Json).unwrap();
    assert_eq!(snapshot.checksum(), CHECKSUM);
}

#[test]
fn generated_typed_xml_codecs_run_handwritten_hooks() {
    let run = support::run_consumer(
        "xml-hooks-consumer",
        BUNDLE,
        &[
            ("hooks.rs", include_str!("fixtures/xml_hooks/hooks.rs")),
            (
                "pilot_tests.rs",
                include_str!("fixtures/xml_hooks/consumer_tests.rs"),
            ),
        ],
        "pub mod hooks;\n#[cfg(test)]\nmod pilot_tests;\n",
        "\"xml\"",
        &Default::default(),
    );
    assert!(run.stdout.contains("3 passed"), "{}", run.stdout);
}
