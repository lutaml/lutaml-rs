mod values {
    pub use lutaml_rs::values::*;
}
#[path = "../src/lml/mod.rs"]
mod lml;
use lml::*;
use lutaml_rs::contracts;
use lutaml_rs::*;
use serde_json::{json, Value};
use std::collections::BTreeMap;

fn model(name: &str, register: &str) -> Value {
    json!({"lml":2,"model":name,"register":register,"attributes":[{"name":"amount","type":{"kind":"builtin","name":"decimal"},"required":false,"collection":false,"default":{"present":true,"value":"9007199254740993.000"},"initialize_empty":false}],"mappings":{"json":{"rules":[{"key":["amount","amt"],"to":"amount"}]}}})
}
fn bundle() -> Value {
    json!({"lml_bundle":1,"roots":["Example::Node@custom"],"models":{"Example::Node@custom":model("Example::Node","custom")},"value_types":{},"capabilities":["json"],"provenance":{"source_revision":"one"}})
}
fn bytes(v: &Value) -> Vec<u8> {
    serde_json::to_vec(v).unwrap()
}
fn parsed(v: &Value) -> Snapshot {
    load(&bytes(v), InputFormat::Json).unwrap()
}
fn refused(v: &Value, reason: ErrorReason, location: &str) -> Box<ContractError> {
    let error = load(&bytes(v), InputFormat::Json).unwrap_err();
    assert_eq!(error.reason, reason, "{error:?}");
    assert_eq!(error.location, location, "{error:?}");
    error
}

#[test]
fn json_yaml_share_exact_values_canonical_bytes_and_semantic_identity() {
    let v = bundle();
    let json = parsed(&v);
    let yaml = load(
        br#"lml_bundle: 1
roots: ['Example::Node@custom']
models:
  Example::Node@custom:
    lml: 2
    model: Example::Node
    register: custom
    attributes:
      - name: amount
        type: {kind: builtin, name: decimal}
        required: false
        collection: false
        default: {present: true, value: '9007199254740993.000'}
        initialize_empty: false
    mappings:
      json:
        rules: [{key: [amount, amt], to: amount}]
value_types: {}
capabilities: [json]
provenance: {source_revision: one}
"#,
        InputFormat::Yaml,
    )
    .unwrap();
    assert_eq!(json.canonical_json(), yaml.canonical_json());
    assert_eq!(json.checksum(), yaml.checksum());
    assert_eq!(
        json.bundle().unwrap().models["Example::Node@custom"].attributes[0].default,
        DefaultValue::Present(NeutralValue::String("9007199254740993.000".into()))
    );
    let mut changed = v.clone();
    changed["provenance"]["source_revision"] = json!("two");
    assert_eq!(json.checksum(), parsed(&changed).checksum());
    assert_ne!(json.canonical_json(), parsed(&changed).canonical_json());
    changed["models"]["Example::Node@custom"]["attributes"][0]["default"]["value"] =
        json!("9007199254740993.00");
    assert_ne!(json.checksum(), parsed(&changed).checksum());
    let mut copy = json.bundle().unwrap();
    copy.roots.clear();
    assert_eq!(json.bundle().unwrap().roots, vec!["Example::Node@custom"]);
    assert_eq!(
        Loader.load_bundle(&bytes(&v), InputFormat::Json).unwrap(),
        load_bundle(&bytes(&v), InputFormat::Json).unwrap()
    );
    assert!(json.tree().get("models").is_some());
}
#[test]
fn v1_identity_preserves_actual_legacy_insertion_order() {
    let original = r#"{"lml":1,"model":"LmlSpecFixtures::Unit","attributes":[{"name":"id","type":"string"},{"name":"symbol","type":"string","default":"u"}],"mappings":{"json":{"rules":[{"key":["id"],"to":"id"}]}}}"#;
    let loaded = load(original.as_bytes(), InputFormat::Json).unwrap();
    assert_eq!(loaded.canonical_json(), original);
    assert_eq!(
        loaded.checksum(),
        "sha256:04b66d5ff963c422758bce40377498c7f466645c215f19bccc5936d78468911b"
    );
    let reordered = original.replace(
        r#""name":"id","type":"string""#,
        r#""type":"string","name":"id""#,
    );
    assert_ne!(
        load(reordered.as_bytes(), InputFormat::Json)
            .unwrap()
            .checksum(),
        loaded.checksum()
    );
    let legacy = load(
        br#"{"extra":true,"model":"Legacy","lml":1,"register":null}"#,
        InputFormat::Json,
    )
    .unwrap();
    assert_eq!(
        legacy.canonical_json(),
        r#"{"lml":1,"model":"Legacy","attributes":[],"mappings":{}}"#
    );
    assert!(legacy.bundle().is_err());
}
#[test]
fn canonical_golden_keeps_unicode_slashes_and_array_order() {
    let node = Node::Object(vec![
        ("z".into(), Node::String("é/\n".into())),
        (
            "a".into(),
            Node::Array(vec![Node::String("b".into()), Node::String("a".into())]),
        ),
    ]);
    assert_eq!(
        canonical_json(&node).unwrap(),
        "{\"a\":[\"b\",\"a\"],\"z\":\"é/\\n\"}"
    );
    assert_eq!(
        checksum(&Node::Object(vec![])).unwrap(),
        "sha256:44136fa355b3678a1146ad16f7e8649e94fb4fc21fe77e8310c060f61caaff8a"
    );
}
#[test]
fn refuses_bad_versions_and_duplicate_json_or_yaml_keys() {
    let mut v = bundle();
    v["lml_bundle"] = json!(2);
    refused(&v, ErrorReason::UnsupportedVersion, "/lml_bundle");
    v = bundle();
    v["models"]["Example::Node@custom"]["lml"] = json!(1);
    refused(
        &v,
        ErrorReason::UnsupportedVersion,
        "/models/Example::Node@custom/lml",
    );
    refused(
        &json!({"lml":999,"model":"X"}),
        ErrorReason::UnsupportedVersion,
        "/lml",
    );
    let duplicate = load(
        br#"{"lml_bundle":1,"models":{"X@default":{},"X@default":{}}}"#,
        InputFormat::Json,
    )
    .unwrap_err();
    assert_eq!(duplicate.reason, ErrorReason::InvalidDocument);
    assert_eq!(duplicate.location, "/models/X@default");
    for wire in [
        "lml: 1\nmodel: A\nmodel: B\n",
        "lml: 1\nmodel: A\n---\nlml: 1\nmodel: B\n",
    ] {
        assert!(load(wire.as_bytes(), InputFormat::Yaml).is_err());
    }
    for wire in [
        br#"{"lml":1,"lml":2}"#.as_slice(),
        b"{} trailing",
        b"\xef\xbb\xbf{}",
        br#"{"model":"\ud800"}"#,
    ] {
        assert!(load(wire, InputFormat::Json).is_err());
    }
}
#[test]
fn yaml_rejects_aliases_anchors_tags_and_executable_classes_but_preserves_literal_text() {
    for wire in [
        "lml: 1\nmodel: &name A\n",
        "lml: 1\nmodel: *name\n",
        "lml: 1\nmodel: !!str A\n",
        "lml: 1\nmodel: !ruby/object:A {}\n",
        "lml: 1\nmodel: [ &name A ]\n",
    ] {
        assert!(load(wire.as_bytes(), InputFormat::Yaml).is_err(), "{wire}");
    }
    for wire in [
        "lml: 1\nmodel: 'A &safe *safe !safe'\n",
        "lml: 1\nmodel: |\n  &safe *safe !safe\n",
    ] {
        let snapshot = load(wire.as_bytes(), InputFormat::Yaml).unwrap();
        assert!(snapshot.canonical_json().contains("&safe *safe !safe"));
    }
}
#[test]
fn validates_exact_default_presence_and_structural_bounds_without_float_fallback() {
    let mut v = bundle();
    let at = "/models/Example::Node@custom/attributes/0";
    v["models"]["Example::Node@custom"]["attributes"][0]["default"] =
        json!({"present":true,"value":9007199254740993u64});
    refused(
        &v,
        ErrorReason::InvalidDocument,
        &format!("{at}/default/value"),
    );
    // decimal is shape-checked only (a JSON string), as in Ruby and TS: the
    // exporter passes a user's string through verbatim.
    for value in [json!("1/3"), json!("NaN"), json!("abc")] {
        let mut v = bundle();
        v["models"]["Example::Node@custom"]["attributes"][0]["default"]["value"] = value;
        parsed(&v);
    }
    for value in [json!(1.25), json!(true)] {
        let mut v = bundle();
        v["models"]["Example::Node@custom"]["attributes"][0]["default"]["value"] = value;
        let error = load(&bytes(&v), InputFormat::Json).unwrap_err();
        assert!(matches!(
            error.reason,
            ErrorReason::InvalidDocument | ErrorReason::NonSerializableDefault
        ));
    }
    for default in [
        json!({"present":false,"value":null}),
        json!({"present":true}),
    ] {
        let mut v = bundle();
        v["models"]["Example::Node@custom"]["attributes"][0]["default"] = default;
        refused(
            &v,
            ErrorReason::NonSerializableDefault,
            &format!("{at}/default"),
        );
    }
    for value in [json!(null), json!("0.000")] {
        let mut v = bundle();
        v["models"]["Example::Node@custom"]["attributes"][0]["default"]["value"] = value;
        parsed(&v);
    }
    let mut v = bundle();
    v["models"]["Example::Node@custom"]["attributes"][0]["collection"] = json!({"min":2,"max":1});
    refused(
        &v,
        ErrorReason::InvalidDocument,
        &format!("{at}/collection/max"),
    );
}
#[test]
fn graph_identity_register_context_and_discriminator_targets_are_checked() {
    let mut v = bundle();
    v["models"]["Example::Node@custom"]["register"] = json!("default");
    refused(&v, ErrorReason::DuplicateId, "/models/Example::Node@custom");
    v = bundle();
    v["models"]["Example::Node@custom"]["attributes"][0]["type"] =
        json!({"kind":"model","id":"Example::Node@default"});
    v["models"]["Example::Node@custom"]["attributes"][0]["default"] = json!({"present":false});
    let error = refused(
        &v,
        ErrorReason::DanglingReference,
        "/models/Example::Node@custom/attributes/0/type/id",
    );
    assert_eq!(error.model_id.as_deref(), Some("Example::Node@custom"));
    assert_eq!(error.attribute.as_deref(), Some("amount"));
    v["models"]["Example::Node@custom"]["attributes"][0]["type"]["id"] =
        json!("Example::Node@custom");
    parsed(&v);
    v["models"]["Example::Node@custom"]["attributes"][0]["discriminator"] =
        json!({"attribute":"missing","cases":{"node":"Example::Node@custom"}});
    refused(
        &v,
        ErrorReason::UnknownDiscriminator,
        "/models/Example::Node@custom/attributes/0/discriminator",
    );
}
#[test]
fn rejects_unknown_features_and_preserves_declared_enum_alias_order() {
    let mut v = bundle();
    v["models"]["Example::Node@custom"]["attributes"][0]["type"]["name"] = json!("double");
    refused(
        &v,
        ErrorReason::UnsupportedType,
        "/models/Example::Node@custom/attributes/0/type/name",
    );
    v = bundle();
    v["models"]["Example::Node@custom"]["attributes"][0]["type"]["name"] = json!("float");
    for good in ["0.71", "1.0e-5", "-0.0", "1.0e20", "9.999999999999998e15"] {
        v["models"]["Example::Node@custom"]["attributes"][0]["default"]["value"] = json!(good);
        parsed(&v);
    }
    for bad in [
        json!("1.0e-05"),
        json!("+0.71"),
        json!(".71"),
        json!("1e-5"),
        json!("NaN"),
        json!("Infinity"),
        json!(1),
    ] {
        v["models"]["Example::Node@custom"]["attributes"][0]["default"]["value"] = bad;
        refused(
            &v,
            ErrorReason::NonSerializableDefault,
            "/models/Example::Node@custom/attributes/0/default/value",
        );
    }
    // JSON-number defaults are refused before type checking.
    v["models"]["Example::Node@custom"]["attributes"][0]["default"]["value"] = json!(0.71);
    refused(
        &v,
        ErrorReason::InvalidDocument,
        "/models/Example::Node@custom/attributes/0/default/value",
    );
    v = bundle();
    v["capabilities"] = json!([]);
    refused(
        &v,
        ErrorReason::CapabilityMissing,
        "/models/Example::Node@custom/mappings/json",
    );
    v = bundle();
    v["models"]["Example::Node@custom"]["custom_initialize"] = json!(true);
    refused(
        &v,
        ErrorReason::CustomInitialize,
        "/models/Example::Node@custom/custom_initialize",
    );
    v = bundle();
    v["models"]["Example::Node@custom"]["attributes"][0]["values"] =
        json!(["2.0", "9007199254740993.000"]);
    let graph = parsed(&v).bundle().unwrap();
    assert_eq!(
        graph.models["Example::Node@custom"].attributes[0]
            .enumeration
            .as_ref()
            .unwrap()[0],
        NeutralValue::String("2.0".into())
    );
    assert_eq!(
        graph.models["Example::Node@custom"]
            .mappings
            .json
            .as_ref()
            .unwrap()
            .rules[0]
            .key,
        vec!["amount", "amt"]
    );
}
#[test]
fn value_types_binding_requirements_and_parg_metadata_are_external_and_checked() {
    let mut v = bundle();
    v["value_types"] = json!({"Money":{"base":"decimal","facets":{"fraction_digits":3}}});
    v["models"]["Example::Node@custom"]["attributes"][0]["type"] =
        json!({"kind":"value","id":"Money"});
    let graph = parsed(&v).bundle().unwrap();
    assert_eq!(
        validate_type_bindings(&graph, &TypeBindings::new())
            .unwrap_err()
            .reason,
        ErrorReason::MissingTypeBinding
    );
    validate_type_bindings(
        &graph,
        &BTreeMap::from([(
            "Money".into(),
            TypeBinding {
                rust_type: "Money".into(),
                codec: None,
            },
        )]),
    )
    .unwrap();
    let mut bindings = vec![PargClassBinding {
        rule: "document".into(),
        model_id: "Example::Node@custom".into(),
        captures: BTreeMap::from([("value".into(), "amount".into())]),
    }];
    validate_parg_bindings(&graph, &bindings).unwrap();
    bindings[0].captures.insert("bad".into(), "absent".into());
    assert_eq!(
        validate_parg_bindings(&graph, &bindings)
            .unwrap_err()
            .reason,
        ErrorReason::DanglingReference
    );
    v["value_types"]["Money"]["facets"]["unknown_facet"] = json!(true);
    refused(
        &v,
        ErrorReason::UnsupportedType,
        "/value_types/Money/facets/unknown_facet",
    );
}
#[test]
fn validates_per_instance_mixed_order_and_all_member_occurrences() {
    let mut m = model("Example::Node", "custom");
    m["ordered_content"] = json!({"member":"__lml_content","version":1});
    m["attributes"][0]["collection"] = json!({"min":0,"max":null});
    m["attributes"][0]["default"] = json!({"present":false});
    m["mappings"] = json!({"xml":{"namespace":{"mode":"unqualified"},"rules":[{"kind":"element","name":["amount"],"to":"amount","namespace":{"mode":"uri","uri":"urn:example"}}]}});
    let m: ModelDocument = serde_json::from_value(m).unwrap();
    let data = BTreeMap::from([(
        "amount".into(),
        NeutralValue::Array(vec![
            NeutralValue::String("1.0".into()),
            NeutralValue::String("2.0".into()),
        ]),
    )]);
    let tokens = vec![
        ContentToken::Element {
            attribute: "amount".into(),
            index: 1,
        },
        ContentToken::Text {
            source: TextSource::Literal {
                literal: "between".into(),
            },
        },
        ContentToken::Element {
            attribute: "amount".into(),
            index: 0,
        },
    ];
    validate_ordered_content(&m, &data, &tokens).unwrap();
    assert_eq!(
        validate_ordered_content(&m, &data, &tokens[..1])
            .unwrap_err()
            .reason,
        ErrorReason::InvalidOrderedContent
    );
    let mut duplicate = tokens.clone();
    duplicate.push(tokens[0].clone());
    assert!(validate_ordered_content(&m, &data, &duplicate).is_err());
}

#[test]
fn rejects_nested_invalid_model_defaults_and_nonstring_yaml_keys() {
    let mut v = bundle();
    v["models"]["Example::Node@custom"]["attributes"][0]["type"] =
        json!({"kind":"model","id":"Child@default"});
    v["models"]["Example::Node@custom"]["attributes"][0]["default"] =
        json!({"present":true,"value":{"amount":"1.250"}});
    v["models"]["Child@default"] = model("Child", "default");
    let good = parsed(&v);
    good.verify_checksum(good.checksum()).unwrap();
    assert_eq!(
        good.verify_checksum("sha256:wrong").unwrap_err().reason,
        ErrorReason::ChecksumMismatch
    );
    v["models"]["Example::Node@custom"]["attributes"][0]["default"]["value"]["amount"] =
        json!(true);
    refused(
        &v,
        ErrorReason::NonSerializableDefault,
        "/models/Example::Node@custom/attributes/0/default/value/amount",
    );
    assert!(load(b"lml: 1\nmodel: X\nmappings: {1: x}", InputFormat::Yaml).is_err());
    assert!(load(b"\x0c{\"lml\":1,\"model\":\"X\"}", InputFormat::Json).is_err());
}

#[test]
fn xml_compositors_namespace_resets_and_render_policies_are_validated() {
    let mut v = bundle();
    v["capabilities"] = json!(["xml"]);
    v["models"]["Example::Node@custom"]["mappings"] = json!({"xml":{
        "namespace":{"mode":"unqualified"},
        "rules":[{"kind":"element","name":["amount","alternate"],"to":"amount","namespace":{"mode":"uri","uri":"urn:amount","prefix":"a"},"render":{"nil":"omit","empty":"as_blank","default":false}}],
        "sequence":[{"kind":"sequence","members":[{"kind":"element","name":["amount"],"to":"amount"},{"kind":"choice","min":0,"max":1,"members":[{"kind":"element","name":["amount"],"to":"amount"}]}],"min":0,"max":null}]}});
    let graph = parsed(&v).bundle().unwrap();
    let xml = graph.models["Example::Node@custom"]
        .mappings
        .xml
        .as_ref()
        .unwrap();
    assert_eq!(xml.namespace, Namespace::Unqualified);
    assert!(xml.root.is_none());
    assert_eq!(
        xml.rules[0].name.as_ref().unwrap(),
        &vec!["amount", "alternate"]
    );
    v["models"]["Example::Node@custom"]["mappings"]["xml"]["rules"][0]["render"]["nil"] =
        json!("execute");
    let graph = parsed(&v).bundle().unwrap();
    assert_eq!(
        graph.models["Example::Node@custom"]
            .mappings
            .xml
            .as_ref()
            .unwrap()
            .rules[0]
            .render
            .as_ref()
            .unwrap()
            .nil,
        RenderAction::Other("execute".into())
    );
}

#[test]
fn legacy_yaml_large_integer_does_not_round_through_f64() {
    let yaml=b"lml: 1\nmodel: Legacy\nattributes:\n  - name: amount\n    type: integer\n    default: 123456789012345678901234567890123456789012345678901234567890\n";
    let snapshot = load(yaml, InputFormat::Yaml).unwrap();
    assert!(
        snapshot
            .canonical_json()
            .contains("123456789012345678901234567890123456789012345678901234567890"),
        "{}",
        snapshot.canonical_json()
    );
}

#[test]
fn raw_yaml_styles_multibyte_scalars_and_failure_cleanup_preserve_data() {
    let wire = br#"%YAML 1.2
---
lml: 1
model: Legacy
attributes:
  - {name: unicode, default: "\u00e9/\n"}
  - {name: quoted, default: '+-1'}
  - {name: plain, default: +-1}
  - name: folded
    default: >-
      &text *text !text
      second line
  - {name: radix, default: 0xffffffffffffffffffffffffffffffffffffffffffffffff}
  - {name: signed, default: -123456789012345678901234567890123456789012345678901234567890}
"#;
    let snapshot = load(wire, InputFormat::Yaml).unwrap();
    let tree: Value = serde_json::from_str(snapshot.canonical_json()).unwrap();
    assert_eq!(tree["attributes"][0]["default"], "é/\n");
    assert_eq!(tree["attributes"][1]["default"], "+-1");
    assert_eq!(tree["attributes"][2]["default"], "+-1");
    assert_eq!(
        tree["attributes"][3]["default"],
        "&text *text !text second line"
    );
    assert!(snapshot
        .canonical_json()
        .contains("-123456789012345678901234567890123456789012345678901234567890"));
    for wire in [
        b"lml: 1\nmodel: Legacy\nattributes: [".as_slice(),
        b"lml: 1\nmodel: !tag {}",
        b"lml: 1\nmodel: &anchor {}",
        b"lml: 1\nmodel: *missing",
        b"\xff",
    ] {
        assert!(load(wire, InputFormat::Yaml).is_err());
        // Each failure unwinds both parser and any initialized event. A later
        // independent load must retain its own input and parsing state.
        assert_eq!(
            load(b"lml: 1\nmodel: Legacy", InputFormat::Yaml)
                .unwrap()
                .canonical_json(),
            r#"{"lml":1,"model":"Legacy","attributes":[],"mappings":{}}"#
        );
    }
    let duplicate = load(
        b"lml_bundle: 1\nmodels:\n  A@default: {}\n  A@default: {}\n",
        InputFormat::Yaml,
    )
    .unwrap_err();
    assert_eq!(duplicate.reason, ErrorReason::InvalidDocument);
    assert_eq!(duplicate.location, "/models/A@default");
}

#[test]
fn boolean_render_policies_preserve_the_original_declared_wire_kind() {
    let mut v = bundle();
    v["models"]["Example::Node@custom"]["mappings"]["json"]["rules"][0]["render"] =
        json!({"nil":true,"empty":true,"default":false});
    let snapshot = parsed(&v);
    let graph = snapshot.bundle().unwrap();
    let policy = graph.models["Example::Node@custom"]
        .mappings
        .json
        .as_ref()
        .unwrap()
        .rules[0]
        .render
        .as_ref()
        .unwrap();
    assert_eq!(policy.nil, RenderAction::Boolean(true));
    assert_eq!(policy.empty, RenderAction::Boolean(true));
    assert!(snapshot.canonical_json().contains(r#""empty":true"#));
    let mut lexical = v.clone();
    lexical["models"]["Example::Node@custom"]["mappings"]["json"]["rules"][0]["render"] =
        json!({"nil":"nil","empty":"blank","default":false});
    assert_ne!(graph, parsed(&lexical).bundle().unwrap());
    assert_ne!(snapshot.checksum(), parsed(&lexical).checksum());
    v["models"]["Example::Node@custom"]["mappings"]["json"]["rules"][0]["render"] =
        json!({"nil":false,"empty":false,"default":false});
    let graph = parsed(&v).bundle().unwrap();
    let policy = graph.models["Example::Node@custom"]
        .mappings
        .json
        .as_ref()
        .unwrap()
        .rules[0]
        .render
        .as_ref()
        .unwrap();
    assert_eq!(policy.nil, RenderAction::Boolean(false));
    assert_eq!(policy.empty, RenderAction::Boolean(false));
}

#[test]
fn legacy_yaml_float_checksum_ambiguity_refuses_with_actionable_context() {
    let yaml=b"lml: 1\nmodel: Legacy\nattributes:\n  - name: amount\n    type: float\n    default: 1.0e+15\n";
    let error = load(yaml, InputFormat::Yaml).unwrap_err();
    assert_eq!(error.reason, ErrorReason::UnsupportedType);
    assert_eq!(error.location, "/attributes/0/default");
    assert_eq!(error.attribute.as_deref(), Some("amount"));
    assert_eq!(error.klass.as_deref(), Some("Legacy"));
    assert_eq!(error.format.as_deref(), Some("yaml"));
    assert!(error.detail.unwrap().contains("producer-canonical v1 JSON"));
    let canonical = r#"{"lml":1,"model":"Legacy","attributes":[{"name":"amount","type":"float","default":0.25}],"mappings":{}}"#;
    assert_eq!(
        load(canonical.as_bytes(), InputFormat::Json)
            .unwrap()
            .canonical_json(),
        canonical
    );
    // Quoted legacy values and portable-v2 exact values are strings, even if
    // their spelling resembles exponent notation or a floating-point number.
    assert!(load(
        b"lml: 1\nmodel: Legacy\nattributes: [{name: amount, default: '1.0e+15'}]",
        InputFormat::Yaml
    )
    .is_ok());
}

#[test]
fn canonical_values_rule_metadata_and_recursive_02d_groups_are_lossless() {
    let mut v = bundle();
    let id = "Example::Node@custom";
    v["capabilities"] = json!(["json", "xml"]);
    v["models"][id]["attributes"]
        .as_array_mut()
        .unwrap()
        .push(json!({"name":"kind","type":{"kind":"builtin","name":"string"},"values":["node"]}));
    let leaf = json!({"kind":"element","name":["amount"],"to":"amount","child_mappings":{"id":"key"},"root_mappings":{"id":"key"},"polymorphic_map":{"Example::Node":"node"},"discriminator":{"attribute":"kind","cases":{"node":id}},"render":{"nil":"empty","empty":true,"default":"explicit"}});
    v["models"][id]["mappings"]["xml"] = json!({"root":{"element":"node","mixed":true},"ordered":true,"mixed_content":true,"rules":[leaf.clone()],"sequence":[{"kind":"sequence","members":[{"kind":"choice","min":0,"max":2,"members":[leaf.clone()]}]}]});
    v["models"][id]["mappings"]["json"]["rules"][0]["child_mappings"] =
        json!({"id":"key","name":["body","name"]});
    v["models"][id]["mappings"]["json"]["rules"][0]["discriminator"] =
        leaf["discriminator"].clone();
    v["models"][id]["ordered_content"] = json!({"member":"__lml_content","version":1});
    let snapshot = parsed(&v);
    let checksum = snapshot.checksum().to_owned();
    let graph = snapshot.bundle().unwrap();
    let m = &graph.models[id];
    let xml = m.mappings.xml.as_ref().unwrap();
    assert_eq!(xml.ordered, Some(true));
    assert_eq!(xml.mixed_content, Some(true));
    assert_eq!(xml.root.as_ref().unwrap().mixed, Some(true));
    let choice = &xml.sequence.as_ref().unwrap()[0].members.as_ref().unwrap()[0];
    assert_eq!(choice.kind, XmlRuleKind::Choice);
    assert_eq!(choice.min, Some(NeutralValue::Signed(0)));
    assert_eq!(choice.max, Some(NeutralValue::Signed(2)));
    let actual = &choice.members.as_ref().unwrap()[0];
    assert_eq!(
        serde_json::to_value(actual).unwrap()["child_mappings"],
        leaf["child_mappings"]
    );
    assert_eq!(
        actual.polymorphic_map.as_ref().unwrap()["Example::Node"],
        "node"
    );
    assert_eq!(actual.discriminator.as_ref().unwrap().cases["node"], id);
    assert_eq!(
        actual.render.as_ref().unwrap().nil,
        RenderAction::Other("empty".into())
    );
    assert_eq!(
        actual.render.as_ref().unwrap().default,
        RenderAction::Other("explicit".into())
    );
    let wire = serde_json::to_value(&graph).unwrap();
    assert_eq!(
        wire["models"][id]["attributes"][1]["values"],
        json!(["node"])
    );
    assert!(wire["models"][id]["attributes"][1].get("enum").is_none());
    assert_eq!(
        wire["models"][id]["mappings"]["xml"]["sequence"][0]["members"][0]["members"][0]
            ["root_mappings"],
        leaf["root_mappings"]
    );
    assert_eq!(snapshot.checksum(), checksum);
    let neutral: NeutralValue = serde_json::from_value(v.clone()).unwrap();
    let yaml = serde_yaml::to_string(&neutral).unwrap();
    let from_yaml = load(yaml.as_bytes(), InputFormat::Yaml).unwrap();
    assert_eq!(from_yaml.checksum(), snapshot.checksum());
    assert_eq!(from_yaml.bundle().unwrap(), graph);
    let mut dangling = v.clone();
    dangling["models"][id]["mappings"]["xml"]["sequence"][0]["members"][0]["members"][0]
        ["discriminator"]["cases"]["node"] = json!("Missing@default");
    assert_eq!(
        load(&bytes(&dangling), InputFormat::Json)
            .unwrap_err()
            .reason,
        ErrorReason::DanglingReference
    );
}
#[test]
fn canonical_optional_attributes_and_values_preserve_source_identity() {
    let v = json!({"lml_bundle":1,"roots":["Minimal@default"],"models":{"Minimal@default":{"lml":2,"model":"Minimal","register":"default","attributes":[{"name":"kind","type":{"kind":"builtin","name":"string"},"values":["a","b"]}]}},"value_types":{},"capabilities":[]});
    let snapshot = parsed(&v);
    let graph = snapshot.bundle().unwrap();
    assert!(!graph.models["Minimal@default"].attributes[0].required);
    assert!(matches!(
        graph.models["Minimal@default"].attributes[0].default,
        DefaultValue::Absent
    ));
    assert!(snapshot.canonical_json().contains("\"values\""));
    let wire = serde_json::to_value(graph).unwrap();
    assert_eq!(
        wire["models"]["Minimal@default"]["attributes"][0]["values"],
        json!(["a", "b"])
    );
    assert!(wire["models"]["Minimal@default"]["attributes"][0]
        .get("enum")
        .is_none());
}
#[test]
fn malformed_02d_flags_groups_and_policies_refuse_without_floating_conversion() {
    let mut v = bundle();
    let id = "Example::Node@custom";
    v["models"][id]["mappings"]["json"]["ordered"] = json!("true");
    assert_eq!(
        load(&bytes(&v), InputFormat::Json).unwrap_err().reason,
        ErrorReason::UnsupportedMapping
    );
    v = bundle();
    v["capabilities"] = json!(["xml"]);
    v["models"][id]["mappings"] = json!({"xml":{"sequence":[1]}});
    assert!(load(&bytes(&v), InputFormat::Json).is_err());
    v = bundle();
    v["models"][id]["mappings"]["json"]["rules"][0]["render"] = json!({"empty":1.25});
    assert_eq!(
        load(&bytes(&v), InputFormat::Json).unwrap_err().reason,
        ErrorReason::InvalidDocument
    );
    v = bundle();
    v["models"][id]["mappings"]["toml"] = json!({"rules":[]});
    assert_eq!(
        load(&bytes(&v), InputFormat::Json).unwrap_err().reason,
        ErrorReason::CapabilityMissing
    );
}
#[test]
fn actual_04b_preservation_schema_json_yaml_loads_with_pinned_identity() {
    use sha2::{Digest, Sha256};
    let json = include_bytes!("../src/lml/fixtures/02d-preservation.schema.json");
    let yaml = include_bytes!("../src/lml/fixtures/02d-preservation.schema.yaml");
    assert_eq!(
        format!("{:x}", Sha256::digest(json)),
        "66b86b75d36568c1c353a2d5cf32d0e6562e21e7dfea0004a757d0f774761264"
    );
    assert_eq!(
        format!("{:x}", Sha256::digest(yaml)),
        "cf40d46e901e4a8e7bb4b7f7ad4365ffec687bef81832663dc136626b3963c3d"
    );
    let a = load(json, InputFormat::Json).unwrap();
    let b = load(yaml, InputFormat::Yaml).unwrap();
    assert_eq!(
        a.checksum(),
        "sha256:81eaac6230a81d4285fb4ea743f202234ef3f3c48482ca60ce9e9b642a5622e5"
    );
    assert_eq!(a.checksum(), b.checksum());
    assert_eq!(a.bundle().unwrap(), b.bundle().unwrap());
    let graph = a.bundle().unwrap();
    assert_eq!(graph.models.len(), 8);
    assert!(graph.models["LmlPreservation::Mixed@default"]
        .ordered_content
        .is_some());
    let states = &graph.models["LmlPreservation::States@default"]
        .mappings
        .json
        .as_ref()
        .unwrap()
        .rules;
    assert!(states
        .iter()
        .any(|r| r.render.as_ref().is_some_and(
            |p| p.empty == RenderAction::Boolean(true) && p.nil == RenderAction::Boolean(true)
        )));
}

#[test]
fn exact_integer_defaults_compare_to_typed_numeric_enum_metadata() {
    let mut schema: Value = serde_json::from_slice(include_bytes!(
        "../src/lml/fixtures/02d-preservation.schema.json"
    ))
    .unwrap();
    schema["models"]["LmlPreservation::TypedEnum@default"]["attributes"][0]["default"] =
        json!({"present":true,"value":1});
    let snapshot = parsed(&schema);
    let graph = snapshot.bundle().unwrap();
    assert_eq!(
        graph.models["LmlPreservation::TypedEnum@default"].attributes[0].enumeration,
        Some(vec![NeutralValue::Signed(1), NeutralValue::Signed(2)])
    );
    assert!(snapshot.canonical_json().contains("\"values\":[1,2]"));
}

/// Case -> Ruby verdict -> Rust verdict. `ruby` was measured with
/// `tests/fixtures/ruby_parity_measure.rb` against
/// lutaml-model 7e23cd7 `DocumentValidator`; `rust` is pinned here. Where
/// the accept/reject outcomes differ the row must carry a `divergence` note
/// (Rust deliberately stricter); an unexplained divergence fails the test.
#[test]
fn ruby_document_validator_parity_table() {
    let table: Vec<Value> =
        serde_json::from_str(include_str!("fixtures/ruby-parity.json")).unwrap();
    let mut dump = BTreeMap::new();
    let mut failures = Vec::new();
    for row in &table {
        let name = row["name"].as_str().unwrap();
        let input = match row.get("raw") {
            Some(raw) => raw.as_str().unwrap().as_bytes().to_vec(),
            None => bytes(&row["doc"]),
        };
        let (rust, checksum) = match load(&input, InputFormat::Json) {
            Ok(snapshot) => ("OK".to_string(), Some(snapshot.checksum().to_string())),
            Err(e) => (
                format!(
                    "ERR {}",
                    serde_json::to_value(&e.reason).unwrap().as_str().unwrap()
                ),
                None,
            ),
        };
        dump.insert(
            name.to_string(),
            json!({"verdict": rust, "checksum": checksum}),
        );
        if std::env::var_os("PARITY_DUMP").is_some() {
            continue;
        }
        let ruby = row["ruby"].as_str().unwrap();
        if rust != row["rust"].as_str().unwrap() {
            failures.push(format!(
                "{name}: pinned rust {} but got {rust}",
                row["rust"]
            ));
        }
        let same_outcome = (ruby == "OK") == (rust == "OK");
        let noted = row
            .get("divergence")
            .and_then(Value::as_str)
            .is_some_and(|n| !n.is_empty());
        if same_outcome && noted {
            failures.push(format!("{name}: stale divergence note"));
        }
        if !same_outcome && !noted {
            failures.push(format!(
                "{name}: ruby {ruby} vs rust {rust} without divergence note"
            ));
        }
        if let (Some(expected), Some(actual)) =
            (row.get("ruby_checksum").and_then(Value::as_str), &checksum)
        {
            if expected != actual {
                failures.push(format!("{name}: checksum {actual} != ruby {expected}"));
            }
        }
    }
    if let Some(path) = std::env::var_os("PARITY_DUMP") {
        std::fs::write(path, serde_json::to_string_pretty(&dump).unwrap()).unwrap();
        return;
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

fn v1_json(default: &str) -> String {
    format!(
        r#"{{"lml":1,"model":"Legacy","attributes":[{{"name":"amount","default":{default}}}],"mappings":{{}}}}"#
    )
}

/// M3: a v1 JSON float is admitted only in the spelling every Ruby JSON
/// generator prints, so the checksum is Ruby's. Expected digests were measured
/// with `Lutaml::Model::Lml::Document.from_hash(h).checksum` (also pinned by the
/// parity table); the refused spellings re-render differently in Ruby.
#[test]
fn v1_json_floats_keep_ruby_checksums_or_refuse() {
    for good in [
        "1.5",
        "1.0",
        "100.0",
        "-0.0",
        "0.0001",
        "123.456",
        "-1.25",
        "999999999999999.9",
    ] {
        let snapshot = load(v1_json(good).as_bytes(), InputFormat::Json).unwrap();
        assert!(
            snapshot
                .canonical_json()
                .contains(&format!("\"default\":{good}")),
            "{good}"
        );
    }
    for bad in [
        "1.50", "0.10", "1e2", "1E2", "1.5e0", "1e-5", "1.0e-5", "1e20", "1.0e+20", "1e15",
        "1e999", "0.00001", "9.999e-5",
    ] {
        let error = load(v1_json(bad).as_bytes(), InputFormat::Json).expect_err(bad);
        assert_eq!(error.reason, ErrorReason::UnsupportedType, "{bad}");
        assert_eq!(error.location, "/attributes/0/default", "{bad}");
        assert_eq!(error.format.as_deref(), Some("json"), "{bad}");
        assert_eq!(error.attribute.as_deref(), Some("amount"), "{bad}");
    }
    // Ruby parses -0 as the integer 0, and keeps big integers exact.
    let negative_zero = load(v1_json("-0").as_bytes(), InputFormat::Json).unwrap();
    assert!(negative_zero.canonical_json().contains("\"default\":0}"));
    load(
        v1_json("123456789012345678901234567890").as_bytes(),
        InputFormat::Json,
    )
    .unwrap();
}

/// L1: object-key and enum duplicate scans are linear, and a YAML integer
/// scalar is never run through a quadratic BigInt parse.
#[test]
fn large_documents_load_in_bounded_time() {
    use std::time::{Duration, Instant};
    // Wall-clock limits flake on a loaded machine, so the scans are checked by
    // how they scale: 4x the entries must cost well under 16x the time (what a
    // quadratic scan costs). A linear scan measures about 4x.
    let limit = Duration::from_secs(20);
    let within = |label: &str, limit: Duration, run: &dyn Fn()| {
        let started = Instant::now();
        run();
        assert!(
            started.elapsed() < limit,
            "{label} took {:?}",
            started.elapsed()
        );
    };
    let timed = |text: &str, format: InputFormat| {
        let started = Instant::now();
        load(text.as_bytes(), format).unwrap();
        started.elapsed().as_secs_f64()
    };
    let scales_linearly = |label: &str, format: InputFormat, build: &dyn Fn(usize) -> String| {
        let (small, large) = (build(20_000), build(80_000));
        timed(&small, format); // warm up allocator and code paths
        let ratio = timed(&large, format) / timed(&small, format).max(1e-3);
        assert!(
            ratio < 10.0,
            "{label}: 4x entries took {ratio:.1}x the time"
        );
    };
    let json_types = |n: usize| {
        let types: String = (0..n)
            .map(|i| format!(r#""v{i}":{{"base":"string","facets":{{}}}}"#))
            .collect::<Vec<_>>()
            .join(",");
        format!(
            r#"{{"lml_bundle":1,"roots":[],"models":{{}},"value_types":{{{types}}},"capabilities":[]}}"#
        )
    };
    scales_linearly("JSON keys", InputFormat::Json, &json_types);
    let yaml_types = |n: usize| {
        let yaml = (0..n).fold(String::new(), |mut out, i| {
            out.push_str(&format!("  v{i}: {{base: string, facets: {{}}}}\n"));
            out
        });
        format!("lml_bundle: 1\nroots: []\nmodels: {{}}\ncapabilities: []\nvalue_types:\n{yaml}")
    };
    scales_linearly("YAML keys", InputFormat::Yaml, &yaml_types);
    let enum_doc = |n: usize| {
        let values: Vec<String> = (0..n).map(|i| format!("\"e{i}\"")).collect();
        format!(
            r#"{{"lml":2,"model":"Foo","register":"default","attributes":[{{"name":"a","type":{{"kind":"builtin","name":"string"}},"values":[{}]}}]}}"#,
            values.join(",")
        )
    };
    scales_linearly("enum values", InputFormat::Json, &enum_doc);
    let attr = enum_doc(20_000);
    let duplicate = attr.replace("\"e7\"", "\"e8\"");
    assert_eq!(
        load(duplicate.as_bytes(), InputFormat::Json)
            .unwrap_err()
            .reason,
        ErrorReason::InvalidDocument
    );
    let digits = "9".repeat(800_000);
    let big = format!("lml: 1\nmodel: X\nattributes: [{{name: n, default: {digits}}}]\n");
    within("800k-digit YAML integer", limit, &|| {
        load(big.as_bytes(), InputFormat::Yaml).unwrap();
    });
    let hex = format!(
        "lml: 1\nmodel: X\nattributes: [{{name: n, default: 0x{}}}]\n",
        "f".repeat(5000)
    );
    within("oversized radix integer", limit, &|| {
        assert_eq!(
            load(hex.as_bytes(), InputFormat::Yaml).unwrap_err().reason,
            ErrorReason::InvalidDocument
        );
    });
}

/// L3: YAML 1.2 floats need a digit; dot/sign/exponent fragments stay strings.
#[test]
fn yaml_plain_scalars_without_digits_are_strings() {
    for text in [
        ".", "-.", "+.", ".e5", "-.e5", ".E-3", "e5", "-e5", "1e", "1e+", "..", "1.2.3",
    ] {
        let yaml = format!("lml: 1\nmodel: X\nattributes: [{{name: n, default: {text}}}]\n");
        let snapshot = load(yaml.as_bytes(), InputFormat::Yaml).unwrap();
        assert_eq!(
            snapshot.tree().get("attributes").unwrap().array().unwrap()[0].get("default"),
            Some(&Node::String(text.to_string())),
            "{text}"
        );
    }
    // A genuine float is still a float, which a v1 YAML identity refuses.
    for text in [".5", "5.", "-.5", "1e5", "5.e3"] {
        let yaml = format!("lml: 1\nmodel: X\nattributes: [{{name: n, default: {text}}}]\n");
        let error = load(yaml.as_bytes(), InputFormat::Yaml).unwrap_err();
        assert_eq!(error.reason, ErrorReason::UnsupportedType, "{text}");
    }
}

/// M2: `mappings.toml` validates like json/yaml; the typed view then refuses it
/// by name rather than dropping it.
#[test]
fn toml_mapping_validates_and_the_typed_view_refuses_it_by_name() {
    let mut v = bundle();
    v["capabilities"] = json!(["json", "toml"]);
    v["models"]["Example::Node@custom"]["mappings"]["toml"] =
        json!({"rules":[{"key":["amount"],"to":"amount"}]});
    let snapshot = parsed(&v);
    let error = snapshot.bundle().unwrap_err();
    assert_eq!(error.reason, ErrorReason::UnsupportedFormat);
    assert_eq!(error.location, "/models/Example::Node@custom/mappings/toml");
    assert_eq!(error.format.as_deref(), Some("toml"));
    v["models"]["Example::Node@custom"]["mappings"]["toml"]["rules"][0]["to"] = json!("missing");
    refused(
        &v,
        ErrorReason::DanglingReference,
        "/models/Example::Node@custom/mappings/toml/rules/0/to",
    );
}

/// 02g: the `any_attribute` XML rule is accepted exactly where Ruby accepts it
/// (parity rows `v2_any_attr_*`) and the typed view carries it as a rule kind.
#[test]
fn any_attribute_rule_is_validated_like_ruby_and_typed() {
    let rule = json!({"kind":"any_attribute","to":"other"});
    let mut v = bundle();
    v["capabilities"] = json!(["json", "xml"]);
    let model = &mut v["models"]["Example::Node@custom"];
    model["attributes"]
        .as_array_mut()
        .unwrap()
        .push(json!({"name":"other","type":{"kind":"builtin","name":"hash"}}));
    model["mappings"]["xml"] = json!({"rules":[rule]});
    let at = "/models/Example::Node@custom/mappings/xml/rules";
    let typed = parsed(&v).bundle().unwrap();
    let rules = &typed.models["Example::Node@custom"]
        .mappings
        .xml
        .as_ref()
        .unwrap()
        .rules;
    assert_eq!(rules[0].kind, XmlRuleKind::AnyAttribute);
    assert_eq!(rules[0].to, "other");
    let rules =
        |v: &mut Value| v["models"]["Example::Node@custom"]["mappings"]["xml"]["rules"].take();
    let mut w = v.clone();
    w["models"]["Example::Node@custom"]["mappings"]["xml"]["rules"] = json!([rule, rule]);
    refused(&w, ErrorReason::UnsupportedMapping, &format!("{at}/1"));
    let mut w = v.clone();
    w["models"]["Example::Node@custom"]["mappings"]["xml"]["rules"][0]["to"] = json!("amount");
    refused(&w, ErrorReason::UnsupportedMapping, &format!("{at}/0/to"));
    let mut w = v.clone();
    w["models"]["Example::Node@custom"]["mappings"]["xml"]["rules"][0]["name"] = json!(["x"]);
    refused(&w, ErrorReason::UnsupportedMapping, &format!("{at}/0/name"));
    let mut w = v.clone();
    w["models"]["Example::Node@custom"]["mappings"]["xml"]["rules"][0]["to"] = json!("missing");
    refused(&w, ErrorReason::DanglingReference, &format!("{at}/0/to"));
    let mut w = v.clone();
    let moved = rules(&mut w);
    w["models"]["Example::Node@custom"]["mappings"]["json"] = json!({"rules":moved});
    refused(
        &w,
        ErrorReason::UnsupportedMapping,
        "/models/Example::Node@custom/mappings/json/rules/0",
    );
    let mut w = v.clone();
    let moved = rules(&mut w);
    w["models"]["Example::Node@custom"]["mappings"]["xml"] =
        json!({"sequence":[{"kind":"sequence","members":moved}]});
    refused(
        &w,
        ErrorReason::UnsupportedMapping,
        "/models/Example::Node@custom/mappings/xml/sequence/0/members/0/kind",
    );
}

/// Ruby review-R2 rules (attribute names, reserved members, nesting depth,
/// mapping scalars, duplicate keys): 178 Ruby-generated cases with the expected
/// reason and, where Ruby reports one, location (bundle text errors have none).
#[test]
fn ruby_r2_refusal_cases_match_ruby() {
    let cases: Vec<Value> =
        serde_json::from_str(include_str!("fixtures/r2_refusals.json")).unwrap();
    assert_eq!(cases.len(), 178);
    for ty in [
        "string",
        "token",
        "uri",
        "email",
        "hostname",
        "ip",
        "boolean",
        "integer",
        "decimal",
        "float",
        "rational",
        "date",
        "time",
        "date_time",
        "time_without_date",
        "duration",
        "qname",
        "base64_binary",
        "hex_binary",
        "symbol",
        "hash",
        "email_address",
        "ip_v4_address",
        "ip_v6_address",
        "uuid",
        "reference",
        "uri_reference",
        "date_with_timezone",
        "date_time_with_timezone",
        "day_time_duration",
        "year_month_duration",
        "positive_integer",
        "non_negative_integer",
    ] {
        for kind in ["ok", "refused"] {
            let name = format!("typed_default_{kind}_{ty}");
            assert!(cases.iter().any(|c| c["name"] == name), "{name} missing");
        }
    }
    let mut failures = Vec::new();
    for case in &cases {
        let name = case["name"].as_str().unwrap();
        let input = case["input"].as_str().unwrap().as_bytes();
        match (load(input, InputFormat::Json), case["expect"].as_object()) {
            (Ok(_), None) => {}
            (Ok(_), Some(_)) => failures.push(format!("{name}: accepted, Ruby refuses")),
            (Err(e), None) => failures.push(format!(
                "{name}: refused {:?} at {}, Ruby accepts",
                e.reason, e.location
            )),
            (Err(e), Some(expect)) => {
                let reason = serde_json::to_value(&e.reason).unwrap();
                if reason != expect["reason"] {
                    failures.push(format!("{name}: reason {reason} != {}", expect["reason"]));
                }
                if let Some(location) = expect["location"].as_str() {
                    if e.location != location {
                        failures.push(format!("{name}: location {} != {location}", e.location));
                    }
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
