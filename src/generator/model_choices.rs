//! Compile source choice semantics into generated model validation only.
use super::*;
fn contains(group: &ModelChoiceGroup, name: &str) -> bool {
    group.members.iter().any(|member| match member {
        ModelChoiceMember::Attribute { attribute } => attribute == name,
        ModelChoiceMember::Choice { choice } => contains(choice, name),
    })
}
pub(crate) fn choice_attribute(doc: &ModelDocument, name: &str) -> bool {
    doc.choices
        .as_ref()
        .is_some_and(|c| c.groups.iter().any(|g| contains(g, name)))
}
fn optional(group: &ModelChoiceGroup, doc: &ModelDocument) -> bool {
    group.members.iter().any(|member| match member {
        ModelChoiceMember::Choice { choice } => optional(choice, doc),
        ModelChoiceMember::Attribute { attribute } => doc.attributes.iter().any(|a| {
            a.name == *attribute && matches!(a.collection, Collection::Bounded { min: 0, .. })
        }),
    })
}
fn render_empty(group: &ModelChoiceGroup, doc: &ModelDocument, attribute: &str) -> bool {
    group.format.as_deref() == Some("xml")
        && doc
            .mappings
            .xml
            .as_ref()
            .and_then(|xml| {
                xml.rules
                    .iter()
                    .find(|r| r.kind == XmlRuleKind::Element && r.to == attribute)
            })
            .and_then(|r| r.render.as_ref())
            .is_some_and(|r| !matches!(r.empty, RenderAction::Absent | RenderAction::Omit))
}
fn compile(group: &ModelChoiceGroup, doc: &ModelDocument, id: &str) -> ContractResult<String> {
    let mut code = String::from("(||->Result<bool,ValidationError>{let mut count=0u64;");
    let upper = format!(
        "return Err(invalid({},\"\",\"choice_upper_bound\"));",
        quote(id)
    );
    for member in &group.members {
        match member {
            ModelChoiceMember::Choice { choice } => {
                write!(code, "if {}? {{count+=1;}}", compile(choice, doc, id)?).unwrap();
            }
            ModelChoiceMember::Attribute { attribute } => {
                let a = doc
                    .attributes
                    .iter()
                    .find(|a| a.name == *attribute)
                    .unwrap();
                let field = member_identifier(attribute);
                let bounded = matches!(a.collection, Collection::Bounded { .. });
                // Model objects are present even when every field is absent. Only
                // builtin data values and collections use Ruby empty? semantics.
                let empty = if bounded {
                    "v.is_empty()".to_owned()
                } else if matches!(a.value_type, TypeRef::Builtin { .. })
                    && a.discriminator.is_none()
                    && a.reference.is_none()
                {
                    "match serde_json::to_value(v).map_err(|_|invalid(\"\",\"\",\"invalid_document\"))? { serde_json::Value::String(v)=>v.is_empty(),serde_json::Value::Array(v)=>v.is_empty(),serde_json::Value::Object(v)=>v.is_empty(),_=>false }".into()
                } else {
                    "false".into()
                };
                write!(
                    code,
                    "if let Presence::Value(v)=&self.{field} {{let empty={empty};if !empty {{"
                )
                .unwrap();
                if let Collection::Bounded { max: Some(max), .. } = a.collection {
                    if max == 0 {
                        return Err(error(
                            ErrorReason::UnsupportedType,
                            id,
                            attribute,
                            "zero-sized choice collection batch validation is not supported",
                        ));
                    }
                    write!(code, "let batches=(v.len() as u64).div_ceil({max});").unwrap();
                    if group.min > 0 {
                        write!(code, "if batches<{} {{return Ok(false);}}", group.min).unwrap();
                    }
                    if let Some(max) = group.max {
                        write!(code, "if batches>{max} {{{upper}}}").unwrap();
                    }
                }
                code.push_str("count+=1;");
                if render_empty(group, doc, attribute) {
                    code.push_str("}else{count+=1;");
                }
                code.push_str("}}");
            }
        }
    }
    let allow_empty = group.format.as_deref() == Some("xml")
        && doc.mappings.xml.is_some()
        && group.members.iter().all(|m| match m {
            ModelChoiceMember::Choice { .. } => true,
            ModelChoiceMember::Attribute { attribute } => render_empty(group, doc, attribute),
        });
    if allow_empty || optional(group, doc) {
        code.push_str("if count==0{return Ok(true);}");
    }
    if group.min > 0 {
        write!(code, "if count<{} {{return Ok(false);}}", group.min).unwrap();
    }
    if let Some(max) = group.max {
        write!(code, "if count>{max} {{{upper}}}").unwrap();
    }
    code.push_str("Ok(true)})()");
    Ok(code)
}
pub(super) fn compile_model_choices(doc: &ModelDocument, id: &str) -> ContractResult<String> {
    let mut code = String::new();
    if let Some(choices) = &doc.choices {
        for group in &choices.groups {
            write!(
                code,
                "if !{}? {{return Err(invalid({},\"\",\"choice_lower_bound\"));}}",
                compile(group, doc, id)?,
                quote(id)
            )
            .unwrap();
        }
    }
    Ok(code)
}
