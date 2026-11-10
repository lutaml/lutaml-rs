mod contracts {
    pub use lutaml_rs::contracts::*;
}
mod values {
    pub use lutaml_rs::values::*;
}
#[path = "../src/codecs/mod.rs"]
mod codecs;
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
    assert!(stdout.contains("14 passed"), "{stdout}");
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
        "Absent", "Null", "Member", "Literal", "Element", "Text", "Cdata", "Unique", "Visitor",
        "Ok", "Err", "Some", "None", "Self", "Case",
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
