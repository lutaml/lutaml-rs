//! HANDWRITTEN target-local counterparts of the 25 Plurimath `Uncertainty` key-value hooks (source: plurimath
//! 050a3a9c `Core.node_fields` / `declare_node_field` / `declare_plain_field`). Not compiler output; no source
//! text is evaluated. Each function re-implements one source contract.
//!
//! Storage contract: the generated model keeps declared data only (`__class`, the node views). What the source
//! keeps in instance variables behind these hooks lives in [`State`], the explicit application state handed to
//! every hook. A hook never touches another field's slot.
use super::contracts::{ContractError, ContractResult, ErrorReason};
use super::json_codecs::Plurimath_Math_Function_UncertaintyHooks;
use super::models::Plurimath_Math_Function_Uncertainty as Unc;
use serde_json::{Map, Value};
use std::collections::BTreeMap;

const UNC: &str = "Plurimath::Math::Function::Uncertainty@default";
/// Source `STRING_KEYS`: the list that tells String keys from Symbol keys in a hash.
const STRING_KEYS: &str = "__string_keys";
/// Node classes the finite pilot graph can name in a mixed list (source NODE_CLASS_MAP, restricted to the graph).
const MIXED_CLASSES: [&str; 4] = [
    "Plurimath::Math::Core",
    "Plurimath::Math::Number",
    "Plurimath::Math::Function::Unitsml",
    "Plurimath::Math::Function::Uncertainty",
];

/// A hash as the source holds it: each key remembers whether it was a String (else a Symbol).
#[derive(Clone, Debug, PartialEq)]
pub enum Keyed {
    Scalar(Value),
    List(Vec<Keyed>),
    Hash(Vec<(String, bool, Keyed)>),
}
#[derive(Clone, Debug, PartialEq)]
pub enum MixedItem {
    Text(String),
    Node {
        class: String,
        node: Map<String, Value>,
    },
}
#[derive(Default, Debug)]
pub struct State {
    pub symbols: BTreeMap<&'static str, String>,
    pub text: BTreeMap<&'static str, String>,
    pub value: BTreeMap<&'static str, Keyed>,
    pub mixed: BTreeMap<&'static str, Vec<MixedItem>>,
}
/// The handwritten implementation of every hook of the Uncertainty model.
pub struct UncertaintyHooks;

/// A hook refused a shape the source contract does not define.
fn refuse(hook: &str, detail: impl ToString) -> Box<ContractError> {
    Box::new(ContractError {
        reason: ErrorReason::InvalidDocument,
        model_id: Some(UNC.into()),
        attribute: None,
        format: Some("json".into()),
        rule: Some(hook.into()),
        location: String::new(),
        klass: None,
        detail: Some(detail.to_string()),
    })
}

/// Source `Core.load_keys`.
fn load_keys(hook: &str, value: &Value) -> ContractResult<Keyed> {
    match value {
        Value::Array(items) => Ok(Keyed::List(
            items
                .iter()
                .map(|i| load_keys(hook, i))
                .collect::<Result<_, _>>()?,
        )),
        Value::Object(map) => {
            let strings: Vec<&str> = match map.get(STRING_KEYS) {
                None => Vec::new(),
                Some(Value::Array(list)) => list
                    .iter()
                    .map(|k| {
                        k.as_str()
                            .ok_or_else(|| refuse(hook, "__string_keys must be a list of strings"))
                    })
                    .collect::<Result<_, _>>()?,
                Some(_) => return Err(refuse(hook, "__string_keys must be a list of strings")),
            };
            let mut entries = Vec::new();
            for (key, item) in map {
                if key == STRING_KEYS {
                    continue;
                }
                entries.push((
                    key.clone(),
                    strings.contains(&key.as_str()),
                    load_keys(hook, item)?,
                ));
            }
            Ok(Keyed::Hash(entries))
        }
        other => Ok(Keyed::Scalar(other.clone())),
    }
}
/// Source `Core.dump_keys`.
fn dump_keys(keyed: &Keyed) -> Value {
    match keyed {
        Keyed::Scalar(v) => v.clone(),
        Keyed::List(items) => Value::Array(items.iter().map(dump_keys).collect()),
        Keyed::Hash(entries) => {
            let mut map = Map::new();
            for (key, _, item) in entries {
                map.insert(key.clone(), dump_keys(item));
            }
            let strings: Vec<Value> = entries
                .iter()
                .filter(|e| e.1)
                .map(|e| Value::String(e.0.clone()))
                .collect();
            if !strings.is_empty() {
                map.insert(STRING_KEYS.into(), Value::Array(strings));
            }
            Value::Object(map)
        }
    }
}

/// Symbol fields: a stored Symbol is written as its name. The four are closed vocabularies, so only the
/// symbol form is defined (the `{"string": s}` encoding of free-form values is refused).
macro_rules! symbol {
    ($from:ident, $to:ident, $field:literal, $tags:expr) => {
        fn $from(&self, _model: &mut Unc, value: &Value, state: &mut State) -> ContractResult<()> {
            match value {
                Value::Null => {
                    state.symbols.remove($field);
                    Ok(())
                }
                Value::String(tag) if $tags.contains(&tag.as_str()) => {
                    state.symbols.insert($field, tag.clone());
                    Ok(())
                }
                Value::String(tag) => Err(refuse(
                    stringify!($from),
                    format!("unknown {} tag {tag:?}", $field),
                )),
                other => Err(refuse(
                    stringify!($from),
                    format!("expected a symbol name, got {other}"),
                )),
            }
        }
        fn $to(
            &self,
            _model: &Unc,
            document: &mut Map<String, Value>,
            state: &mut State,
        ) -> ContractResult<()> {
            if let Some(tag) = state.symbols.get($field) {
                document.insert($field.into(), Value::String(tag.clone()));
            }
            Ok(())
        }
    };
}

/// Node fields: `<field>_text`, `<field>_value`, `<field>_mixed`.
macro_rules! node {
    ($field:literal, $text_from:ident, $text_to:ident, $value_from:ident, $value_to:ident, $mixed_from:ident, $mixed_to:ident) => {
        fn $text_from(&self, _model: &mut Unc, value: &Value, state: &mut State) -> ContractResult<()> {
            let Value::String(text) = value else {
                return Err(refuse(stringify!($text_from), format!("expected a string, got {value}")));
            };
            state.text.insert($field, text.clone());
            Ok(())
        }
        fn $text_to(&self, _model: &Unc, document: &mut Map<String, Value>, state: &mut State) -> ContractResult<()> {
            if let Some(text) = state.text.get($field) {
                document.insert(concat!($field, "_text").into(), Value::String(text.clone()));
            }
            Ok(())
        }
        // Source: an attribute Hash or an empty array.
        fn $value_from(&self, _model: &mut Unc, value: &Value, state: &mut State) -> ContractResult<()> {
            let empty_list = matches!(value, Value::Array(a) if a.is_empty());
            if !value.is_object() && !empty_list {
                return Err(refuse(stringify!($value_from), format!("expected an object or an empty list, got {value}")));
            }
            state.value.insert($field, load_keys(stringify!($value_from), value)?);
            Ok(())
        }
        fn $value_to(&self, _model: &Unc, document: &mut Map<String, Value>, state: &mut State) -> ContractResult<()> {
            if let Some(held) = state.value.get($field) {
                document.insert(concat!($field, "_value").into(), dump_keys(held));
            }
            Ok(())
        }
        // Source: a list mixing Strings and nodes: {"text": s} | {"class": name, "node": hash}.
        fn $mixed_from(&self, _model: &mut Unc, value: &Value, state: &mut State) -> ContractResult<()> {
            let hook = stringify!($mixed_from);
            let Value::Array(items) = value else {
                return Err(refuse(hook, format!("expected a list, got {value}")));
            };
            let mut loaded = Vec::new();
            for item in items {
                let Value::Object(object) = item else {
                    return Err(refuse(hook, "list items must be objects"));
                };
                if !object.contains_key("node") {
                    let Some(Value::String(text)) = object.get("text") else {
                        return Err(refuse(hook, "a text item needs a string `text`"));
                    };
                    loaded.push(MixedItem::Text(text.clone()));
                } else {
                    let class = match object.get("class") {
                        Some(Value::String(c)) if MIXED_CLASSES.contains(&c.as_str()) => c.clone(),
                        other => return Err(refuse(hook, format!("unknown node class {other:?}"))),
                    };
                    let Some(Value::Object(node)) = object.get("node") else {
                        return Err(refuse(hook, "`node` must be an object"));
                    };
                    loaded.push(MixedItem::Node { class, node: node.clone() });
                }
            }
            state.mixed.insert($field, loaded);
            Ok(())
        }
        fn $mixed_to(&self, _model: &Unc, document: &mut Map<String, Value>, state: &mut State) -> ContractResult<()> {
            if let Some(held) = state.mixed.get($field) {
                if held.iter().any(|i| matches!(i, MixedItem::Text(_))) {
                    let items = held
                        .iter()
                        .map(|item| match item {
                            MixedItem::Text(t) => serde_json::json!({ "text": t }),
                            MixedItem::Node { class, node } => serde_json::json!({ "class": class, "node": node }),
                        })
                        .collect();
                    document.insert(concat!($field, "_mixed").into(), Value::Array(items));
                }
            }
            Ok(())
        }
    };
}

impl Plurimath_Math_Function_UncertaintyHooks<State> for UncertaintyHooks {
    symbol!(
        category_from_kv,
        category_to_kv,
        "category",
        ["standard", "expanded", "coverage_interval"]
    );
    symbol!(
        interval_kind_from_kv,
        interval_kind_to_kv,
        "interval_kind",
        ["shortest", "probabilistically_symmetric"]
    );
    symbol!(
        notation_from_kv,
        notation_to_kv,
        "notation",
        ["absolute", "concise"]
    );
    symbol!(
        distribution_from_kv,
        distribution_to_kv,
        "distribution",
        ["rect", "triang", "arcsine"]
    );
    node!(
        "value",
        value_text_from_kv,
        value_text_to_kv,
        value_value_from_kv,
        value_value_to_kv,
        value_mixed_from_kv,
        value_mixed_to_kv
    );
    node!(
        "magnitude",
        magnitude_text_from_kv,
        magnitude_text_to_kv,
        magnitude_value_from_kv,
        magnitude_value_to_kv,
        magnitude_mixed_from_kv,
        magnitude_mixed_to_kv
    );
    node!(
        "coverage_factor",
        coverage_factor_text_from_kv,
        coverage_factor_text_to_kv,
        coverage_factor_value_from_kv,
        coverage_factor_value_to_kv,
        coverage_factor_mixed_from_kv,
        coverage_factor_mixed_to_kv
    );
    node!(
        "coverage_probability",
        coverage_probability_text_from_kv,
        coverage_probability_text_to_kv,
        coverage_probability_value_from_kv,
        coverage_probability_value_to_kv,
        coverage_probability_mixed_from_kv,
        coverage_probability_mixed_to_kv
    );
    node!(
        "lower",
        lower_text_from_kv,
        lower_text_to_kv,
        lower_value_from_kv,
        lower_value_to_kv,
        lower_mixed_from_kv,
        lower_mixed_to_kv
    );
    node!(
        "upper",
        upper_text_from_kv,
        upper_text_to_kv,
        upper_value_from_kv,
        upper_value_to_kv,
        upper_mixed_from_kv,
        upper_mixed_to_kv
    );
    node!(
        "unit",
        unit_text_from_kv,
        unit_text_to_kv,
        unit_value_from_kv,
        unit_value_to_kv,
        unit_mixed_from_kv,
        unit_mixed_to_kv
    );
}

/// Test helper: the handwritten hooks with exactly one named hook left as the generated failing stub.
pub struct Without(pub &'static str);
macro_rules! delegate_read {
    ($($name:ident),*) => { $(
        fn $name(&self, model: &mut Unc, value: &Value, state: &mut State) -> ContractResult<()> {
            if self.0 == stringify!($name) {
                super::json_codecs::Plurimath_Math_Function_UncertaintyNoHooks.$name(model, value, state)
            } else {
                UncertaintyHooks.$name(model, value, state)
            }
        }
    )* };
}
macro_rules! delegate_write {
    ($($name:ident),*) => { $(
        fn $name(&self, model: &Unc, document: &mut Map<String, Value>, state: &mut State) -> ContractResult<()> {
            if self.0 == stringify!($name) {
                super::json_codecs::Plurimath_Math_Function_UncertaintyNoHooks.$name(model, document, state)
            } else {
                UncertaintyHooks.$name(model, document, state)
            }
        }
    )* };
}
impl Plurimath_Math_Function_UncertaintyHooks<State> for Without {
    delegate_read!(
        category_from_kv,
        interval_kind_from_kv,
        notation_from_kv,
        distribution_from_kv,
        value_text_from_kv,
        value_value_from_kv,
        value_mixed_from_kv,
        magnitude_text_from_kv,
        magnitude_value_from_kv,
        magnitude_mixed_from_kv,
        coverage_factor_text_from_kv,
        coverage_factor_value_from_kv,
        coverage_factor_mixed_from_kv,
        coverage_probability_text_from_kv,
        coverage_probability_value_from_kv,
        coverage_probability_mixed_from_kv,
        lower_text_from_kv,
        lower_value_from_kv,
        lower_mixed_from_kv,
        upper_text_from_kv,
        upper_value_from_kv,
        upper_mixed_from_kv,
        unit_text_from_kv,
        unit_value_from_kv,
        unit_mixed_from_kv
    );
    delegate_write!(
        category_to_kv,
        interval_kind_to_kv,
        notation_to_kv,
        distribution_to_kv,
        value_text_to_kv,
        value_value_to_kv,
        value_mixed_to_kv,
        magnitude_text_to_kv,
        magnitude_value_to_kv,
        magnitude_mixed_to_kv,
        coverage_factor_text_to_kv,
        coverage_factor_value_to_kv,
        coverage_factor_mixed_to_kv,
        coverage_probability_text_to_kv,
        coverage_probability_value_to_kv,
        coverage_probability_mixed_to_kv,
        lower_text_to_kv,
        lower_value_to_kv,
        lower_mixed_to_kv,
        upper_text_to_kv,
        upper_value_to_kv,
        upper_mixed_to_kv,
        unit_text_to_kv,
        unit_value_to_kv,
        unit_mixed_to_kv
    );
}
