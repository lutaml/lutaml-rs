mod contracts {
    pub use lutaml_rs::contracts::*;
}
mod values {
    pub use lutaml_rs::values::*;
}
#[path = "../src/codecs/mod.rs"]
mod codecs;
// This JSON harness imports private reserved-name metadata from the emitter;
// its XML entrypoint is exercised by tests/generator.rs instead.
#[expect(
    dead_code,
    reason = "the copied emitter is used only for JSON generation"
)]
#[path = "../src/generator/mod.rs"]
mod generator;
use codecs::RustJsonEmitter;
use contracts::*;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
#[allow(dead_code)]
#[path = "../src/lml/mod.rs"]
mod lml;
fn attribute(name: &str, t: BuiltinType) -> Attribute {
    Attribute {
        name: name.into(),
        value_type: TypeRef::Builtin { name: t },
        required: false,
        collection: Collection::Scalar(ScalarCollection),
        default: DefaultValue::Absent,
        initialize_empty: false,
        discriminator: None,
        reference: None,
        enumeration: None,
    }
}
fn model(name: &str, attributes: Vec<Attribute>, rules: Vec<KeyRule>) -> ModelDocument {
    ModelDocument {
        lml: 2,
        model: name.into(),
        register: "default".into(),
        attributes,
        mappings: Mappings {
            json: Some(KeyMapping {
                rules,
                root_mappings: None,
                child_mappings: None,
                ..Default::default()
            }),
            ..Default::default()
        },
        ordered_content: None,
        choices: None,
        custom_initialize: false,
    }
}
fn rule(
    name: &str,
    aliases: &[&str],
    nil: RenderAction,
    empty: RenderAction,
    default: bool,
) -> KeyRule {
    KeyRule {
        key: aliases.iter().map(|s| s.to_string()).collect(),
        to: name.into(),
        render: Some(RenderPolicy {
            nil,
            empty,
            default: default.into(),
        }),
        ..Default::default()
    }
}
fn keep(name: &str, aliases: &[&str]) -> KeyRule {
    rule(name, aliases, RenderAction::Nil, RenderAction::Blank, true)
}
fn reference(name: &str, id: &str) -> Attribute {
    let mut a = attribute(name, BuiltinType::String);
    a.value_type = TypeRef::Model { id: id.into() };
    a
}
fn fixture() -> Bundle {
    let item = model(
        "Item",
        vec![
            attribute("id", BuiltinType::String),
            attribute("name", BuiltinType::String),
            attribute("amount", BuiltinType::Decimal),
        ],
        vec![
            keep("id", &["wire_id"]),
            keep("name", &["wire_name", "old_name"]),
            keep("amount", &["price"]),
        ],
    );
    let mut enabled = attribute("enabled", BuiltinType::Boolean);
    enabled.default = DefaultValue::Present(NeutralValue::Boolean(false));
    let mut entries = attribute("entries", BuiltinType::String);
    entries.collection = Collection::Bounded {
        min: 0,
        max: Some(3),
        class_binding: None,
    };
    let mut selected = reference("selected", "Item@default");
    selected.discriminator = Some(Discriminator {
        attribute: "kind".into(),
        cases: BTreeMap::from([("item".into(), "Item@default".into())]),
    });
    let mut record = model(
        "Record",
        vec![
            attribute("title", BuiltinType::String),
            enabled,
            entries,
            attribute("count", BuiltinType::Integer),
            attribute("amount", BuiltinType::Decimal),
            reference("child", "Item@default"),
            selected,
            attribute("meta", BuiltinType::Hash),
        ],
        vec![
            keep("title", &["label", "old_label"]),
            rule(
                "enabled",
                &["enabled"],
                RenderAction::Omit,
                RenderAction::Blank,
                false,
            ),
            rule(
                "entries",
                &["items"],
                RenderAction::Nil,
                RenderAction::Omit,
                true,
            ),
            keep("count", &["count"]),
            keep("amount", &["amount"]),
            keep("child", &["child"]),
            keep("selected", &["selected"]),
            keep("meta", &["meta"]),
        ],
    );
    record.ordered_content = Some(OrderedContent {
        member: "__lml_content".into(),
        version: 1,
    });
    let mut collection = reference("records", "Item@default");
    collection.collection = Collection::Bounded {
        min: 0,
        max: None,
        class_binding: None,
    };
    let mut root = model(
        "Root",
        vec![collection.clone()],
        vec![keep("records", &["ignored"])],
    );
    root.mappings.json.as_mut().unwrap().root_mappings = Some(BTreeMap::from([(
        "id".into(),
        NeutralValue::String("key".into()),
    )]));
    let mut children = model(
        "Children",
        vec![collection],
        vec![keep("records", &["records", "old_records"])],
    );
    children.mappings.json.as_mut().unwrap().child_mappings = Some(BTreeMap::from([
        ("id".into(), NeutralValue::String("key".into())),
        (
            "name".into(),
            NeutralValue::Array(vec![
                NeutralValue::String("path".into()),
                NeutralValue::String("name".into()),
            ]),
        ),
        ("amount".into(), NeutralValue::String("price".into())),
    ]));
    let root_paths = root.mappings.json.as_mut().unwrap().root_mappings.take();
    root.mappings.json.as_mut().unwrap().rules[0].root_mappings = root_paths;
    let child_paths = children
        .mappings
        .json
        .as_mut()
        .unwrap()
        .child_mappings
        .take();
    children.mappings.json.as_mut().unwrap().rules[0].child_mappings = child_paths;
    Bundle {
        lml_bundle: 1,
        roots: vec!["Record@default".into()],
        models: BTreeMap::from([
            ("Item@default".into(), item),
            ("Record@default".into(), record),
            ("Root@default".into(), root),
            ("Children@default".into(), children),
        ]),
        value_types: BTreeMap::new(),
        capabilities: vec!["json".into()],
        provenance: None,
    }
}
#[test]
fn generation_is_deterministic_and_rejects_ambiguous_aliases() {
    let b = fixture();
    let output = RustJsonEmitter
        .generate_json_codecs(&b, &TypeBindings::new())
        .unwrap();
    assert_eq!(
        output,
        RustJsonEmitter
            .generate_json_codecs(&b, &TypeBindings::new())
            .unwrap()
    );
    assert_eq!(output.keys().collect::<Vec<_>>(), vec!["json_codecs.rs"]);
    let mut b = fixture();
    b.models
        .get_mut("Record@default")
        .unwrap()
        .mappings
        .json
        .as_mut()
        .unwrap()
        .rules[1]
        .key = vec!["label".into()];
    assert_eq!(
        RustJsonEmitter
            .generate_json_codecs(&b, &TypeBindings::new())
            .unwrap_err()
            .reason,
        ErrorReason::UnsupportedMapping
    );
}
#[test]
fn ambiguous_keyed_descriptors_reject_without_artifacts() {
    let mut b = fixture();
    let m = b.models.get_mut("Children@default").unwrap();
    let descriptor = m.mappings.json.as_mut().unwrap().rules[0]
        .child_mappings
        .take();
    m.mappings.json.as_mut().unwrap().child_mappings = descriptor;

    let mut a = m.attributes[0].clone();
    a.name = "other".into();
    m.attributes.push(a);
    m.mappings
        .json
        .as_mut()
        .unwrap()
        .rules
        .push(keep("other", &["other"]));
    assert_eq!(
        RustJsonEmitter
            .generate_json_codecs(&b, &TypeBindings::new())
            .unwrap_err()
            .reason,
        ErrorReason::UnsupportedMapping
    );
}
#[test]
fn overlapping_keyed_paths_and_custom_codec_signatures_reject() {
    let mut b = fixture();
    let paths = b
        .models
        .get_mut("Children@default")
        .unwrap()
        .mappings
        .json
        .as_mut()
        .unwrap()
        .rules[0]
        .child_mappings
        .as_mut()
        .unwrap();
    paths.insert("amount".into(), NeutralValue::String("path".into()));
    assert_eq!(
        RustJsonEmitter
            .generate_json_codecs(&b, &TypeBindings::new())
            .unwrap_err()
            .reason,
        ErrorReason::UnsupportedMapping
    );
    let binding = TypeBinding {
        rust_type: "String".into(),
        codec: Some("arbitrary_callable".into()),
    };
    assert_eq!(
        RustJsonEmitter
            .generate_json_codecs(&fixture(), &BTreeMap::from([("unused".into(), binding)]))
            .unwrap_err()
            .reason,
        ErrorReason::UnsupportedMapping
    );
}
const PORTABLE_PROFILE: &str = "portable-exact-json-v1";
#[derive(Debug, PartialEq, Eq)]
enum CorpusError {
    ChecksumMismatch,
    InvalidCorpus,
    MissingProfile,
    MissingCase,
    UnsupportedProfile,
}
fn select_portable(index: &serde_json::Value) -> Result<Vec<serde_json::Value>, CorpusError> {
    let profiles = index["profiles"]
        .as_object()
        .ok_or(CorpusError::MissingProfile)?;
    let targets = index["target_profiles"]["rust"]
        .as_array()
        .filter(|v| !v.is_empty())
        .ok_or(CorpusError::MissingProfile)?;
    if targets.iter().any(|p| p.as_str() != Some(PORTABLE_PROFILE)) {
        return Err(CorpusError::UnsupportedProfile);
    }
    if !profiles.contains_key(PORTABLE_PROFILE) {
        return Err(CorpusError::MissingProfile);
    }
    let cases = index["cases"]
        .as_array()
        .ok_or(CorpusError::InvalidCorpus)?;
    let mut by_id = BTreeMap::new();
    for case in cases {
        let id = case["id"].as_str().ok_or(CorpusError::InvalidCorpus)?;
        if by_id.insert(id, case).is_some() {
            return Err(CorpusError::InvalidCorpus);
        }
    }
    let mut covered = BTreeSet::new();
    for (profile, definition) in profiles {
        let required = definition["required_case_ids"]
            .as_array()
            .filter(|v| !v.is_empty())
            .ok_or(CorpusError::MissingCase)?;
        for id in required {
            let id = id.as_str().ok_or(CorpusError::InvalidCorpus)?;
            let case = by_id.get(id).ok_or(CorpusError::MissingCase)?;
            if case["wire_profile"].as_str() != Some(profile) || !covered.insert(id) {
                return Err(CorpusError::InvalidCorpus);
            }
        }
    }
    if covered.len() != cases.len() {
        return Err(CorpusError::InvalidCorpus);
    }
    let selected = profiles
        .get(PORTABLE_PROFILE)
        .ok_or(CorpusError::MissingProfile)?["required_case_ids"]
        .as_array()
        .ok_or(CorpusError::MissingCase)?;
    selected
        .iter()
        .map(|id| {
            let case = by_id
                .get(id.as_str().ok_or(CorpusError::InvalidCorpus)?)
                .ok_or(CorpusError::MissingCase)?;
            if case["format"] != "json" {
                return Err(CorpusError::UnsupportedProfile);
            }
            Ok((*case).clone())
        })
        .collect()
}
fn oracle_files() -> BTreeMap<&'static str, Vec<u8>> {
    BTreeMap::from([
        (
            "manifest.json",
            include_bytes!("../src/codecs/oracle/manifest.json").to_vec(),
        ),
        (
            "index.json",
            include_bytes!("../src/codecs/oracle/index.json").to_vec(),
        ),
        (
            "schema.json",
            include_bytes!("../src/codecs/oracle/schema.json").to_vec(),
        ),
        (
            "schema.yaml",
            include_bytes!("../src/codecs/oracle/schema.yaml").to_vec(),
        ),
    ])
}
fn verify_oracle(files: &BTreeMap<&str, Vec<u8>>) -> Result<serde_json::Value, CorpusError> {
    let source: serde_json::Value =
        serde_json::from_str(include_str!("../src/codecs/oracle/source.json"))
            .map_err(|_| CorpusError::InvalidCorpus)?;
    let manifest = files
        .get("manifest.json")
        .ok_or(CorpusError::InvalidCorpus)?;
    if format!("sha256:{:x}", Sha256::digest(manifest))
        != source["manifest_sha256"].as_str().unwrap()
    {
        return Err(CorpusError::ChecksumMismatch);
    }
    let manifest: serde_json::Value =
        serde_json::from_slice(manifest).map_err(|_| CorpusError::InvalidCorpus)?;
    for pin in manifest["files"]
        .as_array()
        .ok_or(CorpusError::InvalidCorpus)?
    {
        let name = pin["path"].as_str().ok_or(CorpusError::InvalidCorpus)?;
        let bytes = files.get(name).ok_or(CorpusError::InvalidCorpus)?;
        if format!("sha256:{:x}", Sha256::digest(bytes))
            != pin["sha256"].as_str().ok_or(CorpusError::InvalidCorpus)?
        {
            return Err(CorpusError::ChecksumMismatch);
        }
    }
    Ok(manifest)
}
fn pinned_oracle() -> (Bundle, Vec<serde_json::Value>) {
    let files = oracle_files();
    let manifest = verify_oracle(&files).unwrap();
    let snapshot = lml::load(&files["schema.json"], InputFormat::Json).unwrap();
    snapshot
        .verify_checksum(manifest["lml_checksum"].as_str().unwrap())
        .unwrap();
    let original = snapshot.canonical_json().to_owned();
    let checksum = snapshot.checksum().to_owned();
    let bundle = snapshot.bundle().unwrap();
    assert_eq!(snapshot.canonical_json(), original);
    assert_eq!(snapshot.checksum(), checksum);
    let index: serde_json::Value = serde_json::from_slice(&files["index.json"]).unwrap();
    let cases = select_portable(&index).unwrap();
    assert_eq!(
        cases
            .iter()
            .map(|c| c["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["core-json-portable-exact"]
    );
    assert_eq!(
        index["profiles"]["ruby-native-v1"]["required_case_ids"]
            .as_array()
            .unwrap()
            .len(),
        4
    );
    for case in &cases {
        assert_eq!(case["lml_checksum"], checksum);
    }
    (bundle, cases)
}
#[test]
fn portable_profile_has_explicit_required_ids_and_corruption_failures() {
    let files = oracle_files();
    verify_oracle(&files).unwrap();
    let mut corrupt = files.clone();
    corrupt.get_mut("index.json").unwrap().push(b' ');
    assert_eq!(verify_oracle(&corrupt), Err(CorpusError::ChecksumMismatch));
    let index: serde_json::Value = serde_json::from_slice(&files["index.json"]).unwrap();
    let mut missing = index.clone();
    missing["profiles"]
        .as_object_mut()
        .unwrap()
        .remove(PORTABLE_PROFILE);
    assert_eq!(select_portable(&missing), Err(CorpusError::MissingProfile));
    let mut missing = index.clone();
    missing["cases"]
        .as_array_mut()
        .unwrap()
        .retain(|c| c["id"] != "core-json-portable-exact");
    assert_eq!(select_portable(&missing), Err(CorpusError::MissingCase));
    let mut unsupported = index.clone();
    unsupported["target_profiles"]["rust"] = serde_json::json!(["unknown-profile"]);
    assert_eq!(
        select_portable(&unsupported),
        Err(CorpusError::UnsupportedProfile)
    );
    let (bundle, cases) = pinned_oracle();
    assert!(bundle.models.contains_key("LmlConformance::Record@default"));
    assert_eq!(cases.len(), 1);
}
fn consumer_bundle() -> (Bundle, Vec<serde_json::Value>) {
    let mut b = fixture();
    let (oracle_bundle, oracle_cases) = pinned_oracle();
    b.models.extend(oracle_bundle.models);
    let mut preservation = lml::load(
        include_bytes!("../src/lml/fixtures/02d-preservation.schema.json"),
        InputFormat::Json,
    )
    .unwrap()
    .bundle()
    .unwrap();
    b.models.insert(
        "LmlPreservation::TypedEnum@default".into(),
        preservation
            .models
            .remove("LmlPreservation::TypedEnum@default")
            .unwrap(),
    );
    let mut integer = attribute("number", BuiltinType::Integer);
    integer.default =
        DefaultValue::Present(NeutralValue::String("009007199254740993123456789".into()));
    integer.enumeration = Some(vec![
        NeutralValue::String("9007199254740993123456789".into()),
        NeutralValue::Signed(2),
    ]);
    b.models.insert(
        "BigEnum@default".into(),
        model("BigEnum", vec![integer], vec![keep("number", &["number"])]),
    );

    let mut count = attribute("count", BuiltinType::Integer);
    count.default = DefaultValue::Present(NeutralValue::Signed(5));
    b.models.insert(
        "IntDefault@default".into(),
        model(
            "IntDefault",
            vec![count],
            vec![rule(
                "count",
                &["count"],
                RenderAction::Nil,
                RenderAction::Blank,
                false,
            )],
        ),
    );

    let mut ratio = attribute("ratio", BuiltinType::Float);
    ratio.default = DefaultValue::Present(NeutralValue::String("0.71".into()));
    b.models.insert(
        "Measure@default".into(),
        model(
            "Measure",
            vec![attribute("value", BuiltinType::Float), ratio],
            vec![keep("value", &["value"]), keep("ratio", &["ratio"])],
        ),
    );

    let control_text = "\0\u{8}\u{c}\u{2028}";
    let mut enum_value = attribute("enum_value", BuiltinType::String);
    enum_value.enumeration = Some(vec![NeutralValue::String(control_text.into())]);
    let mut default_value = attribute("default_value", BuiltinType::String);
    default_value.default = DefaultValue::Present(NeutralValue::String(control_text.into()));
    b.models.insert(
        "ControlText@default".into(),
        model(
            "ControlText",
            vec![
                enum_value,
                default_value,
                attribute("aliased", BuiltinType::String),
            ],
            vec![
                keep("enum_value", &["enum_value"]),
                keep("default_value", &["default_value"]),
                keep("aliased", &[control_text]),
            ],
        ),
    );

    (b, oracle_cases)
}
#[test]
fn emitted_consumer_compiles_and_executes_json_behavior() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let dir = root.join("target/json-generated-consumer");
    std::fs::create_dir_all(dir.join("src")).unwrap();
    let (b, oracle_cases) = consumer_bundle();
    let models = generator::RustEmitter
        .generate_models(&b, &TypeBindings::new())
        .unwrap();
    let json = RustJsonEmitter
        .generate_json_codecs(&b, &TypeBindings::new())
        .unwrap();
    std::fs::write(dir.join("src/oracle-cases.json"),serde_json::to_vec(&serde_json::json!({"profile":PORTABLE_PROFILE,"expected_case_ids":["core-json-portable-exact"],"cases":oracle_cases})).unwrap()).unwrap();
    std::fs::write(
        dir.join("src/float_wire_vectors.json"),
        include_str!("fixtures/float_wire_vectors.json"),
    )
    .unwrap();
    std::fs::write(dir.join("src/models.rs"), &models["models.rs"]).unwrap();
    std::fs::write(dir.join("src/json_codecs.rs"), &json["json_codecs.rs"]).unwrap();
    std::fs::write(
        dir.join("src/values.rs"),
        include_str!("../src/values/mod.rs"),
    )
    .unwrap();
    std::fs::write(dir.join("src/lib.rs"),format!("pub mod contracts {{ pub use lutaml_rs::contracts::*; }}\npub mod values;\npub mod models;\npub mod json_codecs;\n{}",include_str!("../src/codecs/consumer_tests.rs.txt"))).unwrap();
    let manifest = format!(
        r#"[package]
name="generated-json-consumer"
version="0.0.0"
edition="2021"
[dependencies]
lutaml-rs={{path={}}}
serde={{version="=1.0.219",features=["derive"]}}
serde_json={{version="=1.0.140",features=["arbitrary_precision"]}}
num-bigint="=0.4.6"
num-rational="=0.4.2"
num-traits="=0.2.19"
"#,
        serde_json::to_string(root.to_str().unwrap()).unwrap()
    );
    std::fs::write(dir.join("Cargo.toml"), manifest).unwrap();
    // The outer targeted Cargo invocation holds the host memory lock. This child
    // inherits that lease and uses one build worker rather than reacquiring it.
    let result = std::process::Command::new(env!("CARGO"))
        .args(["test", "--offline", "--manifest-path"])
        .arg(dir.join("Cargo.toml"))
        .args(["--", "--nocapture"])
        .env("CARGO_BUILD_JOBS", "1")
        .env("CARGO_TARGET_DIR", root.join("target/json-consumer-build"))
        .output()
        .unwrap();
    std::fs::write(
        dir.join("test-output.log"),
        [result.stdout.as_slice(), result.stderr.as_slice()].concat(),
    )
    .unwrap();
    assert!(
        result.status.success(),
        "generated consumer failed:\n{}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    let stdout = String::from_utf8_lossy(&result.stdout);
    assert!(stdout.contains("18 passed"), "{stdout}");
}

#[test]
fn admitted_unknown_render_policies_are_named_codegen_refusals() {
    let mut b = fixture();
    b.models
        .get_mut("Item@default")
        .unwrap()
        .mappings
        .json
        .as_mut()
        .unwrap()
        .rules[0]
        .render
        .as_mut()
        .unwrap()
        .nil = RenderAction::Other("unknown-native-policy".into());
    assert_eq!(
        RustJsonEmitter
            .generate_json_codecs(&b, &TypeBindings::new())
            .unwrap_err()
            .reason,
        ErrorReason::UnsupportedMapping
    );
}

/// Write a generated-code consumer crate and run one Cargo command in it. The
/// outer targeted Cargo invocation holds the host memory lock; children use one
/// build worker and share one target directory so dependencies build once.
fn run_consumer(
    name: &str,
    bundle: &Bundle,
    lib_tail: &str,
    cargo: &[&str],
) -> std::process::Output {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let dir = root.join("target").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("src")).unwrap();
    let models = generator::RustEmitter
        .generate_models(bundle, &TypeBindings::new())
        .unwrap();
    let json = RustJsonEmitter
        .generate_json_codecs(bundle, &TypeBindings::new())
        .unwrap();
    std::fs::write(dir.join("src/models.rs"), &models["models.rs"]).unwrap();
    std::fs::write(dir.join("src/json_codecs.rs"), &json["json_codecs.rs"]).unwrap();
    std::fs::write(
        dir.join("src/values.rs"),
        include_str!("../src/values/mod.rs"),
    )
    .unwrap();
    std::fs::write(dir.join("src/lib.rs"),format!("pub mod contracts {{ pub use lutaml_rs::contracts::*; }}\npub mod values;\npub mod models;\npub mod json_codecs;\n{lib_tail}")).unwrap();
    std::fs::write(
        dir.join("Cargo.toml"),
        format!(
            "[package]\nname=\"{name}\"\nversion=\"0.0.0\"\nedition=\"2021\"\n[dependencies]\nlutaml-rs={{path={}}}\nserde={{version=\"=1.0.219\",features=[\"derive\"]}}\nserde_json={{version=\"=1.0.140\",features=[\"arbitrary_precision\"]}}\nnum-bigint=\"=0.4.6\"\nnum-rational=\"=0.4.2\"\nnum-traits=\"=0.2.19\"\n",
            serde_json::to_string(root.to_str().unwrap()).unwrap()
        ),
    )
    .unwrap();
    std::process::Command::new(env!("CARGO"))
        .args(cargo)
        .args(["--offline", "--manifest-path"])
        .arg(dir.join("Cargo.toml"))
        .env("CARGO_BUILD_JOBS", "1")
        .env("CARGO_TARGET_DIR", root.join("target/json-consumer-build"))
        .output()
        .unwrap()
}
fn model_named(name: &str) -> ModelDocument {
    model(
        name,
        vec![attribute("a", BuiltinType::String)],
        vec![keep("a", &["a"])],
    )
}
/// Names that are not reserved but resemble emitted items: enum variants and
/// function-local types in the runtime, prelude names never written bare,
/// module names, and names that used to alias emitted helper functions
/// (`decode_Root_data`, `decode_Root_keyed`).
const UNRESERVED_LOOKALIKES: &[&str] = &[
    "Text",
    "Element",
    "Cdata",
    "Member",
    "Literal",
    "Absent",
    "Null",
    "Unique",
    "Visitor",
    "Case0",
    "Case1",
    "A",
    "S",
    "T",
    "D",
    "E",
    "values",
    "models",
    "contracts",
    "json_codecs",
    "parg_bindings",
    "Ok",
    "Err",
    "Some",
    "None",
    "Iterator",
    "From",
    "Into",
    "AsRef",
    "Drop",
    "Fn",
    "Formatter",
    "Ordering",
    "Hash",
    "Error",
    "Item_data",
    "Item_keyed",
    "Root_data",
    "Root_keyed",
    "Children_data",
    "Children_keyed",
    "Record_data",
    "Record_json",
    "decode_Item",
    "read_Item_json",
];
#[test]
fn unreserved_lookalike_names_and_former_function_aliases_generate_a_crate_that_compiles() {
    let mut b = fixture();
    for name in UNRESERVED_LOOKALIKES {
        b.models
            .insert(format!("{name}@default"), model_named(name));
    }
    let out = run_consumer("json-lookalike-names", &b, "", &["check"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}
#[test]
fn every_capitalized_name_in_emitted_code_is_reserved_or_known_to_be_scoped() {
    let (b, _) = consumer_bundle();
    let names = generator::model_names(&b).unwrap();
    let models = generator::RustEmitter
        .generate_models(&b, &TypeBindings::new())
        .unwrap();
    let json = RustJsonEmitter
        .generate_json_codecs(&b, &TypeBindings::new())
        .unwrap();
    // Enum variants, fn-local types and a few prelude constructors that the
    // compile test above proves a model may share.
    let scoped = [
        "Absent",
        "Null",
        "Member",
        "Literal",
        "Element",
        "Text",
        "Cdata",
        "Unique",
        "Visitor",
        "Ok",
        "Err",
        "Some",
        "None",
        "Self",
        "Case",
        "Bool",
        "Number",
        "Array",
        "Object",
        "NEUTRAL_NUMBERS",
        // `impl Drop for ProvenanceWire`: a prelude trait, never declared.
        "Drop",
    ];
    let mut unknown = BTreeSet::new();
    for text in [&models["models.rs"], &json["json_codecs.rs"]] {
        let code = strip_literals(text);
        let bytes = code.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i].is_ascii_alphabetic() || bytes[i] == b'_' {
                let start = i;
                while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                    i += 1;
                }
                let token = &code[start..i];
                let qualified = start >= 2 && &code[start - 2..start] == "::";
                let capitalized = token.starts_with(|c: char| c.is_ascii_uppercase());
                let model_derived = names.values().any(|n| token.starts_with(&format!("{n}_")))
                    || names.values().any(|n| n == token);
                let numbered_case =
                    token.starts_with("Case") && token[4..].bytes().all(|c| c.is_ascii_digit());
                if capitalized
                    && token.len() > 1
                    && !qualified
                    && !model_derived
                    && !numbered_case
                    && !scoped.contains(&token)
                    && !generator::RESERVED_NAMES.contains(&token)
                {
                    unknown.insert(token.to_string());
                }
            } else {
                i += 1;
            }
        }
    }
    assert!(
        unknown.is_empty(),
        "emitted names neither reserved nor known to be scoped: {unknown:?}"
    );
}
fn strip_literals(text: &str) -> String {
    let mut out = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                while let Some(c) = chars.next() {
                    if c == '\\' {
                        chars.next();
                    } else if c == '"' {
                        break;
                    }
                }
                out.push(' ');
            }
            '/' if chars.peek() == Some(&'/') => {
                for c in chars.by_ref() {
                    if c == '\n' {
                        break;
                    }
                }
                out.push('\n');
            }
            _ => out.push(c),
        }
    }
    out
}
#[test]
fn emitted_function_names_cannot_alias_across_models() {
    // `Item` + `Item_data` used to emit `decode_Item_data` twice.
    let mut b = fixture();
    for name in ["Item_data", "Item_keyed", "Root_keyed", "Root_data"] {
        b.models
            .insert(format!("{name}@default"), model_named(name));
    }
    let json = RustJsonEmitter
        .generate_json_codecs(&b, &TypeBindings::new())
        .unwrap();
    let code = &json["json_codecs.rs"];
    let mut seen = BTreeSet::new();
    for line in code.lines() {
        if let Some(rest) = line
            .trim_start()
            .strip_prefix("fn ")
            .or_else(|| line.trim_start().strip_prefix("pub fn "))
        {
            if let Some(name) = rest.split(['(', '<']).next() {
                assert!(
                    seen.insert(name.to_string()) || name.is_empty(),
                    "duplicate fn {name}"
                );
            }
        }
    }
    assert!(
        seen.contains("decode_Item")
            && seen.contains("hydrate_Item")
            && seen.contains("hydrate_Item_data")
    );
}

const HOOKS_BUNDLE: &str = include_str!("fixtures/serialization_hooks_bundle.json");
const HOOKS_CONSUMER: &str = r##"
#[cfg(test)]
mod tests {
    use super::contracts::{ContractError, ContractResult, ErrorReason};
    use super::json_codecs::*;
    use super::models::{ExactInteger, Example_Measure, Presence};
    use serde_json::{Map, Value};
    #[derive(Default)]
    struct State { category: Option<String>, note: String }
    struct Real;
    impl Example_MeasureHooks<State> for Real {
        fn category_from_kv(&self, _m: &mut Example_Measure, value: &Value, s: &mut State) -> ContractResult<()> {
            s.category = value.as_str().map(str::to_owned);
            Ok(())
        }
        fn category_to_kv(&self, _m: &Example_Measure, d: &mut Map<String, Value>, s: &mut State) -> ContractResult<()> {
            if let Some(c) = &s.category { d.insert("category".into(), Value::String(c.clone())); }
            Ok(())
        }
        fn note_to_kv(&self, _m: &Example_Measure, d: &mut Map<String, Value>, s: &mut State) -> ContractResult<()> {
            d.insert("note".into(), Value::String(s.note.clone()));
            Ok(())
        }
    }
    /// Counts calls; implements only the WRITE hooks.
    struct WriteOnly(std::cell::RefCell<Vec<&'static str>>);
    impl Example_MeasureHooks<()> for WriteOnly {
        fn category_to_kv(&self, _m: &Example_Measure, _d: &mut Map<String, Value>, _s: &mut ()) -> ContractResult<()> {
            self.0.borrow_mut().push("category_to_kv"); Ok(())
        }
        fn note_to_kv(&self, _m: &Example_Measure, _d: &mut Map<String, Value>, _s: &mut ()) -> ContractResult<()> {
            self.0.borrow_mut().push("note_to_kv"); Ok(())
        }
    }
    /// Implements only the READ hook.
    struct ReadOnly(std::cell::RefCell<Vec<&'static str>>);
    impl Example_MeasureHooks<()> for ReadOnly {
        fn category_from_kv(&self, _m: &mut Example_Measure, _v: &Value, _s: &mut ()) -> ContractResult<()> {
            self.0.borrow_mut().push("category_from_kv"); Ok(())
        }
    }
    fn reason(e: &ContractError) -> (ErrorReason, String) { (e.reason.clone(), e.location.clone()) }

    #[test]
    fn declared_data_never_runs_a_hook_and_the_stub_fails_by_name() {
        let model = read_Example_Measure_json(r#"{"amount":"5"}"#).unwrap();
        assert_eq!(model.amount, Presence::Value(ExactInteger::parse("5").unwrap()));
        // serde reconstruction stays data-only.
        let data: Example_Measure = serde_json::from_str(r#"{"amount":"5"}"#).unwrap();
        assert_eq!(data, model);
        let read = read_Example_Measure_json(r#"{"amount":"5","category":"x"}"#).unwrap_err();
        assert_eq!(reason(&read), (ErrorReason::CustomSerialization, "/category".into()));
        let write = write_Example_Measure_json(&model).unwrap_err();
        assert_eq!(reason(&write), (ErrorReason::CustomSerialization, "/category".into()));
    }
    #[test]
    fn only_the_attempted_direction_runs_and_fails() {
        let model = read_Example_Measure_json(r#"{"amount":"5"}"#).unwrap();
        let writes = WriteOnly(Default::default());
        let out = write_Example_Measure_json_with(&model, &writes, &mut ()).unwrap();
        assert_eq!(out, r#"{"amount":"5"}"#);
        assert_eq!(*writes.0.borrow(), ["category_to_kv", "note_to_kv"]);
        // reading with a write-only implementation never invokes a write hook
        writes.0.borrow_mut().clear();
        read_Example_Measure_json_with(r#"{"amount":"5"}"#, &writes, &mut ()).unwrap();
        assert!(writes.0.borrow().is_empty());
        // a read-only implementation still fails a write, without running its read hook
        let reads = ReadOnly(Default::default());
        let err = write_Example_Measure_json_with(&model, &reads, &mut ()).unwrap_err();
        assert_eq!(reason(&err), (ErrorReason::CustomSerialization, "/category".into()));
        assert!(reads.0.borrow().is_empty());
        // a hook-only key with no declared read direction fails on read even with every hook installed
        let err = read_Example_Measure_json_with(r#"{"note":"n"}"#, &writes, &mut ()).unwrap_err();
        assert_eq!(reason(&err), (ErrorReason::CustomSerialization, "/note".into()));
    }
    #[test]
    fn explicit_handwritten_hooks_carry_real_values_through_read_and_write() {
        let mut state = State { category: None, note: "kept".into() };
        let model = read_Example_Measure_json_with(r#"{"amount":"7","category":"metric"}"#, &Real, &mut state).unwrap();
        assert_eq!(state.category.as_deref(), Some("metric"));
        let out = write_Example_Measure_json_with(&model, &Real, &mut state).unwrap();
        let value: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(value, serde_json::json!({"amount":"7","category":"metric","note":"kept"}));
    }
}
"##;
#[test]
fn hooked_consumer_compiles_and_executes_hook_behavior() {
    let b: Bundle = serde_json::from_str(HOOKS_BUNDLE).unwrap();
    let out = run_consumer("json-hooks-consumer", &b, HOOKS_CONSUMER, &["test"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{stdout}\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(stdout.contains("3 passed"), "{stdout}");
}
#[test]
fn unhooked_bundles_emit_no_hook_code_and_nested_hooked_models_are_refused() {
    let b: Bundle = serde_json::from_str(HOOKS_BUNDLE).unwrap();
    let hooked = RustJsonEmitter
        .generate_json_codecs(&b, &TypeBindings::new())
        .unwrap();
    assert!(hooked["json_codecs.rs"].contains("pub trait Example_MeasureHooks<S>"));
    let mut plain = b.clone();
    let rules = &mut plain
        .models
        .get_mut("Example::Measure@default")
        .unwrap()
        .mappings
        .json
        .as_mut()
        .unwrap()
        .rules;
    rules.retain(|r| r.custom.is_none());
    let text = RustJsonEmitter
        .generate_json_codecs(&plain, &TypeBindings::new())
        .unwrap()["json_codecs.rs"]
        .clone();
    assert!(!text.contains("Hooks") && !text.contains("custom_serialization"));
    // a model that holds the hooked model as a member cannot carry the hooks
    let mut nested = b.clone();
    let mut holder = nested.models["Example::Measure@default"].clone();
    holder.model = "Holder".into();
    let mut member = attribute("m", BuiltinType::String);
    member.value_type = TypeRef::Model {
        id: "Example::Measure@default".into(),
    };
    holder.attributes = vec![member];
    holder.mappings = Mappings::default();
    nested.models.insert("Holder@default".into(), holder);
    let error = RustJsonEmitter
        .generate_json_codecs(&nested, &TypeBindings::new())
        .unwrap_err();
    assert_eq!(error.reason, ErrorReason::UnsupportedMapping);
}

/// Default provenance matrix. Ruby (probe /private/tmp/lml-1033-default-policy-probe.rb, ordinary Serializable
/// classes, auto and explicit mapping): `new` -> default applied -> JSON `{}`; `new(flag: false)` -> explicit
/// -> `{"flag":false}`; `new(flag: true)` -> `{"flag":true}`. Independent of any custom constructor.
fn provenance_bundle() -> Bundle {
    let model = |name: &str, marked: bool, render_default: Option<bool>| {
        let mut flag = attribute("flag", BuiltinType::Boolean);
        flag.default = DefaultValue::Present(NeutralValue::Boolean(false));
        let mut count = attribute("count", BuiltinType::Integer);
        count.default = DefaultValue::Present(NeutralValue::Unsigned(0));
        let mut m = match render_default {
            Some(default) => {
                let policy = |member: &str| {
                    rule(
                        member,
                        &[member],
                        RenderAction::Omit,
                        RenderAction::Omit,
                        default,
                    )
                };
                model(
                    name,
                    vec![flag, count],
                    vec![policy("flag"), policy("count")],
                )
            }
            // No explicit rules: the implicit (auto) mapping.
            None => ModelDocument {
                mappings: Mappings::default(),
                ..model(name, vec![flag, count], vec![])
            },
        };
        m.custom_initialize = marked;
        (format!("{name}@default"), m)
    };
    Bundle {
        lml_bundle: 1,
        roots: vec!["Quiet@default".into()],
        models: BTreeMap::from([
            model("Quiet", false, Some(false)),
            model("QuietMarked", true, Some(false)),
            model("Loud", false, Some(true)),
            model("LoudMarked", true, Some(true)),
            model("Auto", false, None),
            model("AutoMarked", true, None),
        ]),
        value_types: BTreeMap::new(),
        capabilities: vec![],
        provenance: None,
    }
}
const PROVENANCE_CONSUMER: &str = r##"
#[cfg(test)]
mod tests {
    use super::json_codecs::*;
    use super::models::*;
    use serde_json::{json, Value};

    macro_rules! matrix {
        ($name:ident, $read:ident, $write:ident, $ty:ident, $render_default:expr) => {
            #[test]
            fn $name() {
                let render_default: bool = $render_default;
                let wire = |m: &$ty| serde_json::from_str::<Value>(&$write(m).unwrap()).unwrap();
                // Provided on decode: explicit false / true / 0 stay rendered, with or without apply_defaults.
                for input in [r#"{"flag":false}"#, r#"{"flag":true}"#, r#"{"count":"0"}"#, r#"{"flag":false,"count":"0"}"#] {
                    let mut m = $read(input).unwrap();
                    assert_eq!(wire(&m), serde_json::from_str::<Value>(input).unwrap(), "decode {input}");
                    m.apply_defaults().unwrap();
                    let applied = wire(&m);
                    let given: Value = serde_json::from_str(input).unwrap();
                    for (key, value) in given.as_object().unwrap() {
                        assert_eq!(applied[key], *value, "explicit {key} survives apply_defaults in {input}");
                    }
                }
                // Applied when omitted: suppressed (render default false) or rendered (render default true).
                let mut m = $read("{}").unwrap();
                m.apply_defaults().unwrap();
                assert_eq!(m.flag, Presence::Value(false));
                if render_default {
                    assert_eq!(wire(&m), json!({"flag": false, "count": "0"}));
                } else {
                    assert_eq!(wire(&m), json!({}));
                }
                // apply_defaults is idempotent and keeps provenance.
                let before = wire(&m);
                m.apply_defaults().unwrap();
                assert_eq!(wire(&m), before);
                // Not applied: absent stays absent.
                assert_eq!(wire(&$read("{}").unwrap()), json!({}));
                // Constructed: an assigned default-equal value is explicit; Default + apply is applied.
                let mut built = $ty::default();
                built.flag = Presence::Value(false);
                built.apply_defaults().unwrap();
                assert_eq!(wire(&built)["flag"], json!(false));
                // A changed value is always rendered; null keeps its policy (omit).
                let mut changed = $read("{}").unwrap();
                changed.apply_defaults().unwrap();
                changed.flag = Presence::Value(true);
                assert_eq!(wire(&changed)["flag"], json!(true));
                let nulled = $read(r#"{"flag":null}"#).unwrap();
                assert_eq!(nulled.flag, Presence::Null);
                assert!(wire(&nulled).get("flag").is_none());
                // Provenance is not part of plain serde output or of equality.
                assert!(serde_json::to_value(&m).unwrap().get("__lml_applied").is_none());
                let mut twin = $read("{}").unwrap();
                twin.flag = Presence::Value(false);
                twin.count = Presence::Value(serde_json::from_str("\"0\"").unwrap());
                assert_eq!(m, twin);
            }
        };
    }
    matrix!(quiet, read_Quiet_json, write_Quiet_json, Quiet, false);
    matrix!(quiet_marked, read_QuietMarked_json, write_QuietMarked_json, QuietMarked, false);
    matrix!(loud, read_Loud_json, write_Loud_json, Loud, true);
    matrix!(loud_marked, read_LoudMarked_json, write_LoudMarked_json, LoudMarked, true);
    // Implicit mapping behaves like the explicit render-default-false rules.
    matrix!(auto, read_Auto_json, write_Auto_json, Auto, false);
    matrix!(auto_marked, read_AutoMarked_json, write_AutoMarked_json, AutoMarked, false);
}
"##;
#[test]
fn default_provenance_matrix_for_marked_and_unmarked_models() {
    let out = run_consumer(
        "json-provenance-consumer",
        &provenance_bundle(),
        PROVENANCE_CONSUMER,
        &["test"],
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{stdout}\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(stdout.contains("6 passed"), "{stdout}");
}

/// A declared REQUIRED member supplied by a `custom.from` hook. The hook runs after the declared data is
/// built and before validation, with mutable access, so the member is filled (not rejected, not forced into
/// opaque state); validation sees the handwritten result.
#[test]
fn a_required_declared_member_supplied_by_a_read_hook_is_filled_before_validation() {
    let mut title = attribute("title", BuiltinType::String);
    title.required = true;
    let mut rule = rule(
        "title",
        &["t"],
        RenderAction::Omit,
        RenderAction::Omit,
        false,
    );
    rule.custom = Some(CustomSerialization {
        to: None,
        from: Some("title_from_kv".into()),
    });
    let bundle = Bundle {
        lml_bundle: 1,
        roots: vec!["Req@default".into()],
        models: BTreeMap::from([("Req@default".into(), model("Req", vec![title], vec![rule]))]),
        value_types: BTreeMap::new(),
        capabilities: vec![],
        provenance: None,
    };
    let consumer = r##"
#[cfg(test)]
mod tests {
    use super::contracts::{ContractResult, ErrorReason};
    use super::json_codecs::*;
    use super::models::*;
    use serde_json::Value;
    struct Fill;
    impl ReqHooks<()> for Fill {
        fn title_from_kv(&self, model: &mut Req, value: &Value, _s: &mut ()) -> ContractResult<()> {
            model.title = Presence::Value(value.as_str().unwrap().to_uppercase());
            Ok(())
        }
    }
    struct Blank;
    impl ReqHooks<()> for Blank {
        fn title_from_kv(&self, _m: &mut Req, _v: &Value, _s: &mut ()) -> ContractResult<()> { Ok(()) }
    }
    #[test]
    fn filled_before_validation() {
        let model = read_Req_json_with(r#"{"t":"abc"}"#, &Fill, &mut ()).unwrap();
        assert_eq!(model.title, Presence::Value("ABC".to_string()));
        assert_eq!(write_Req_json(&model).unwrap(), r#"{"t":"ABC"}"#);
    }
    #[test]
    fn validation_still_runs_on_the_handwritten_result() {
        let error = read_Req_json_with(r#"{"t":"abc"}"#, &Blank, &mut ()).unwrap_err();
        assert_eq!(error.reason, ErrorReason::InvalidDocument);
    }
    #[test]
    fn a_missing_handler_stays_custom_serialization() {
        let error = read_Req_json(r#"{"t":"abc"}"#).unwrap_err();
        assert_eq!(error.reason, ErrorReason::CustomSerialization);
    }
}
"##;
    let out = run_consumer("json-required-hook", &bundle, consumer, &["test"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{stdout}\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(stdout.contains("3 passed"), "{stdout}");
}

/// A parent with NO defaults whose child has defaults: `apply_defaults` recurses (single, collection, union
/// cases), the root write enables provenance although only a descendant tracks defaults, and the bookkeeping
/// field can neither be supplied through serde/JSON input nor leak into ordinary output.
fn nested_provenance_bundle() -> Bundle {
    let inner = |name: &str, render_default: bool| {
        let mut flag = attribute("flag", BuiltinType::Boolean);
        flag.default = DefaultValue::Present(NeutralValue::Boolean(false));
        let policy = rule(
            "flag",
            &["flag"],
            RenderAction::Omit,
            RenderAction::Omit,
            render_default,
        );
        (
            format!("{name}@default"),
            model(name, vec![flag], vec![policy]),
        )
    };
    let parent = |name: &str, inner: &str| {
        let mut one = attribute("inner", BuiltinType::String);
        one.value_type = TypeRef::Model {
            id: format!("{inner}@default"),
        };
        let mut many = attribute("items", BuiltinType::String);
        many.value_type = TypeRef::Model {
            id: format!("{inner}@default"),
        };
        many.collection = Collection::Bounded {
            min: 0,
            max: None,
            class_binding: None,
        };
        let mut absent = attribute("absent_child", BuiltinType::String);
        absent.value_type = TypeRef::Model {
            id: format!("{inner}@default"),
        };
        let rules = ["inner", "items", "absent_child"]
            .map(|m| rule(m, &[m], RenderAction::Omit, RenderAction::Omit, false))
            .to_vec();
        (
            format!("{name}@default"),
            model(name, vec![one, many, absent], rules),
        )
    };
    Bundle {
        lml_bundle: 1,
        roots: vec!["Parent@default".into()],
        models: BTreeMap::from([
            inner("Inner", false),
            inner("InnerLoud", true),
            parent("Parent", "Inner"),
            parent("ParentLoud", "InnerLoud"),
        ]),
        value_types: BTreeMap::new(),
        capabilities: vec![],
        provenance: None,
    }
}
const NESTED_PROVENANCE: &str = r##"
#[cfg(test)]
mod tests {
    use super::json_codecs::*;
    use super::models::*;
    use serde_json::{json, Value};
    macro_rules! nested {
        ($name:ident, $read:ident, $write:ident, $ty:ident, $render_default:expr) => {
            #[test]
            fn $name() {
                let rd: bool = $render_default;
                let wire = |m: &$ty| serde_json::from_str::<Value>(&$write(m).unwrap()).unwrap();
                let mut m = $read(r#"{"inner":{},"items":[{},{"flag":false}]}"#).unwrap();
                m.apply_defaults().unwrap();
                let Presence::Value(inner) = &m.inner else { panic!() };
                assert!(inner.__lml_applied.contains("flag"));
                let Presence::Value(items) = &m.items else { panic!() };
                assert!(items[0].__lml_applied.contains("flag"));
                assert!(!items[1].__lml_applied.contains("flag"), "explicit equal value stays explicit");
                assert!(m.absent_child.is_absent(), "absent children are never created");
                let expected = if rd {
                    json!({"inner": {"flag": false}, "items": [{"flag": false}, {"flag": false}]})
                } else {
                    json!({"inner": {}, "items": [{}, {"flag": false}]})
                };
                assert_eq!(wire(&m), expected);
                let before = $write(&m).unwrap();
                m.apply_defaults().unwrap();
                assert_eq!(before, $write(&m).unwrap(), "idempotent");
                // null stays null and is untouched by defaults
                let mut nulled = $read(r#"{"inner":null}"#).unwrap();
                nulled.apply_defaults().unwrap();
                assert_eq!(nulled.inner, Presence::Null);
                // private metadata: not output by serde or the codecs, not accepted from input
                assert!(!serde_json::to_string(&m).unwrap().contains("__lml_applied"));
                assert!(!$write(&m).unwrap().contains("__lml_applied"));
                assert!($read(r#"{"__lml_applied":["flag"]}"#).is_err());
                assert!($read(r#"{"inner":{"__lml_applied":["flag"]}}"#).is_err());
                assert!(serde_json::from_str::<$ty>(r#"{"__lml_applied":["flag"]}"#).is_err());
                assert!(serde_json::from_str::<Inner>(r#"{"flag":false,"__lml_applied":["flag"]}"#).is_err());
            }
        };
    }
    nested!(quiet, read_Parent_json, write_Parent_json, Parent, false);
    nested!(loud, read_ParentLoud_json, write_ParentLoud_json, ParentLoud, true);
}
"##;
#[test]
fn defaults_recurse_and_provenance_is_private_in_key_value_codecs() {
    let out = run_consumer(
        "json-nested-provenance",
        &nested_provenance_bundle(),
        NESTED_PROVENANCE,
        &["test"],
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{stdout}\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(stdout.contains("2 passed"), "{stdout}");
}
/// `applied` is declared only when a generated line reads it; otherwise an unused variable fails
/// `clippy -D warnings` in the generated crate. The provenance key is consumed in every shape.
#[test]
fn applied_defaults_are_declared_only_when_read() {
    let encode = |default_rendered: bool| {
        let mut flag = attribute("flag", BuiltinType::Boolean);
        flag.default = DefaultValue::Present(NeutralValue::Boolean(false));
        let r = rule(
            "flag",
            &["flag"],
            RenderAction::Omit,
            RenderAction::Blank,
            default_rendered,
        );
        let m = model("Solo", vec![flag], vec![r]);
        let b = Bundle {
            roots: vec!["Solo@default".into()],
            models: BTreeMap::from([("Solo@default".into(), m)]),
            ..fixture()
        };
        RustJsonEmitter
            .generate_json_codecs(&b, &TypeBindings::new())
            .map(|o| o["json_codecs.rs"].clone())
    };
    let read = encode(false).unwrap();
    assert!(read.contains("let applied:Vec<String>"), "{read}");
    let rendered = encode(true).unwrap();
    assert!(!rendered.contains("let applied"), "{rendered}");
    assert!(
        rendered.contains("data.remove(\"__lml_applied\")"),
        "{rendered}"
    );
}
