//! Closed, unbound string enumerations reuse the existing static enum emitter.
//! Other named semantics retain their explicit trusted-binding requirement.
use super::*;

pub(crate) fn realize_named_enums(
    bundle: &mut Bundle,
    bindings: &TypeBindings,
) -> ContractResult<()> {
    let definitions = &bundle.value_types;
    for (model_id, model) in &mut bundle.models {
        for attribute in &mut model.attributes {
            let TypeRef::Value { id } = &attribute.value_type else {
                continue;
            };
            if bindings.contains_key(id)
                || attribute.enumeration.is_some()
                || attribute.reference.is_some()
                || attribute.discriminator.is_some()
            {
                continue;
            }
            let Some(spec) = definitions.get(id) else {
                continue;
            };
            if spec.base != BuiltinType::String || spec.facets.len() != 1 {
                continue;
            }
            let Some(NeutralValue::Array(values)) = spec.facets.get("enumeration") else {
                continue;
            };
            if values.is_empty()
                || values
                    .iter()
                    .any(|value| !matches!(value, NeutralValue::String(_)))
            {
                return Err(error(
                    ErrorReason::InvalidDocument,
                    model_id,
                    &attribute.name,
                    "named string enumeration needs a nonempty string array",
                ));
            }
            attribute.value_type = TypeRef::Builtin {
                name: BuiltinType::String,
            };
            attribute.enumeration = Some(values.clone());
        }
    }
    Ok(())
}
