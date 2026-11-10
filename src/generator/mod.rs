//! Static Rust source emission. Schemas are data; only caller-supplied bindings
//! may name external Rust types. No grammar is interpreted at runtime.
pub(crate) mod model_choices;
use crate::contracts::*;
use model_choices::{choice_attribute, compile_model_choices};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;

#[derive(Default)]
pub struct RustEmitter;
fn error(reason: ErrorReason, id: &str, field: &str, detail: &str) -> Box<ContractError> {
    Box::new(ContractError {
        reason,
        model_id: Some(id.into()),
        attribute: (!field.is_empty()).then(|| field.into()),
        format: None,
        rule: None,
        location: format!(
            "/models/{}/attributes/{}",
            id.replace('~', "~0").replace('/', "~1"),
            field.replace('~', "~0").replace('/', "~1")
        ),
        klass: None,
        detail: Some(detail.into()),
    })
}
fn quote(s: &str) -> String {
    format!("{s:?}")
}
/// Deterministic Rust member identifier. Emitters must still reject collisions
/// among members of the same model after applying this normalization.
pub fn member_identifier(s: &str) -> String {
    let mut result = String::new();
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            result.push(c);
        } else if !result.ends_with('_') {
            result.push('_');
        }
    }
    if result == "_" {
        result.push_str("Lml");
    }
    if result.is_empty() || result.starts_with(|c: char| c.is_ascii_digit()) {
        result.insert(0, '_');
    }
    if [
        "Self", "self", "super", "crate", "type", "mod", "fn", "pub", "struct", "enum", "match",
        "ref", "move", "use", "impl", "trait", "where", "loop", "in", "as", "const", "static",
        "let", "mut", "async", "await", "dyn", "default", "return", "break", "continue", "if",
        "else", "while", "for", "true", "false", "extern", "unsafe", "union", "abstract", "become",
        "box", "do", "final", "macro", "override", "priv", "typeof", "unsized", "virtual", "yield",
        "try",
    ]
    .contains(&result.as_str())
    {
        result.push('_');
    }
    result
}
/// Every name the generated crate imports, defines at module level, or uses
/// unqualified in a position a same-named model type would shadow. A model whose
/// Rust name is one of these is refused with `ReservedMember` instead of
/// producing a crate that does not compile. `tests/generator.rs` derives the
/// candidate set from the emitted sources, so a new runtime item cannot be added
/// without deciding here whether it is reserved.
pub const RESERVED_NAMES: &[&str] = &[
    // Defined by generator/runtime.rs.txt.
    "Presence",
    "ValidationError",
    "ContentToken",
    "TextSource",
    "ContentTokenWire",
    "QName",
    "ExactInteger",
    "ExactDecimal",
    "PortableFloat",
    "Neutral",
    "NeutralHash",
    "NeutralNumber",
    "NeutralWire",
    // Imported by the generated files.
    "Serialize",
    "Deserialize",
    "ContractError",
    "ContractResult",
    "ErrorReason",
    "Value",
    "Map",
    // Prelude and std names used unqualified by emitted code.
    "String",
    "Vec",
    "Box",
    "Result",
    "Option",
    "Default",
    "Eq",
    "PartialEq",
    "Ord",
    "PartialOrd",
    "Clone",
    "Copy",
    "Debug",
    "ToString",
    "Sized",
    "Send",
    "Sync",
    // Crate and module roots used in emitted paths.
    "serde",
    "serde_json",
    "std",
    "core",
    "alloc",
    "lutaml_rs",
    // Primitive types.
    "bool",
    "str",
    "char",
    "i8",
    "i16",
    "i32",
    "i64",
    "i128",
    "isize",
    "u8",
    "u16",
    "u32",
    "u64",
    "u128",
    "usize",
    "f32",
    "f64",
];
/// Stable model-ID to Rust type-name map. A model name shared by several
/// registers (`Foo@a`, `Foo@b`) is disambiguated deterministically: every model
/// in that group takes the suffix `_<register>`, independent of map order. Names
/// that are reserved, or that still collide, are refused.
pub fn model_names(bundle: &Bundle) -> ContractResult<BTreeMap<String, String>> {
    let mut groups: BTreeMap<String, usize> = BTreeMap::new();
    for m in bundle.models.values() {
        *groups.entry(member_identifier(&m.model)).or_default() += 1;
    }
    let mut used: BTreeSet<String> = BTreeSet::new();
    bundle
        .models
        .iter()
        .map(|(id, m)| {
            let base = member_identifier(&m.model);
            let name = if groups[&base] > 1 {
                format!("{base}_{}", member_identifier(&m.register))
            } else {
                base
            };
            if RESERVED_NAMES.contains(&name.as_str()) {
                return Err(error(
                    ErrorReason::ReservedMember,
                    id,
                    "",
                    &format!("Rust type name {name} is reserved by the generated runtime"),
                ));
            }
            if !used.insert(name.clone()) {
                return Err(error(
                    ErrorReason::ReservedMember,
                    id,
                    "",
                    &format!("Rust type name collision: {name}"),
                ));
            }
            Ok((id.clone(), name))
        })
        .collect()
}
/// Collect the discriminators declared by rules that target `member`, in every
/// format, recursing through XML groups and sequences.
fn rule_discriminators(
    rules: &[XmlRule],
    member: &str,
    from: &'static str,
    out: &mut Vec<(&'static str, Discriminator)>,
) {
    for rule in rules {
        if rule.to == member {
            if let Some(d) = &rule.discriminator {
                out.push((from, d.clone()));
            }
        }
        if let Some(members) = &rule.members {
            rule_discriminators(members, member, from, out);
        }
    }
}
/// The bundle with each polymorphic attribute's governing discriminator hoisted
/// to the attribute. Ruby exports key-value polymorphism on the mapping rule
/// (`mappings.json.rules[].discriminator`) and the case models declare the tag
/// attribute; the attribute-level and every rule-level source must agree, as in
/// the TypeScript generator's `effectiveDiscriminator`. Only plain model-typed
/// attributes are hoisted, so a rule-level discriminator elsewhere still
/// reaches the explicit refusal in `generate_models`.
pub fn effective_discriminators(bundle: &Bundle) -> ContractResult<Bundle> {
    let mut out = bundle.clone();
    for (id, m) in &mut out.models {
        let mut sources: Vec<(String, &'static str, Discriminator)> = Vec::new();
        for attribute in &m.attributes {
            let mut found = Vec::new();
            if let Some(xml) = &m.mappings.xml {
                rule_discriminators(&xml.rules, &attribute.name, "xml rule", &mut found);
                if let Some(sequence) = &xml.sequence {
                    rule_discriminators(sequence, &attribute.name, "xml rule", &mut found);
                }
            }
            for (mapping, from) in [
                (&m.mappings.json, "json rule"),
                (&m.mappings.yaml, "yaml rule"),
            ] {
                if let Some(mapping) = mapping {
                    rule_discriminators(&mapping.rules, &attribute.name, from, &mut found);
                }
            }
            sources.extend(
                found
                    .into_iter()
                    .map(|(f, d)| (attribute.name.clone(), f, d)),
            );
        }
        for attribute in &mut m.attributes {
            let mut all: Vec<(&'static str, Discriminator)> = Vec::new();
            if let Some(d) = &attribute.discriminator {
                all.push(("attribute", d.clone()));
            }
            all.extend(
                sources
                    .iter()
                    .filter(|(to, _, _)| *to == attribute.name)
                    .map(|(_, from, d)| (*from, d.clone())),
            );
            let Some((first_from, first)) = all.first().cloned() else {
                continue;
            };
            if let Some((from, _)) = all.iter().find(|(_, d)| *d != first) {
                return Err(error(
                    ErrorReason::UnsupportedMapping,
                    id,
                    &attribute.name,
                    &format!("discriminator of the {first_from} disagrees with the {from}"),
                ));
            }
            if attribute.discriminator.is_none()
                && attribute.reference.is_none()
                && attribute.enumeration.is_none()
                && matches!(attribute.value_type, TypeRef::Model { .. })
            {
                attribute.discriminator = Some(first);
            }
        }
    }
    Ok(out)
}
fn builtin(t: &BuiltinType, id: &str, field: &str) -> ContractResult<String> {
    Ok(match t {
        BuiltinType::String | BuiltinType::Token | BuiltinType::Symbol | BuiltinType::Reference => {
            "String"
        }
        BuiltinType::Boolean => "bool",
        t if t.is_integer() => "ExactInteger",
        BuiltinType::Decimal | BuiltinType::Rational => "ExactDecimal",
        BuiltinType::Float => "PortableFloat",
        BuiltinType::Qname => "QName",
        // An ordered map of neutral JSON values (also the XML catch-all attribute target).
        BuiltinType::Hash => "NeutralHash",
        // Lexical formats require a declared validator/binding rather than an
        // unchecked String or a platform-dependent date/URI interpretation.
        _ => {
            return Err(error(
                ErrorReason::UnsupportedType,
                id,
                field,
                "builtin requires an explicit lexical codec",
            ))
        }
    }
    .into())
}
fn field_type(
    a: &Attribute,
    id: &str,
    names: &BTreeMap<String, String>,
    bundle: &Bundle,
    bindings: &TypeBindings,
) -> ContractResult<String> {
    let target = a
        .reference
        .as_ref()
        .map(|id| TypeRef::Model { id: id.clone() })
        .unwrap_or_else(|| a.value_type.clone());
    match target {
        TypeRef::Builtin { name } => builtin(&name, id, &a.name),
        TypeRef::Model { id: target } => names
            .get(&target)
            .map(|n| format!("Box<{n}>"))
            .ok_or_else(|| {
                error(
                    ErrorReason::DanglingReference,
                    id,
                    &a.name,
                    "missing model target",
                )
            }),
        TypeRef::Value { id: target } => {
            if !bundle.value_types.contains_key(&target) {
                return Err(error(
                    ErrorReason::UnknownType,
                    id,
                    &a.name,
                    "missing value type",
                ));
            }
            bindings
                .get(&target)
                .map(|b| b.rust_type.clone())
                .ok_or_else(|| {
                    error(
                        ErrorReason::MissingTypeBinding,
                        id,
                        &a.name,
                        "named value type needs trusted binding",
                    )
                })
        }
    }
}
fn shape(a: &Attribute, ty: &str) -> String {
    match a.collection {
        Collection::Scalar(_) => ty.into(),
        Collection::Bounded { .. } => format!("Vec<{ty}>"),
    }
}
/// Everything the emitter needs, with every validation and decision already made.
struct Plan {
    needs_same_integer: bool,
    models: Vec<ModelPlan>,
}
struct ModelPlan {
    id: String,
    name: String,
    ordered_content: bool,
    choice_checks: String,
    attrs: Vec<AttrPlan>,
}
struct AttrPlan {
    /// Wire name as declared.
    name: String,
    /// Rust member identifier.
    field: String,
    /// Final Rust type after collection shaping.
    shaped_ty: String,
    required: bool,
    bounded: bool,
    bounds: Option<(u64, Option<u64>)>,
    nested_validate: bool,
    /// Wire-form JSON of the default, when one is declared.
    default_wire: Option<String>,
    initialize_empty: bool,
    union: Option<UnionPlan>,
    enumeration: Option<EnumPlan>,
}
struct UnionPlan {
    ty: String,
    field: String,
    tag_attribute: String,
    cases: Vec<UnionCase>,
    base_index: Option<usize>,
}
struct UnionCase {
    tag: String,
    target_name: String,
    declared: Option<String>,
}
struct EnumPlan {
    name: String,
    /// The underlying scalar type the literals are parsed as.
    ty: String,
    integer: bool,
    /// Wire-form JSON of each literal.
    literals: Vec<String>,
}
impl ModelEmitter for RustEmitter {
    fn generate_models(
        &self,
        bundle: &Bundle,
        bindings: &TypeBindings,
    ) -> ContractResult<SourceMap> {
        let plan = plan_models(bundle, bindings)?;
        Ok(BTreeMap::from([("models.rs".into(), emit_models(&plan))]))
    }
}
/// Make every decision and validation for the bundle; emission is then pure.
fn plan_models(bundle: &Bundle, bindings: &TypeBindings) -> ContractResult<Plan> {
    let hoisted = effective_discriminators(bundle)?;
    let bundle = &hoisted;
    let names = model_names(bundle)?;
    let needs_same_integer = bundle.models.values().flat_map(|m| &m.attributes).any(|a| {
        a.enumeration.is_some()
            && matches!(&a.value_type, TypeRef::Builtin { name } if name.is_integer())
    });
    let mut type_names: BTreeSet<String> = names.values().cloned().collect();
    let mut models = Vec::new();
    for (id, m) in &bundle.models {
        validate_ordered_content(id, m)?;
        let name = &names[id];
        let mut fields = BTreeSet::new();
        let mut attrs = Vec::new();
        for a in &m.attributes {
            if a.name == "__lml_content" {
                return Err(error(
                    ErrorReason::ReservedMember,
                    id,
                    &a.name,
                    "reserved ordered-content member",
                ));
            }
            if !fields.insert(member_identifier(&a.name)) {
                return Err(error(
                    ErrorReason::ReservedMember,
                    id,
                    &a.name,
                    "Rust field name collision",
                ));
            }
            if let Collection::Bounded {
                min,
                max,
                class_binding,
            } = &a.collection
            {
                if max.is_some_and(|max| max < *min) {
                    return Err(error(
                        ErrorReason::InvalidDocument,
                        id,
                        &a.name,
                        "invalid cardinality",
                    ));
                }
                if class_binding.is_some() {
                    return Err(error(
                        ErrorReason::MissingTypeBinding,
                        id,
                        &a.name,
                        "custom collection binding unsupported",
                    ));
                }
            }
            let mut ty = field_type(a, id, &names, bundle, bindings)?;
            let mut union = None;
            if let Some(d) = &a.discriminator {
                if a.enumeration.is_some()
                    || a.reference.is_some()
                    || !matches!(a.value_type, TypeRef::Model { .. })
                    || d.cases.is_empty()
                {
                    return Err(error(
                        ErrorReason::InvalidDocument,
                        id,
                        &a.name,
                        "invalid discriminator declaration",
                    ));
                }
                ty = format!("{name}_{}_Union", member_identifier(&a.name));
                if !type_names.insert(ty.clone()) {
                    return Err(error(
                        ErrorReason::ReservedMember,
                        id,
                        &a.name,
                        "union type collision",
                    ));
                }
                union = Some(plan_union(&ty, id, &a.name, d, &names, bundle)?);
            }
            let mut enumeration = None;
            if let Some(values) = &a.enumeration {
                if values.is_empty()
                    || values.iter().enumerate().any(|(i, v)| {
                        values[..i]
                            .iter()
                            .any(|entry| same_enum_literal(a, entry, v))
                    })
                {
                    return Err(error(
                        ErrorReason::InvalidDocument,
                        id,
                        &a.name,
                        "empty or duplicate enum",
                    ));
                }
                // Validate each literal against the declared scalar before emitting.
                for value in values {
                    validate_literal(a, &enum_literal(a, value), id, bundle)?;
                }
                let en = format!("{name}_{}_Enum", member_identifier(&a.name));
                if !type_names.insert(en.clone()) {
                    return Err(error(
                        ErrorReason::ReservedMember,
                        id,
                        &a.name,
                        "enum type collision",
                    ));
                }
                enumeration = Some(EnumPlan {
                    name: en.clone(),
                    ty: ty.clone(),
                    integer: matches!(&a.value_type, TypeRef::Builtin { name } if name.is_integer()),
                    literals: values
                        .iter()
                        .map(|v| serde_json::to_string(&enum_literal(a, v)).unwrap())
                        .collect(),
                });
                ty = en;
            }
            let mut default_wire = None;
            if let DefaultValue::Present(v) = &a.default {
                validate_default(a, v, id, bundle)?;
                default_wire = Some(serde_json::to_string(&wire_default(a, v)).unwrap());
            }
            if a.initialize_empty && matches!(a.collection, Collection::Scalar(_)) {
                return Err(error(
                    ErrorReason::InvalidDocument,
                    id,
                    &a.name,
                    "initialize_empty requires collection",
                ));
            }
            let bounds = match a.collection {
                Collection::Bounded { min, max, .. } => Some((min, max)),
                Collection::Scalar(_) => None,
            };
            attrs.push(AttrPlan {
                name: a.name.clone(),
                field: member_identifier(&a.name),
                shaped_ty: shape(a, &ty),
                required: a.required,
                bounded: bounds.is_some(),
                bounds: if choice_attribute(m, &a.name) {
                    None
                } else {
                    bounds
                },
                nested_validate: a.enumeration.is_none()
                    && (a.discriminator.is_some()
                        || a.reference.is_some()
                        || matches!(a.value_type, TypeRef::Model { .. })),
                default_wire,
                initialize_empty: a.initialize_empty,
                union,
                enumeration,
            });
        }
        models.push(ModelPlan {
            id: id.clone(),
            name: name.clone(),
            ordered_content: m.ordered_content.is_some(),
            choice_checks: compile_model_choices(m, id)?,
            attrs,
        });
    }
    Ok(Plan {
        needs_same_integer,
        models,
    })
}
fn validate_ordered_content(id: &str, m: &ModelDocument) -> ContractResult<()> {
    if let Some(ordered) = &m.ordered_content {
        if ordered.member != "__lml_content" || ordered.version != 1 {
            return Err(error(
                ErrorReason::InvalidOrderedContent,
                id,
                "__lml_content",
                "unsupported ordered-content declaration",
            ));
        }
    }
    let ordered = m.mappings.xml.as_ref().is_some_and(|v| {
        v.ordered == Some(true)
            || v.mixed_content == Some(true)
            || v.root.as_ref().is_some_and(|r| r.mixed == Some(true))
    }) || m
        .mappings
        .json
        .iter()
        .chain(m.mappings.yaml.iter())
        .any(|v| v.ordered == Some(true) || v.mixed_content == Some(true));
    if ordered && m.ordered_content.is_none() {
        return Err(error(
            ErrorReason::InvalidOrderedContent,
            id,
            "__lml_content",
            "ordered/mixed declarations require portable instance tokens",
        ));
    }
    for rule in m
        .mappings
        .json
        .iter()
        .chain(m.mappings.yaml.iter())
        .flat_map(|mapping| &mapping.rules)
    {
        if let Some(d) = &rule.discriminator {
            if m.attributes
                .iter()
                .find(|a| a.name == rule.to)
                .and_then(|a| a.discriminator.as_ref())
                != Some(d)
            {
                return Err(error(
                    ErrorReason::UnsupportedMapping,
                    id,
                    &rule.to,
                    "rule-level discriminator needs an identical typed attribute discriminator",
                ));
            }
        }
    }
    Ok(())
}
/// Pure emission: a function of the plan only, with no decisions or errors.
fn emit_models(plan: &Plan) -> String {
    let mut output = include_str!("runtime.rs.txt").to_owned();
    if plan.needs_same_integer {
        output.push_str("fn same_integer(a:&ExactInteger,b:&ExactInteger)->bool { fn parts(s:&str)->(bool,&str) { let negative=s.starts_with('-'); let digits=s.trim_start_matches(['-','+']).trim_start_matches('0'); (negative&&!digits.is_empty(),digits) } parts(a.as_str())==parts(b.as_str()) }\n");
    }
    for model in &plan.models {
        emit_model(&mut output, model);
    }
    output
}
fn emit_enum(output: &mut String, en: &EnumPlan) {
    let (name, ty) = (&en.name, &en.ty);
    writeln!(output,"#[allow(non_camel_case_types)]\n#[derive(Clone, Debug, PartialEq, Eq)]\npub enum {name} {{").unwrap();
    for index in 0..en.literals.len() {
        writeln!(output, "Case{index},").unwrap();
    }
    output.push_str("}\n");
    writeln!(output,"impl Serialize for {name} {{ fn serialize<S:serde::Serializer>(&self,s:S)->Result<S::Ok,S::Error>{{ match self {{").unwrap();
    for (index, literal) in en.literals.iter().enumerate() {
        writeln!(output,"Self::Case{index} => {{ let v:{ty}=serde_json::from_str({}).map_err(serde::ser::Error::custom)?; v.serialize(s) }},",quote(literal)).unwrap();
    }
    output.push_str("} } }\n");
    writeln!(output,"impl<'de> Deserialize<'de> for {name} {{ fn deserialize<D:serde::Deserializer<'de>>(d:D)->Result<Self,D::Error>{{ let v={ty}::deserialize(d)?;").unwrap();
    for (index, literal) in en.literals.iter().enumerate() {
        let literal = quote(literal);
        if en.integer {
            writeln!(output,"if same_integer(&v,&serde_json::from_str::<{ty}>({literal}).map_err(serde::de::Error::custom)?) {{ return Ok(Self::Case{index}); }}").unwrap();
        } else {
            writeln!(output,"if v == serde_json::from_str::<{ty}>({literal}).map_err(serde::de::Error::custom)? {{ return Ok(Self::Case{index}); }}").unwrap();
        }
    }
    output.push_str("Err(serde::de::Error::custom(\"invalid enum value\")) } }\n");
}
fn emit_model(output: &mut String, model: &ModelPlan) {
    let (id, name) = (&model.id, &model.name);
    for a in &model.attrs {
        if let Some(union) = &a.union {
            emit_union(output, union, id);
        }
        if let Some(en) = &a.enumeration {
            emit_enum(output, en);
        }
    }
    writeln!(output,"#[allow(non_camel_case_types, non_snake_case)]\n#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]\n#[serde(deny_unknown_fields)]\npub struct {name} {{").unwrap();
    for a in &model.attrs {
        writeln!(output,"#[serde(rename = {}, default, skip_serializing_if = \"Presence::is_absent\")]\npub {}: Presence<{}>,",quote(&a.name),a.field,a.shaped_ty).unwrap();
    }
    if model.ordered_content {
        output.push_str("#[serde(default, skip_serializing_if = \"Presence::is_absent\")]\npub __lml_content: Presence<Vec<ContentToken>>,\n");
    }
    output.push_str("}\n");
    writeln!(
        output,
        "impl Default for {name} {{ fn default()->Self {{ Self {{"
    )
    .unwrap();
    for a in &model.attrs {
        writeln!(output, "{}: Presence::Absent,", a.field).unwrap();
    }
    if model.ordered_content {
        output.push_str("__lml_content: Presence::Absent,\n");
    }
    output.push_str("} } }\n");
    writeln!(
        output,
        "impl {name} {{\npub fn apply_defaults(&mut self)->Result<(),ValidationError>{{"
    )
    .unwrap();
    for a in &model.attrs {
        let field = &a.field;
        if let Some(wire) = &a.default_wire {
            writeln!(output,"if self.{field}.is_absent() {{ self.{field}=serde_json::from_str::<Presence<{}>>({}).map_err(|_|invalid({}, {}, \"non_serializable_default\"))?; }}",a.shaped_ty,quote(wire),quote(id),quote(&a.name)).unwrap();
        } else if a.initialize_empty {
            writeln!(
                output,
                "if self.{field}.is_absent() {{ self.{field}=Presence::Value(Vec::new()); }}"
            )
            .unwrap();
        }
    }
    output.push_str("Ok(()) }\npub fn validate(&self)->Result<(),ValidationError>{self.validate_phase(true)}\npub fn validate_shape(&self)->Result<(),ValidationError>{self.validate_phase(false)}\nfn validate_phase(&self, choices:bool)->Result<(),ValidationError>{\n");
    for a in &model.attrs {
        let field = &a.field;
        let err = format!(
            "invalid({}, {}, \"invalid_document\")",
            quote(id),
            quote(&a.name)
        );
        if a.required {
            writeln!(output,"if matches!(self.{field},Presence::Absent|Presence::Null) {{ return Err({err}); }}").unwrap();
        }
        if let Some((min, max)) = a.bounds {
            let mut bounds = Vec::new();
            if min > 0 {
                bounds.push(format!("(v.len() as u64) < {min}u64"));
            }
            if let Some(max) = max {
                bounds.push(format!("(v.len() as u64) > {max}u64"));
            }
            if !bounds.is_empty() {
                writeln!(
                    output,
                    "if let Presence::Value(v)=&self.{field} {{ if {} {{ return Err({err}); }} }}",
                    bounds.join(" || ")
                )
                .unwrap();
            }
        }
        if a.nested_validate {
            if a.bounded {
                writeln!(output,"if let Presence::Value(values)=&self.{field} {{ for v in values {{ v.validate_phase(choices)?; }} }}").unwrap();
            } else {
                writeln!(
                    output,
                    "if let Presence::Value(v)=&self.{field} {{ v.validate_phase(choices)?; }}"
                )
                .unwrap();
            }
        }
    }
    if model.ordered_content {
        let err = format!(
            "invalid({}, \"__lml_content\", \"invalid_ordered_content\")",
            quote(id)
        );
        writeln!(
            output,
            "if matches!(self.__lml_content,Presence::Null) {{ return Err({err}); }}"
        )
        .unwrap();
        output.push_str("if let Presence::Value(tokens)=&self.__lml_content { let mut seen=std::collections::BTreeSet::new(); for token in tokens { if let Some((member,index))=token.reference() { let count=match member {\n");
        for a in &model.attrs {
            let field = &a.field;
            let (pattern, count) = if a.bounded {
                ("v", "v.len() as u64")
            } else {
                ("_", "1")
            };
            writeln!(
                output,
                "{} => match &self.{field} {{ Presence::Value({pattern})=>{count}, _=>0 }},",
                quote(&a.name)
            )
            .unwrap();
        }
        writeln!(output,"_ => return Err({err}), }}; if index>=count || !seen.insert((member,index)) {{ return Err({err}); }} }} }} }}").unwrap();
    }
    if !model.choice_checks.is_empty() {
        writeln!(output, "if choices {{ {} }}", model.choice_checks).unwrap();
    }
    output.push_str("Ok(()) }\n}\n");
}
/// Validate one polymorphic attribute's cases and capture what emission needs.
fn plan_union(
    ty: &str,
    id: &str,
    field: &str,
    d: &Discriminator,
    names: &BTreeMap<String, String>,
    bundle: &Bundle,
) -> ContractResult<UnionPlan> {
    let mut cases = Vec::new();
    for (tag, target) in &d.cases {
        let target_name = names.get(target).ok_or_else(|| {
            error(
                ErrorReason::DanglingReference,
                id,
                field,
                "missing discriminator target",
            )
        })?;
        let declared = bundle.models[target]
            .attributes
            .iter()
            .find(|a| a.name == d.attribute);
        if let Some(a) = declared {
            let plain = matches!(
                a.value_type,
                TypeRef::Builtin {
                    name: BuiltinType::String | BuiltinType::Token | BuiltinType::Symbol
                }
            ) && matches!(a.collection, Collection::Scalar(_))
                && a.enumeration.is_none()
                && a.discriminator.is_none()
                && a.reference.is_none();
            if !plain {
                return Err(error(
                    ErrorReason::UnsupportedMapping,
                    id,
                    field,
                    "discriminator attribute on a case model must be a plain string member",
                ));
            }
        }
        cases.push(UnionCase {
            tag: tag.clone(),
            target_name: target_name.clone(),
            declared: declared.map(|a| member_identifier(&a.name)),
        });
    }
    // The field's declared model: the case an absent tag reads as.
    let base = bundle.models[id]
        .attributes
        .iter()
        .find(|a| a.name == field)
        .and_then(|a| {
            a.reference.clone().or(match &a.value_type {
                TypeRef::Model { id } => Some(id.clone()),
                _ => None,
            })
        });
    let base_index = d
        .cases
        .iter()
        .position(|(_, target)| Some(target) == base.as_ref());
    Ok(UnionPlan {
        ty: ty.to_owned(),
        field: field.to_owned(),
        tag_attribute: d.attribute.clone(),
        cases,
        base_index,
    })
}
/// Emit the closed union for one polymorphic attribute. The wire form is the
/// case model's object plus the discriminator member. A case model that
/// declares the discriminator attribute (Ruby's contract) keeps it as an
/// ordinary member whose value must equal the case's tag; a case model that
/// does not declare it has the tag added and removed around the payload.
fn emit_union(output: &mut String, union: &UnionPlan, id: &str) {
    let ty = &union.ty;
    let field = &union.field;
    let cases = &union.cases;
    let tag_key = quote(&union.tag_attribute);
    writeln!(
        output,
        "#[allow(non_camel_case_types)]\n#[derive(Clone, Debug, PartialEq, Eq)]\npub enum {ty} {{"
    )
    .unwrap();
    for (index, case) in cases.iter().enumerate() {
        writeln!(output, "Case{index}(Box<{}>),", case.target_name).unwrap();
    }
    output.push_str("}\n");
    writeln!(output, "impl Serialize for {ty} {{ fn serialize<S:serde::Serializer>(&self,s:S)->Result<S::Ok,S::Error>{{ let (tag,payload)=match self {{").unwrap();
    for (index, case) in cases.iter().enumerate() {
        // A declared member carries its own value: an absent tag stays absent (Ruby
        // writes nil as no attribute). Only an undeclared case gets the tag added.
        let tag_expr = if case.declared.is_some() {
            "None::<&str>".to_string()
        } else {
            format!("Some({})", quote(&case.tag))
        };
        writeln!(
            output,
            "Self::Case{index}(v) => ({tag_expr}, serde_json::to_value(v)),"
        )
        .unwrap();
    }
    writeln!(output, "}}; let mut payload=match payload.map_err(serde::ser::Error::custom)? {{ serde_json::Value::Object(m)=>m, _=>return Err(serde::ser::Error::custom(\"discriminator payload must be an object\")) }}; if let Some(tag)=tag {{ payload.insert({tag_key}.into(),serde_json::Value::String(tag.into())); }} serde_json::Value::Object(payload).serialize(s) }} }}").unwrap();
    writeln!(output, "impl<'de> Deserialize<'de> for {ty} {{ fn deserialize<D:serde::Deserializer<'de>>(d:D)->Result<Self,D::Error>{{ let mut payload=match serde_json::Value::deserialize(d)? {{ serde_json::Value::Object(m)=>m, _=>return Err(serde::de::Error::custom(\"expected object\")) }}; let tag=match payload.get({tag_key}) {{ Some(serde_json::Value::String(t))=>Some(t.clone()), None=>None, _=>return Err(serde::de::Error::custom(\"missing discriminator\")) }}; match tag.as_deref() {{").unwrap();
    for (index, case) in cases.iter().enumerate() {
        let strip = if case.declared.is_some() {
            ""
        } else {
            &format!("payload.remove({tag_key}); ")
        };
        writeln!(output, "Some({}) => {{ {strip}Ok(Self::Case{index}(Box::new(serde_json::from_value(serde_json::Value::Object(payload)).map_err(serde::de::Error::custom)?))) }},", quote(&case.tag)).unwrap();
    }
    // An absent tag reads as the base case, matching the XML reader and Ruby.
    match union.base_index {
        Some(index) => writeln!(output, "None => {{ Ok(Self::Case{index}(Box::new(serde_json::from_value(serde_json::Value::Object(payload)).map_err(serde::de::Error::custom)?))) }},").unwrap(),
        None => output.push_str("None => Err(serde::de::Error::custom(\"missing discriminator\")),\n"),
    }
    output.push_str("_ => Err(serde::de::Error::custom(\"unknown discriminator\")), } } }\n");
    writeln!(
        output,
        "impl {ty} {{ pub fn validate(&self)->Result<(),ValidationError>{{self.validate_phase(true)}} pub fn validate_shape(&self)->Result<(),ValidationError>{{self.validate_phase(false)}} fn validate_phase(&self,choices:bool)->Result<(),ValidationError>{{ match self {{"
    )
    .unwrap();
    for (index, case) in cases.iter().enumerate() {
        write!(
            output,
            "Self::Case{index}(v) => {{ v.validate_phase(choices)?; "
        )
        .unwrap();
        if let Some(member) = &case.declared {
            write!(output, "if let Presence::Value(t)=&v.{member} {{ if t!={} {{ return Err(invalid({}, {}, \"unknown_discriminator\")); }} }} ", quote(&case.tag), quote(id), quote(field)).unwrap();
        }
        output.push_str("Ok(()) },\n");
    }
    output.push_str("} } }\n");
}
fn same_enum_literal(a: &Attribute, left: &NeutralValue, right: &NeutralValue) -> bool {
    let left = enum_literal(a, left);
    let right = enum_literal(a, right);
    if matches!(&a.value_type, TypeRef::Builtin { name } if name.is_integer()) {
        if let (NeutralValue::String(left), NeutralValue::String(right)) = (&left, &right) {
            return matches!((left.parse::<num_bigint::BigInt>(),right.parse::<num_bigint::BigInt>()),(Ok(left),Ok(right))if left==right);
        }
    }
    left == right
}
/// Ruby exports an integer default as a JSON number, but the generated
/// instance wire form of an integer is an exact string, so the default is
/// rendered to that wire form (per element for collections).
pub(crate) fn wire_default(a: &Attribute, value: &NeutralValue) -> NeutralValue {
    match value {
        NeutralValue::Array(items) => {
            NeutralValue::Array(items.iter().map(|i| wire_default(a, i)).collect())
        }
        _ => enum_literal(a, value),
    }
}
fn enum_literal(a: &Attribute, value: &NeutralValue) -> NeutralValue {
    if matches!(&a.value_type, TypeRef::Builtin { name } if name.is_integer()) {
        match value {
            NeutralValue::Signed(v) => return NeutralValue::String(v.to_string()),
            NeutralValue::Unsigned(v) => return NeutralValue::String(v.to_string()),
            _ => {}
        }
    }
    value.clone()
}
fn validate_literal(
    a: &Attribute,
    v: &NeutralValue,
    id: &str,
    _bundle: &Bundle,
) -> ContractResult<()> {
    let valid = match (&a.value_type, v) {
        (
            TypeRef::Builtin {
                name: BuiltinType::Boolean,
            },
            NeutralValue::Boolean(_),
        ) => true,
        (
            TypeRef::Builtin {
                name: BuiltinType::String | BuiltinType::Token | BuiltinType::Symbol,
            },
            NeutralValue::String(_),
        ) => true,
        (TypeRef::Builtin { name }, NeutralValue::String(s)) if name.is_integer() => {
            let d = s.strip_prefix('-').unwrap_or(s);
            !d.is_empty() && d.bytes().all(|c| c.is_ascii_digit())
        }
        (TypeRef::Builtin { name }, NeutralValue::Signed(_) | NeutralValue::Unsigned(_))
            if name.is_integer() =>
        {
            true
        }
        (
            TypeRef::Builtin {
                name: BuiltinType::Decimal | BuiltinType::Rational,
            },
            NeutralValue::String(s),
        ) => {
            let d = s.strip_prefix('-').unwrap_or(s);
            let parts: Vec<_> = d.split('.').collect();
            parts.len() <= 2
                && parts
                    .iter()
                    .all(|p| !p.is_empty() && p.bytes().all(|c| c.is_ascii_digit()))
        }
        (
            TypeRef::Builtin {
                name: BuiltinType::Float,
            },
            NeutralValue::String(s),
        ) => crate::values::canonical_float(s),
        _ => false,
    };
    if valid {
        Ok(())
    } else {
        Err(error(
            ErrorReason::NonSerializableDefault,
            id,
            &a.name,
            "literal does not match declared type",
        ))
    }
}
fn validate_default(
    a: &Attribute,
    v: &NeutralValue,
    id: &str,
    bundle: &Bundle,
) -> ContractResult<()> {
    if matches!(v, NeutralValue::Null) {
        return if a.required {
            Err(error(
                ErrorReason::NonSerializableDefault,
                id,
                &a.name,
                "required default cannot be null",
            ))
        } else {
            Ok(())
        };
    }
    let values = match (&a.collection, v) {
        (Collection::Bounded { min, max, .. }, NeutralValue::Array(v)) => {
            if (v.len() as u64) < *min || max.is_some_and(|m| (v.len() as u64) > m) {
                return Err(error(
                    ErrorReason::NonSerializableDefault,
                    id,
                    &a.name,
                    "default cardinality",
                ));
            }
            v.as_slice()
        }
        (Collection::Bounded { .. }, _) => {
            return Err(error(
                ErrorReason::NonSerializableDefault,
                id,
                &a.name,
                "collection default must be array",
            ))
        }
        _ => std::slice::from_ref(v),
    };
    for v in values {
        validate_literal(a, v, id, bundle)?;
        if a.enumeration
            .as_ref()
            .is_some_and(|vs| !vs.iter().any(|entry| same_enum_literal(a, entry, v)))
        {
            return Err(error(
                ErrorReason::NonSerializableDefault,
                id,
                &a.name,
                "default outside enum",
            ));
        }
    }
    Ok(())
}

impl PargBindingEmitter for RustEmitter {
    fn generate_parg_bindings(
        &self,
        bundle: &Bundle,
        bindings: &[PargClassBinding],
    ) -> ContractResult<SourceMap> {
        let hoisted = effective_discriminators(bundle)?;
        let bundle = &hoisted;
        let names = model_names(bundle)?;
        let mut output=String::from("// Compile-time PARG capture constructors; parsing and application behavior are external.\nuse super::models::*;\n");
        let mut used = BTreeSet::new();
        for binding in bindings {
            let model = bundle.models.get(&binding.model_id).ok_or_else(|| {
                error(
                    ErrorReason::DanglingReference,
                    &binding.model_id,
                    "",
                    "unknown PARG class",
                )
            })?;
            let function = format!("bind_{}", member_identifier(&binding.rule));
            if !used.insert(function.clone()) {
                return Err(error(
                    ErrorReason::ReservedMember,
                    &binding.model_id,
                    "",
                    "duplicate grammar rule function",
                ));
            }
            let mut members = BTreeSet::new();
            let mut args = Vec::new();
            let mut assignments = Vec::new();
            for (index, (capture, member)) in binding.captures.iter().enumerate() {
                if capture.is_empty() || !members.insert(member) {
                    return Err(error(
                        ErrorReason::InvalidDocument,
                        &binding.model_id,
                        member,
                        "invalid or duplicate capture member",
                    ));
                }
                let a = model
                    .attributes
                    .iter()
                    .find(|a| &a.name == member)
                    .ok_or_else(|| {
                        error(
                            ErrorReason::InvalidDocument,
                            &binding.model_id,
                            member,
                            "unknown capture member",
                        )
                    })?;
                if a.discriminator.is_some()
                    || a.enumeration.is_some()
                    || matches!(a.value_type, TypeRef::Value { .. })
                {
                    return Err(error(
                        ErrorReason::MissingTypeBinding,
                        &binding.model_id,
                        member,
                        "capture requires explicit scalar/model type",
                    ));
                }
                let ty = shape(
                    a,
                    &field_type(a, &binding.model_id, &names, bundle, &TypeBindings::new())?,
                );
                args.push(format!("capture_{index}: {ty}"));
                assignments.push(format!(
                    "{}: Presence::Value(capture_{index})",
                    member_identifier(member)
                ));
                writeln!(
                    output,
                    "// capture {} -> member {}",
                    quote(capture),
                    quote(member)
                )
                .unwrap();
            }
            let model_name = &names[&binding.model_id];
            if assignments.len() < model.attributes.len() || model.ordered_content.is_some() {
                assignments.push("..Default::default()".into());
            }
            writeln!(output,"pub fn {function}({})->Result<{model_name},ValidationError>{{ let mut value={model_name}{{ {} }}; value.apply_defaults()?; value.validate_shape()?; Ok(value) }}",args.join(", "),assignments.join(", ")).unwrap();
        }
        Ok(BTreeMap::from([("parg_bindings.rs".into(), output)]))
    }
}
