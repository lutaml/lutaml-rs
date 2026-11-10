use lutaml_rs::*;
use serde_json::json;
use std::collections::BTreeMap;

#[test]
fn portable_graph_preserves_order_exact_defaults_and_recursive_identity() {
    let wire = json!({"lml_bundle":1,"roots":["Example::Node@default"],"models":{
      "Example::Node@default":{"lml":2,"model":"Example::Node","register":"default",
      "attributes":[
        {"name":"children","type":{"kind":"model","id":"Example::Node@default"},"required":false,"collection":{"min":0,"max":null},"default":{"present":false},"initialize_empty":false},
        {"name":"amount","type":{"kind":"builtin","name":"decimal"},"required":false,"collection":false,"default":{"present":true,"value":"9007199254740993.000"},"initialize_empty":false}],
      "mappings":{"xml":{"root":{"element":"node"},"namespace":{"mode":"inherit"},"rules":[{"kind":"element","name":["node","alias"],"to":"children","namespace":{"mode":"unqualified"},"render":{"nil":"omit","empty":"omit","default":false}}]}},
      "ordered_content":{"member":"__lml_content","version":1}}},"value_types":{},"capabilities":["xml"]});
    let bundle: Bundle = serde_json::from_value(wire.clone()).unwrap();
    let model = &bundle.models["Example::Node@default"];
    assert_eq!(model.attributes[0].name, "children");
    assert_eq!(
        model.attributes[1].default,
        DefaultValue::Present(NeutralValue::String("9007199254740993.000".into()))
    );
    assert_eq!(serde_json::to_value(&bundle).unwrap(), wire);
}

#[test]
fn default_presence_does_not_collapse_null_false_or_empty() {
    for value in [json!(null), json!(false), json!(""), json!([]), json!({})] {
        let wire = json!({"present":true,"value":value});
        let default: DefaultValue = serde_json::from_value(wire.clone()).unwrap();
        assert_ne!(default, DefaultValue::Absent);
        assert_eq!(serde_json::to_value(default).unwrap(), wire);
    }
    for invalid in [
        json!({"present":true}),
        json!({"present":false,"value":null}),
        json!({}),
        json!({"present":false,"code":"execute"}),
    ] {
        assert!(serde_json::from_value::<DefaultValue>(invalid).is_err());
    }
    assert_ne!(Presence::<String>::Absent, Presence::Null);
    assert_ne!(Presence::Null, Presence::Value(String::new()));
}

#[test]
fn neutral_numbers_have_no_floating_fallback() {
    assert!(serde_json::from_str::<NeutralValue>("1.25").is_err());
    assert!(serde_json::from_str::<NeutralValue>("18446744073709551616").is_err());
    assert_eq!(
        serde_json::from_str::<NeutralValue>("18446744073709551615").unwrap(),
        NeutralValue::Unsigned(u64::MAX)
    );
    let exact: ExactInteger = "1234567890123456789012345678901234567890".parse().unwrap();
    let ratio = ExactRational::new(exact.clone(), 10.into());
    assert_eq!(
        ratio * ExactRational::from_integer(10.into()),
        ExactRational::from_integer(exact)
    );
    assert!(serde_json::from_value::<Collection>(json!(true)).is_err());
    assert_eq!(
        serde_json::from_value::<TypeRef>(json!({"kind":"builtin","name":"float"})).unwrap(),
        TypeRef::Builtin {
            name: BuiltinType::Float
        }
    );
    assert!(serde_json::from_value::<TypeRef>(json!({"kind":"builtin","name":"double"})).is_err());
}

#[test]
fn content_tokens_preserve_mixed_instance_order_and_namespace_states() {
    let tokens = json!([
      {"kind":"text","literal":"before"},
      {"kind":"element","attribute":"children","index":1},
      {"kind":"cdata","attribute":"content","index":0},
      {"kind":"element","attribute":"children","index":0}]);
    let parsed: Vec<ContentToken> = serde_json::from_value(tokens.clone()).unwrap();
    assert_eq!(serde_json::to_value(parsed).unwrap(), tokens);
    for wire in [
        json!({"mode":"inherit"}),
        json!({"mode":"unqualified"}),
        json!({"mode":"uri","uri":"urn:example","prefix":"e"}),
    ] {
        let namespace: Namespace = serde_json::from_value(wire.clone()).unwrap();
        assert_eq!(serde_json::to_value(namespace).unwrap(), wire);
    }
}

#[test]
fn error_and_artifact_contracts_keep_wire_names_and_pins() {
    let error = ContractError {
        reason: ErrorReason::NonterminatingRational,
        model_id: Some("A@default".into()),
        attribute: Some("amount".into()),
        format: Some("json".into()),
        rule: None,
        location: "/models/A@default/attributes/0".into(),
        klass: Some("A".into()),
        detail: Some("decimal output".into()),
    };
    assert_eq!(
        serde_json::to_value(&error).unwrap()["reason"],
        "nonterminating_rational"
    );
    assert!(error.to_string().contains("/models/A@default/attributes/0"));
    let manifest = Manifest {
        manifest: 1,
        lml_checksum: "sha256:source".into(),
        schema_version: 2,
        generator_version: "test-pin".into(),
        target: Target::Rust,
        capabilities: vec!["json".into()],
        corpora: vec![CorpusPin {
            generation: "generation-1".into(),
            sha256: "corpus".into(),
        }],
        files: vec![FilePin {
            path: "src/models.rs".into(),
            sha256: "file".into(),
        }],
        dependencies: vec![DependencyPin {
            name: "runtime".into(),
            revision: "revision".into(),
            sha256: "dependency".into(),
        }],
    };
    let encoded = serde_json::to_value(&manifest).unwrap();
    assert_eq!(encoded["target"], "rust");
    assert!(encoded.get("digest").is_none());
    assert_eq!(
        serde_json::from_value::<Manifest>(encoded).unwrap(),
        manifest
    );
}

#[test]
fn public_emitter_interfaces_and_compile_time_parg_hook_are_usable() {
    struct Emitter;
    impl PargBindingEmitter for Emitter {
        fn generate_parg_bindings(
            &self,
            bundle: &Bundle,
            bindings: &[PargClassBinding],
        ) -> ContractResult<SourceMap> {
            let binding = &bindings[0];
            assert!(bundle.models.contains_key(&binding.model_id));
            Ok(BTreeMap::from([(
                "src/parser_bindings.rs".into(),
                format!(
                    "{}:{}:{}",
                    binding.rule, binding.model_id, binding.captures["title"]
                ),
            )]))
        }
    }
    let bundle: Bundle = serde_json::from_value(json!({"lml_bundle":1,"roots":["Doc@default"],"models":{"Doc@default":{"lml":2,"model":"Doc","register":"default","attributes":[],"mappings":{}}},"value_types":{},"capabilities":[]})).unwrap();
    let output = Emitter
        .generate_parg_bindings(
            &bundle,
            &[PargClassBinding {
                rule: "document".into(),
                model_id: "Doc@default".into(),
                captures: BTreeMap::from([("title".into(), "heading".into())]),
            }],
        )
        .unwrap();
    assert_eq!(
        output["src/parser_bindings.rs"],
        "document:Doc@default:heading"
    );
    fn accepts_interfaces(
        _: &dyn BundleLoader,
        _: &dyn ModelEmitter,
        _: &dyn JsonCodecEmitter,
        _: &dyn XmlCodecEmitter,
    ) {
    }
    let _ = accepts_interfaces;
}
