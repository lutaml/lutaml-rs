#[path = "../src/xml/contract.rs"]
#[allow(dead_code)]
mod contract;
#[path = "../src/contracts.rs"]
#[allow(dead_code)]
mod contracts;
#[path = "../src/xml/projection.rs"]
mod projection;

use contract::*;
use contracts::*;
use projection::*;
use serde_json::{json, Value};
use std::collections::BTreeMap;

const ID: &str = "Example::Node@default";
fn fixture() -> Value {
    json!({"lml_bundle":1,"roots":[ID],"models":{ID:{
        "lml":2,"model":"Example::Node","register":"default",
        "attributes":[
            {"name":"id","type":{"kind":"builtin","name":"string"},"required":true,"collection":false,"default":{"present":false},"initialize_empty":false},
            {"name":"children","type":{"kind":"model","id":ID},"required":false,"collection":{"min":0,"max":null},"default":{"present":false},"initialize_empty":true},
            {"name":"text","type":{"kind":"builtin","name":"string"},"required":false,"collection":false,"default":{"present":true,"value":""},"initialize_empty":false}
        ],
        "mappings":{"xml":{"root":{"element":"node"},"namespace":{"mode":"uri","uri":"urn:node","prefix":"n"},
            "rules":[
                {"kind":"attribute","name":["id","identifier"],"to":"id","namespace":{"mode":"unqualified"}},
                {"kind":"element","name":["child","legacy"],"to":"children","namespace":{"mode":"inherit"},"render":{"nil":"as_nil","empty":"blank","default":false},"discriminator":{"attribute":"kind","cases":{"node":ID}}},
                {"kind":"content","to":"text","namespace":{"mode":"inherit"},"cdata":true}
            ],
            "sequence":{"kind":"sequence","members":["children",{"kind":"choice","members":["children"],"min":0,"max":1}],"min":0,"max":null},
            "choice":{"kind":"choice","members":["children"],"min":0,"max":1}
        }},"ordered_content":{"member":"__lml_content","version":1}
    }},"value_types":{},"capabilities":["xml"]})
}
fn bundle(value: Value) -> Bundle {
    serde_json::from_value(value).unwrap()
}
fn plan() -> Projection {
    compile_projection(&bundle(fixture()), ID).unwrap()
}
fn tokens() -> Vec<ContentToken> {
    vec![
        ContentToken::Element {
            attribute: "children".into(),
            index: 1,
        },
        ContentToken::Text {
            source: TextSource::Literal {
                literal: " & ".into(),
            },
        },
        ContentToken::Cdata {
            source: TextSource::Member {
                attribute: "text".into(),
                index: 0,
            },
        },
        ContentToken::Element {
            attribute: "children".into(),
            index: 0,
        },
    ]
}
fn counts() -> BTreeMap<String, usize> {
    BTreeMap::from([("children".into(), 2), ("text".into(), 1), ("id".into(), 1)])
}

#[test]
fn compile_preserves_static_batch_and_all_field_metadata() {
    let source = bundle(fixture());
    let snapshot = source.clone();
    let p = compile_projection(&source, ID).unwrap();
    assert_eq!(source, snapshot);
    assert_eq!(p.operations.len(), 7);
    assert_eq!(
        p.operations[0],
        Operation::Root {
            element: Some("node".into()),
            namespace: Namespace::Uri {
                uri: "urn:node".into(),
                prefix: Some("n".into())
            }
        }
    );
    let fields: Vec<_> = p
        .operations
        .iter()
        .filter_map(|op| {
            if let Operation::Field(f) = op {
                Some(f)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(
        fields.iter().map(|f| f.member.as_str()).collect::<Vec<_>>(),
        ["id", "children", "text"]
    );
    assert_eq!(fields[0].aliases, ["id", "identifier"]);
    assert_eq!(fields[0].namespace, Namespace::Unqualified);
    assert_eq!(fields[1].namespace, Namespace::Inherit);
    assert_eq!(fields[1].aliases, ["child", "legacy"]);
    assert_eq!(
        fields[1].render,
        Some(RenderPolicy {
            nil: RenderAction::AsNil,
            empty: RenderAction::Blank,
            default: false.into()
        })
    );
    assert_eq!(fields[0].render, None);
    assert_eq!(fields[1].attribute, source.models[ID].attributes[1]);
    assert_eq!(fields[1].discriminator.as_ref().unwrap().cases["node"], ID);
    assert!(fields[2].cdata);
    assert_eq!(
        source.models[ID]
            .mappings
            .xml
            .as_ref()
            .unwrap()
            .sequence
            .as_ref()
            .unwrap()
            .len(),
        2
    );
    assert!(matches!(
        &p.operations[4],
        Operation::Sequence(Compositor::Sequence {
            min: 0,
            max: None,
            ..
        })
    ));
    assert!(matches!(
        &p.operations[5],
        Operation::Choice(Compositor::Choice {
            min: 0,
            max: Some(1),
            ..
        })
    ));
    assert_eq!(
        p.operations[6],
        Operation::InstanceOrder {
            member: "__lml_content".into()
        }
    );
}

#[test]
fn fragment_and_every_namespace_mode_survive() {
    for ns in [
        json!({"mode":"inherit"}),
        json!({"mode":"unqualified"}),
        json!({"mode":"uri","uri":"urn:x"}),
    ] {
        let mut v = fixture();
        v["models"][ID]["mappings"]["xml"]["root"] = Value::Null;
        v["models"][ID]["mappings"]["xml"]["namespace"] = ns.clone();
        assert_eq!(
            compile_projection(&bundle(v), ID).unwrap().operations[0],
            Operation::Root {
                element: None,
                namespace: serde_json::from_value(ns).unwrap()
            }
        );
    }
}

#[test]
fn exact_instance_order_and_literal_text_are_preserved() {
    let ts = tokens();
    assert_eq!(validate_order(&plan(), &ts, &counts()).unwrap(), ts);
}

#[test]
fn invalid_or_incomplete_instance_references_fail_before_writing() {
    let original = tokens();
    let mut duplicate = original.clone();
    duplicate.push(original[0].clone());
    let mut missing = original.clone();
    missing.pop();
    let mut invalid = original.clone();
    invalid[0] = ContentToken::Element {
        attribute: "children".into(),
        index: 2,
    };
    let mut wrong_kind = original.clone();
    wrong_kind[0] = ContentToken::Element {
        attribute: "text".into(),
        index: 0,
    };
    let mut unknown = original.clone();
    unknown[0] = ContentToken::Element {
        attribute: "absent".into(),
        index: 0,
    };
    for ts in [duplicate, missing, invalid, wrong_kind, unknown] {
        let e = validate_order(&plan(), &ts, &counts()).unwrap_err();
        assert_eq!(e.reason, ErrorReason::InvalidOrderedContent);
        assert_eq!(e.model_id.as_deref(), Some(ID));
        assert!(e.location.starts_with("/__lml_content/"));
    }
}

#[test]
fn malformed_mapping_metadata_is_rejected_by_name_and_location() {
    let cases = [
        (
            "/models/Example::Node@default/mappings/xml/root/element",
            json!(""),
            "root",
        ),
        (
            "/models/Example::Node@default/mappings/xml/rules/0/cdata",
            json!(true),
            "cdata",
        ),
        (
            "/models/Example::Node@default/mappings/xml/rules/1/name",
            json!([]),
            "name",
        ),
        (
            "/models/Example::Node@default/mappings/xml/rules/1/to",
            json!("unknown"),
            "to",
        ),
        (
            "/models/Example::Node@default/mappings/xml/rules/1/namespace",
            json!({"mode":"uri","uri":""}),
            "namespace",
        ),
        (
            "/models/Example::Node@default/mappings/xml/rules/1/prefix",
            json!("bad:prefix"),
            "prefix",
        ),
        (
            "/models/Example::Node@default/mappings/xml/sequence/max",
            json!(0),
            "sequence",
        ),
    ];
    for (path, value, name) in cases {
        let mut v = fixture();
        if path.ends_with("/cdata") {
            v["models"][ID]["mappings"]["xml"]["rules"][0]["cdata"] = value;
        } else if path.ends_with("/prefix") {
            v["models"][ID]["mappings"]["xml"]["rules"][1]["prefix"] = value;
        } else {
            *v.pointer_mut(path).unwrap() = value;
        }
        if name == "sequence" {
            v["models"][ID]["mappings"]["xml"]["sequence"]["min"] = json!(1);
        }
        let e = compile_projection(&bundle(v), ID).unwrap_err();
        assert_eq!(e.reason, ErrorReason::UnsupportedMapping, "{path}");
        assert!(e.location.contains(name), "{e}");
        assert!(e.detail.is_some());
        assert_eq!(e.format.as_deref(), Some("xml"));
    }
}

#[test]
fn dangling_discriminator_and_reserved_member_fail_explicitly() {
    let mut v = fixture();
    v["models"][ID]["mappings"]["xml"]["rules"][1]["discriminator"]["cases"]["node"] =
        json!("Missing@default");
    let e = compile_projection(&bundle(v), ID).unwrap_err();
    assert_eq!(e.reason, ErrorReason::DanglingReference);
    assert!(e.location.ends_with("/discriminator"));
    let mut v = fixture();
    let mut a = v["models"][ID]["attributes"][0].clone();
    a["name"] = json!("__lml_content");
    v["models"][ID]["attributes"]
        .as_array_mut()
        .unwrap()
        .push(a);
    assert_eq!(
        compile_projection(&bundle(v), ID).unwrap_err().reason,
        ErrorReason::ReservedMember
    );
}

#[test]
fn undeclared_order_and_unsupported_format_are_typed_errors() {
    let mut p = plan();
    p.operations.pop();
    assert_eq!(
        validate_order(&p, &tokens(), &counts()).unwrap_err().reason,
        ErrorReason::InvalidOrderedContent
    );
    let mut v = fixture();
    v["models"][ID]["mappings"] = json!({});
    assert_eq!(
        compile_projection(&bundle(v), ID).unwrap_err().reason,
        ErrorReason::UnsupportedFormat
    );
}

struct RecordingAdapter;
impl XmlReader for RecordingAdapter {
    type Input = Vec<Operation>;
    type Model = Vec<Operation>;
    fn read_batch(&mut self, p: &Projection, input: Self::Input) -> ContractResult<Self::Model> {
        assert_eq!(p.operations, input);
        Ok(input)
    }
}
impl XmlWriter for RecordingAdapter {
    type Model = Vec<Operation>;
    type Output = Vec<ContentToken>;
    fn write_batch(
        &mut self,
        p: &Projection,
        model: &Self::Model,
        order: Option<&[ContentToken]>,
    ) -> ContractResult<Self::Output> {
        assert_eq!(&p.operations, model);
        Ok(order.unwrap_or_default().to_vec())
    }
}
#[test]
fn target_adapter_uses_static_reader_and_writer_batches() {
    let p = plan();
    let mut adapter = RecordingAdapter;
    let model = adapter.read_batch(&p, p.operations.clone()).unwrap();
    let ts = tokens();
    assert_eq!(adapter.write_batch(&p, &model, Some(&ts)).unwrap(), ts);
}

#[test]
fn policy_variants_and_attribute_discriminators_are_never_dropped() {
    for action in ["omit", "nil", "blank", "as_nil", "as_blank"] {
        let mut v = fixture();
        v["models"][ID]["mappings"]["xml"]["rules"][1]["render"] =
            json!({"nil":action,"empty":action,"default":true});
        let discriminator = v["models"][ID]["mappings"]["xml"]["rules"][1]["discriminator"].take();
        v["models"][ID]["attributes"][1]["discriminator"] = discriminator;
        let p = compile_projection(&bundle(v), ID).unwrap();
        let Operation::Field(field) = &p.operations[2] else {
            panic!("missing field")
        };
        let render = field.render.as_ref().unwrap();
        assert_eq!(render.default, RenderAction::Boolean(true));
        assert_eq!(serde_json::to_value(&render.nil).unwrap(), action);
        assert_eq!(serde_json::to_value(&render.empty).unwrap(), action);
        assert_eq!(field.discriminator.as_ref().unwrap().attribute, "kind");
    }
}

#[test]
fn scalar_order_references_cannot_use_collection_indices() {
    let mut ts = tokens();
    ts.insert(
        0,
        ContentToken::Text {
            source: TextSource::Member {
                attribute: "text".into(),
                index: 1,
            },
        },
    );
    let mut counts = counts();
    counts.insert("text".into(), 2);
    assert_eq!(
        validate_order(&plan(), &ts, &counts).unwrap_err().reason,
        ErrorReason::InvalidOrderedContent
    );
}

#[test]
fn invalid_compositors_duplicate_mappings_and_conflicting_discriminators_refuse() {
    let mut unknown = fixture();
    unknown["models"][ID]["mappings"]["xml"]["sequence"]["members"][0] = json!("id");
    let mut wrong = fixture();
    wrong["models"][ID]["mappings"]["xml"]["sequence"]["members"] = json!([]);
    let mut duplicate = fixture();
    let rule = duplicate["models"][ID]["mappings"]["xml"]["rules"][0].clone();
    duplicate["models"][ID]["mappings"]["xml"]["rules"]
        .as_array_mut()
        .unwrap()
        .push(rule);
    let mut conflict = fixture();
    conflict["models"][ID]["attributes"][1]["discriminator"] =
        json!({"attribute":"other","cases":{"node":ID}});
    for v in [unknown, wrong, duplicate, conflict] {
        let e = compile_projection(&bundle(v), ID).unwrap_err();
        assert_eq!(e.reason, ErrorReason::UnsupportedMapping);
        assert!(e.detail.is_some());
    }
}

#[test]
fn canonical_rule_groups_and_mixed_flags_lower_without_losing_field_metadata() {
    let mut v = fixture();
    let xml = &mut v["models"][ID]["mappings"]["xml"];
    let leaf = xml["rules"][1].clone();
    xml.as_object_mut().unwrap().remove("choice");
    xml["ordered"] = json!(true);
    xml["mixed_content"] = json!(true);
    xml["root"]["mixed"] = json!(true);
    xml["sequence"] = json!([{"kind":"sequence","members":[leaf.clone(),{"kind":"choice","min":0,"max":1,"members":[leaf]}]}]);
    let graph = bundle(v);
    let p = compile_projection(&graph, ID).unwrap();
    assert_eq!(p.operations.len(), 6);
    let Operation::Sequence(Compositor::Sequence { members, min, max }) = &p.operations[4] else {
        panic!("missing canonical sequence")
    };
    assert_eq!((*min, *max), (1, Some(1)));
    assert!(
        matches!(&members[1],CompositorMember::Group(group)if matches!(group.as_ref(),Compositor::Choice{min:0,max:Some(1),..}))
    );
    assert_eq!(validate_order(&p, &tokens(), &counts()).unwrap(), tokens());
}
#[test]
fn unsupported_new_xml_rule_metadata_is_retained_then_refused_by_projection() {
    let mut v = fixture();
    v["models"][ID]["mappings"]["xml"]["rules"][1]["polymorphic_map"] =
        json!({"Example::Node":"node"});
    let graph = bundle(v);
    assert_eq!(
        graph.models[ID].mappings.xml.as_ref().unwrap().rules[1]
            .polymorphic_map
            .as_ref()
            .unwrap()["Example::Node"],
        "node"
    );
    assert_eq!(
        compile_projection(&graph, ID).unwrap_err().reason,
        ErrorReason::UnsupportedMapping
    );
}

#[test]
fn invalid_xml_names_are_refused_at_generate_time_with_the_location() {
    type Edit = fn(&mut Value);
    let edits: [(&str, Edit); 4] = [
        ("/root/element", |v| v["root"]["element"] = json!("n:r\"#")),
        ("/rules/0/name/1", |v| {
            v["rules"][0]["name"] = json!(["id", "a*/b"])
        }),
        ("/rules/1/prefix", |v| {
            v["rules"][1]["prefix"] = json!("1bad")
        }),
        ("/namespace/prefix", |v| {
            v["namespace"]["prefix"] = json!("a b")
        }),
    ];
    for (suffix, edit) in edits {
        let mut v = fixture();
        edit(&mut v["models"][ID]["mappings"]["xml"]);
        let e = compile_projection(&bundle(v), ID).unwrap_err();
        assert_eq!(e.reason, ErrorReason::UnsupportedMapping, "{suffix}");
        assert!(e.location.ends_with(suffix), "{suffix}: {}", e.location);
    }
}
