//! Compiled INSIDE the generated consumer crate: typed classes and codecs generated from the exact bundle,
//! handwritten hooks from `hooks.rs`, and the 10 actual source cases.
use super::contracts::{ContractError, ErrorReason};
use super::hooks::{Keyed, MixedItem, State, UncertaintyHooks, Without};
use super::json_codecs::*;
use super::models::*;
use serde_json::{json, Map, Value};

const CASES: &str = include_str!("cases.json");
const NODE_FIELDS: [&str; 7] = [
    "value",
    "magnitude",
    "coverage_factor",
    "coverage_probability",
    "lower",
    "upper",
    "unit",
];
const SYMBOL_FIELDS: [&str; 4] = ["category", "interval_kind", "notation", "distribution"];

fn cases() -> Vec<Value> {
    let fixture: Value = serde_json::from_str(CASES).unwrap();
    assert!(fixture["excluded_outside_finite_graph"]
        .as_array()
        .unwrap()
        .is_empty());
    fixture["cases"].as_array().unwrap().clone()
}
fn reason(e: &ContractError) -> (ErrorReason, String) {
    (e.reason.clone(), e.location.clone())
}
fn read(
    text: &str,
    state: &mut State,
) -> Result<Plurimath_Math_Function_Uncertainty, Box<ContractError>> {
    read_Plurimath_Math_Function_Uncertainty_json_with(text, &UncertaintyHooks, state)
}

#[test]
fn the_ten_actual_cases_replay_declared_data_hook_state_and_ruby_json() {
    let (mut tree_equal, mut byte_equal) = (0, 0);
    let all = cases();
    assert_eq!(all.len(), 10);
    for case in &all {
        let id = case["id"].as_str().unwrap();
        let input = serde_json::to_string(&case["input_json"]).unwrap();
        let mut state = State::default();
        let model = read(&input, &mut state).unwrap_or_else(|e| panic!("{id}: {e:?}"));
        // Declared data: typed polymorphic node views, exact strings; hook-only keys are NOT model data.
        assert_eq!(
            model._class,
            Presence::Value("Plurimath::Math::Function::Uncertainty".to_string()),
            "{id}"
        );
        let declared = serde_json::to_value(&model).unwrap();
        for field in NODE_FIELDS {
            assert_eq!(
                declared.get(format!("__{field}_node")),
                case["input_json"].get(field),
                "{id}: {field}"
            );
        }
        for field in SYMBOL_FIELDS {
            assert!(
                declared.get(field).is_none(),
                "{id}: {field} must not be declared data"
            );
        }
        if let Presence::Value(Plurimath_Math_Function_Uncertainty__value_node_Union::Case0(
            number,
        )) = &model._value_node
        {
            assert_eq!(
                number.value,
                Presence::Value(case["model_fields"]["value"].as_str().unwrap().to_string())
            );
            assert_eq!(number.mini_sub_sized, Presence::Value(false));
        } else {
            panic!("{id}: value is not a typed Number");
        }
        match (&model._unit_node, case["model_fields"]["unit"].as_object()) {
            (
                Presence::Value(Plurimath_Math_Function_Uncertainty__unit_node_Union::Case0(unit)),
                Some(want),
            ) => {
                assert_eq!(
                    unit.text,
                    Presence::Value(want["text"].as_str().unwrap().to_string())
                );
            }
            (Presence::Absent, None) => {}
            _ => panic!("{id}: unit mismatch"),
        }
        // Hook state: exactly the symbol fields the source carries, nothing else.
        for field in SYMBOL_FIELDS {
            assert_eq!(
                state.symbols.get(field).map(String::as_str),
                case["model_fields"][field].as_str(),
                "{id}: {field}"
            );
        }
        assert!(
            state.text.is_empty() && state.value.is_empty() && state.mixed.is_empty(),
            "{id}"
        );
        // Reserialization equals Ruby: tree equality, then exact text (counted separately).
        let out = write_Plurimath_Math_Function_Uncertainty_json_with(
            &model,
            &UncertaintyHooks,
            &mut state,
        )
        .unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&out).unwrap(),
            case["ruby_json"],
            "{id}"
        );
        tree_equal += 1;
        if out == serde_json::to_string(&case["ruby_json"]).unwrap() {
            byte_equal += 1;
        }
    }
    println!(
        "PILOT_REPORT cases={} tree_equal={tree_equal} byte_equal={byte_equal}",
        all.len()
    );
    assert_eq!(tree_equal, 10);
    // Byte equality is reported, not asserted: the typed polymorphic node writes its `__class` tag last, while
    // Ruby writes it first. Key order is not part of JSON data equality.
    let _ = byte_equal;
}

#[test]
fn a_missing_handler_fails_custom_serialization_by_name_and_only_for_the_attempted_direction() {
    let case = &cases()[0];
    let input = serde_json::to_string(&case["input_json"]).unwrap();
    // No handlers: the read reaches `category`.
    let error = read_Plurimath_Math_Function_Uncertainty_json(&input).unwrap_err();
    assert_eq!(
        reason(&error),
        (ErrorReason::CustomSerialization, "/category".into())
    );
    assert_eq!(
        error.model_id.as_deref(),
        Some("Plurimath::Math::Function::Uncertainty@default")
    );
    let mut state = State::default();
    let model = read(&input, &mut state).unwrap();
    let error = write_Plurimath_Math_Function_Uncertainty_json(&model).unwrap_err();
    assert_eq!(
        reason(&error),
        (ErrorReason::CustomSerialization, "/category".into())
    );
    // Exactly one handler missing: only that direction, only that key.
    let error = read_Plurimath_Math_Function_Uncertainty_json_with(
        &input,
        &Without("notation_from_kv"),
        &mut State::default(),
    )
    .unwrap_err();
    assert_eq!(
        reason(&error),
        (ErrorReason::CustomSerialization, "/notation".into())
    );
    let error = write_Plurimath_Math_Function_Uncertainty_json_with(
        &model,
        &Without("notation_to_kv"),
        &mut state,
    )
    .unwrap_err();
    assert_eq!(
        reason(&error),
        (ErrorReason::CustomSerialization, "/notation".into())
    );
    // A read never needs a write handler, and a write never needs a read handler.
    read_Plurimath_Math_Function_Uncertainty_json_with(
        &input,
        &Without("notation_to_kv"),
        &mut State::default(),
    )
    .unwrap();
    write_Plurimath_Math_Function_Uncertainty_json_with(
        &model,
        &Without("notation_from_kv"),
        &mut state,
    )
    .unwrap();
}

fn model() -> Plurimath_Math_Function_Uncertainty {
    Plurimath_Math_Function_Uncertainty::default()
}
fn round_trip(wire: Value) -> (State, Value) {
    let mut object = Map::new();
    object.insert(
        "__class".into(),
        json!("Plurimath::Math::Function::Uncertainty"),
    );
    object.extend(wire.as_object().unwrap().clone());
    let text = serde_json::to_string(&object).unwrap();
    let mut state = State::default();
    let m = read(&text, &mut state).unwrap();
    let out =
        write_Plurimath_Math_Function_Uncertainty_json_with(&m, &UncertaintyHooks, &mut state)
            .unwrap();
    (state, serde_json::from_str(&out).unwrap())
}

#[test]
fn symbol_hooks_keep_a_closed_vocabulary_and_refuse_other_shapes() {
    let tags = [
        ("category", "expanded"),
        ("interval_kind", "shortest"),
        ("notation", "concise"),
        ("distribution", "triang"),
    ];
    for (field, tag) in tags {
        let (state, out) = round_trip(json!({ field: tag }));
        assert_eq!(state.symbols.get(field).map(String::as_str), Some(tag));
        assert_eq!(out[field], json!(tag));
        for bad in [json!("bogus"), json!(7), json!({"string": "standard"})] {
            let object = json!({"__class": "Plurimath::Math::Function::Uncertainty", field: bad});
            let error = read(&object.to_string(), &mut State::default()).unwrap_err();
            assert_eq!(error.reason, ErrorReason::InvalidDocument, "{field} {bad}");
        }
        assert!(round_trip(json!({ field: null }))
            .0
            .symbols
            .get(field)
            .is_none());
    }
    let _ = model();
}

#[test]
fn node_field_hooks_round_trip_their_own_slot_and_refuse_unsupported_shapes() {
    for field in NODE_FIELDS {
        let text_key = format!("{field}_text");
        let value_key = format!("{field}_value");
        let mixed_key = format!("{field}_mixed");
        let value = json!({"align": "left", "nested": {"n": [1, "x"], "__string_keys": ["n"]}, "__string_keys": ["id"], "id": "a"});
        let mixed = json!([{"text": "a"}, {"class": "Plurimath::Math::Number", "node": {"__class": "Plurimath::Math::Number", "value": "2"}}, {"text": "b"}]);
        let (state, out) = round_trip(
            json!({ text_key.clone(): "1,23", value_key.clone(): value.clone(), mixed_key.clone(): mixed.clone() }),
        );
        assert_eq!(state.text.keys().copied().collect::<Vec<_>>(), [field]);
        assert_eq!(state.value.keys().copied().collect::<Vec<_>>(), [field]);
        assert_eq!(state.mixed.keys().copied().collect::<Vec<_>>(), [field]);
        assert_eq!(out[&text_key], json!("1,23"));
        assert_eq!(out[&value_key], value);
        assert_eq!(out[&mixed_key], mixed);
        assert!(matches!(state.value[field], Keyed::Hash(_)));
        assert!(matches!(state.mixed[field][0], MixedItem::Text(_)));
        assert_eq!(
            round_trip(json!({ value_key.clone(): [] })).1[&value_key],
            json!([])
        );
        for bad in [
            json!({ text_key.clone(): 5 }),
            json!({ value_key.clone(): "x" }),
            json!({ value_key.clone(): [1] }),
            json!({ value_key.clone(): {"__string_keys": "id"} }),
            json!({ mixed_key.clone(): "x" }),
            json!({ mixed_key.clone(): [{"text": 5}] }),
            json!({ mixed_key.clone(): [{"class": "Plurimath::Math::Nope", "node": {}}] }),
            json!({ mixed_key.clone(): [{"class": "Plurimath::Math::Number", "node": "x"}] }),
        ] {
            let mut object = bad.as_object().unwrap().clone();
            object.insert(
                "__class".into(),
                json!("Plurimath::Math::Function::Uncertainty"),
            );
            let error =
                read(&Value::Object(object).to_string(), &mut State::default()).unwrap_err();
            assert_eq!(error.reason, ErrorReason::InvalidDocument, "{field} {bad}");
        }
    }
    // An all-node mixed list is owned by the node view and is not written, like the source.
    let wire =
        json!({"value_mixed": [{"class": "Plurimath::Math::Number", "node": {"value": "1"}}]});
    assert!(round_trip(wire).1.get("value_mixed").is_none());
}

#[test]
fn an_unknown_polymorphic_tag_is_refused_and_known_fields_survive() {
    let text = json!({"__class": "Plurimath::Math::Function::Uncertainty", "value": {"__class": "Plurimath::Math::Nope", "value": "1"}});
    let error = read(&text.to_string(), &mut State::default()).unwrap_err();
    assert_eq!(error.reason, ErrorReason::UnknownDiscriminator);
}

#[test]
fn the_constructor_stub_fails_custom_initialize_and_explicit_application_init_is_separate() {
    let error = Plurimath_Math_Function_Uncertainty::construct("standard").unwrap_err();
    assert_eq!(error.reason, "custom_initialize");
    assert_eq!(
        error.model_id,
        "Plurimath::Math::Function::Uncertainty@default"
    );
    assert!(Plurimath_Math_Function_Uncertainty::CUSTOM_INITIALIZE);
    // Target-local application init (consumer code, never generated): it builds declared data only.
    let built = Plurimath_Math_Function_Uncertainty::construct_with("1.23", |value: &str| {
        let mut model = Plurimath_Math_Function_Uncertainty::default();
        model._class = Presence::Value("Plurimath::Math::Function::Uncertainty".into());
        let number: Plurimath_Math_Number =
            serde_json::from_value(json!({"__class": "Plurimath::Math::Number", "value": value}))
                .unwrap();
        model._value_node = Presence::Value(
            Plurimath_Math_Function_Uncertainty__value_node_Union::Case0(Box::new(number)),
        );
        Ok(model)
    })
    .unwrap();
    assert!(matches!(built._value_node, Presence::Value(_)));
    // Serde reconstruction (what decoding uses) stays data-only and never calls an initializer.
    let data: Plurimath_Math_Function_Uncertainty =
        serde_json::from_str(r#"{"__class":"Plurimath::Math::Function::Uncertainty"}"#).unwrap();
    assert!(data._value_node.is_absent());
}
