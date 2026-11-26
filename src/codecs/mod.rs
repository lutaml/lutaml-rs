//! Compile per-model JSON transformations into Rust functions. Generated models
//! remain static types; JSON values below are transient codec buffers only.
use crate::contracts::*;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;
#[derive(Default)]
pub struct RustJsonEmitter;
fn fail(id: &str, field: &str, detail: &str) -> Box<ContractError> {
    Box::new(ContractError {
        reason: ErrorReason::UnsupportedMapping,
        model_id: Some(id.into()),
        attribute: (!field.is_empty()).then(|| field.into()),
        format: Some("json".into()),
        rule: None,
        location: format!(
            "/models/{}/mappings/json",
            id.replace('~', "~0").replace('/', "~1")
        ),
        klass: None,
        detail: Some(detail.into()),
    })
}
fn q(s: &str) -> String {
    format!("{s:?}")
}

fn target(a: &Attribute) -> Option<&str> {
    a.reference.as_deref().or(match &a.value_type {
        TypeRef::Model { id } => Some(id.as_str()),
        _ => None,
    })
}
fn rules(model: &ModelDocument) -> Vec<KeyRule> {
    model
        .mappings
        .json
        .as_ref()
        .map(|m| m.rules.clone())
        .unwrap_or_else(|| {
            model
                .attributes
                .iter()
                .map(|a| KeyRule {
                    key: vec![a.name.clone()],
                    to: a.name.clone(),
                    render: None,
                    ..Default::default()
                })
                .collect()
        })
}
#[derive(Clone)]
enum Path {
    Key,
    Value,
    Members(Vec<String>),
}
struct Keyed<'a> {
    attribute: &'a Attribute,
    child: &'a ModelDocument,
    root: bool,
    paths: BTreeMap<String, Path>,
}
fn keyed<'a>(
    model: &'a ModelDocument,
    bundle: &'a Bundle,
    id: &str,
) -> ContractResult<Option<Keyed<'a>>> {
    let Some(mapping) = &model.mappings.json else {
        return Ok(None);
    };
    let declarations: Vec<_> = mapping
        .rules
        .iter()
        .filter(|r| r.root_mappings.is_some() || r.child_mappings.is_some())
        .collect();
    if declarations.len() > 1 {
        return Err(fail(
            id,
            "",
            "multiple keyed rule descriptors are not implemented",
        ));
    }
    let rule = declarations.first().copied();
    if rule.is_some() && (mapping.root_mappings.is_some() || mapping.child_mappings.is_some()) {
        return Err(fail(
            id,
            "",
            "conflicting rule and format keyed descriptors",
        ));
    }
    let (roots, children) = rule
        .map(|r| (&r.root_mappings, &r.child_mappings))
        .unwrap_or((&mapping.root_mappings, &mapping.child_mappings));
    let (root, descriptor) = match (roots, children) {
        (None, None) => return Ok(None),
        (Some(d), None) => (true, d),
        (None, Some(d)) => (false, d),
        _ => return Err(fail(id, "", "root and child descriptors cannot both apply")),
    };
    let candidates: Vec<_> = model
        .attributes
        .iter()
        .filter(|a| {
            matches!(a.collection, Collection::Bounded { .. })
                && target(a).is_some()
                && mapping.rules.iter().any(|r| r.to == a.name)
                && rule.is_none_or(|r| r.to == a.name)
        })
        .collect();
    if candidates.len() != 1 || (root && mapping.rules.len() != 1) {
        return Err(fail(
            id,
            "",
            "keyed descriptor needs one unambiguous mapped model collection",
        ));
    }
    if root && model.ordered_content.is_some() {
        return Err(fail(
            id,
            "__lml_content",
            "root keyed mapping cannot envelope ordered-content metadata",
        ));
    }
    let a = candidates[0];
    if a.discriminator.is_some() {
        return Err(fail(
            id,
            &a.name,
            "keyed discriminator collections need explicit bindings",
        ));
    }
    let child = bundle
        .models
        .get(target(a).unwrap())
        .ok_or_else(|| fail(id, &a.name, "unknown collection model"))?;
    let mut paths = BTreeMap::new();
    for (member, spec) in descriptor {
        if !child.attributes.iter().any(|a| &a.name == member) {
            return Err(fail(
                id,
                member,
                "keyed descriptor names unknown child member",
            ));
        }
        let path = match spec {
            NeutralValue::String(s) if s == "key" => Path::Key,
            NeutralValue::String(s) if s == "value" => Path::Value,
            NeutralValue::String(s) => Path::Members(vec![s.clone()]),
            NeutralValue::Array(v) => Path::Members(
                v.iter()
                    .map(|v| {
                        if let NeutralValue::String(s) = v {
                            Ok(s.clone())
                        } else {
                            Err(fail(id, member, "path must contain strings"))
                        }
                    })
                    .collect::<ContractResult<_>>()?,
            ),
            _ => return Err(fail(id, member, "unsupported keyed path")),
        };
        if matches!(&path,Path::Members(v)if v.is_empty()||v.iter().any(String::is_empty)) {
            return Err(fail(id, member, "empty keyed path"));
        }
        paths.insert(member.clone(), path);
    }
    if paths.values().filter(|p| matches!(p, Path::Key)).count() != 1 {
        return Err(fail(
            id,
            &a.name,
            "keyed collection requires exactly one key member",
        ));
    }
    let paths_only: Vec<_> = paths
        .values()
        .filter_map(|p| match p {
            Path::Members(v) => Some(v.as_slice()),
            Path::Value => Some([].as_slice()),
            Path::Key => None,
        })
        .collect();
    for (i, a) in paths_only.iter().enumerate() {
        if paths_only[..i]
            .iter()
            .any(|b| a.starts_with(b) || b.starts_with(a))
        {
            return Err(fail(id, &a.join("/"), "overlapping keyed mapping paths"));
        }
    }
    Ok(Some(Keyed {
        attribute: a,
        child,
        root,
        paths,
    }))
}
/// A rule with no declared attribute: handled entirely by handwritten hooks.
fn hook_only(rule: &KeyRule) -> bool {
    rule.to.is_empty()
}
/// The attribute-backed direction is skipped when a handwritten hook owns it.
fn read_by_hook(rule: &KeyRule) -> bool {
    rule.custom.as_ref().is_some_and(|c| c.from.is_some()) || hook_only(rule)
}
fn write_by_hook(rule: &KeyRule) -> bool {
    rule.custom.as_ref().is_some_and(|c| c.to.is_some()) || hook_only(rule)
}
pub(crate) const RUST_KEYWORDS: &[&str] = &[
    "as", "break", "const", "continue", "crate", "else", "enum", "extern", "false", "fn", "for",
    "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref", "return",
    "self", "Self", "static", "struct", "super", "trait", "true", "type", "unsafe", "use", "where",
    "while", "async", "await", "dyn", "abstract", "become", "box", "do", "final", "macro",
    "override", "priv", "typeof", "unsized", "virtual", "yield", "try", "gen",
];
/// Hook method names of one model, in rule order: (name, is_read, wire key).
fn hook_methods(rules: &[KeyRule]) -> Vec<(String, bool, String)> {
    let mut out: Vec<(String, bool, String)> = Vec::new();
    for rule in rules {
        let Some(custom) = &rule.custom else { continue };
        for (name, read) in [(&custom.from, true), (&custom.to, false)] {
            if let Some(name) = name {
                if !out.iter().any(|(n, _, _)| n == name) {
                    out.push((
                        name.clone(),
                        read,
                        rule.key.first().cloned().unwrap_or_default(),
                    ));
                }
            }
        }
    }
    out
}
fn aliases(model: &ModelDocument, id: &str) -> ContractResult<Vec<KeyRule>> {
    let rules = rules(model);
    let mut keys = BTreeSet::new();
    let mut members = BTreeSet::new();
    for rule in &rules {
        if (rule.key.is_empty()
            && rule.root_mappings.is_none()
            && model
                .mappings
                .json
                .as_ref()
                .is_none_or(|m| m.root_mappings.is_none()))
            || !hook_only(rule)
                && (!members.insert(rule.to.clone())
                    || !model.attributes.iter().any(|a| a.name == rule.to))
            || hook_only(rule) && rule.custom.is_none()
        {
            return Err(fail(id, &rule.to, "invalid or duplicate mapped member"));
        }
        for key in &rule.key {
            if key.is_empty() || !keys.insert(key) {
                return Err(fail(id, &rule.to, "ambiguous wire alias"));
            }
        }
    }
    Ok(rules)
}
fn transformed(
    out: &mut String,
    a: &Attribute,
    id: &str,
    names: &BTreeMap<String, String>,
    encode: bool,
) -> ContractResult<()> {
    let direction = if encode { "encode" } else { "decode" };
    // The local `v` is one typed member value, never stored on a generated model.
    if let Some(d) = &a.discriminator {
        // A missing tag reads (and writes) as the declared base type, with the
        // tag left absent: Ruby and the TS target do the same.
        let base = target(a)
            .and_then(|t| names.get(t))
            .ok_or_else(|| fail(id, &a.name, "unknown discriminator base"))?;
        writeln!(out,"let mut payload=object(v,{})?; let tag=match payload.remove({}) {{ None=>None, Some(t)=>Some(t.as_str().map(str::to_owned).ok_or_else(||failure(ErrorReason::UnknownDiscriminator,{}, {}, \"/input\", \"non-string discriminator\"))?) }}; let payload=Value::Object(payload); match tag {{ None=>{{ v={direction}_{base}(payload)?; }} Some(tag)=>{{ v=match tag.as_str() {{",q(id),q(&d.attribute),q(id),q(&a.name)).unwrap();
        for (tag, target) in &d.cases {
            let name = names
                .get(target)
                .ok_or_else(|| fail(id, &a.name, "unknown discriminator target"))?;
            writeln!(out, "{} => {direction}_{name}(payload)?,", q(tag)).unwrap();
        }
        writeln!(out,"_=>return Err(failure(ErrorReason::UnknownDiscriminator,{}, {}, \"/input\", \"unknown discriminator\")), }}; v.as_object_mut().ok_or_else(||failure(ErrorReason::InvalidDocument,{}, {}, \"/input\", \"invalid discriminator payload\"))?.insert({}.into(),Value::String(tag)); }} }}",q(id),q(&a.name),q(id),q(&a.name),q(&d.attribute)).unwrap();
    } else if let Some(target) = target(a) {
        let name = names
            .get(target)
            .ok_or_else(|| fail(id, &a.name, "unknown model reference"))?;
        writeln!(out, "v={direction}_{name}(v)?;").unwrap();
    } else if let TypeRef::Builtin { name } = &a.value_type {
        let exact = match name {
            t if t.is_integer() => Some("ExactInteger"),
            BuiltinType::Decimal | BuiltinType::Rational => Some("ExactDecimal"),
            BuiltinType::Float => Some("PortableFloat"),
            _ => None,
        };
        if let Some(exact) = exact {
            writeln!(out,"let lexical=v.as_str().ok_or_else(||failure(ErrorReason::InvalidDocument,{}, {}, \"/input\", \"exact value requires string (portable-exact-json-v1 requires exact values as strings; numeric JSON is ruby-native-v1 and is not accepted)\"))?; super::values::{exact}::parse(lexical).map_err(|e|failure(e.reason.clone(),{}, {}, \"/input\",e.to_string()))?;",q(id),q(&a.name),q(id),q(&a.name)).unwrap();
        }
    }
    Ok(())
}
fn member(
    out: &mut String,
    a: &Attribute,
    id: &str,
    names: &BTreeMap<String, String>,
    encode: bool,
) -> ContractResult<()> {
    let needs_transform = a.discriminator.is_some()
        || target(a).is_some()
        || matches!(
            a.value_type,
            TypeRef::Builtin {
                name: BuiltinType::Integer
                    | BuiltinType::PositiveInteger
                    | BuiltinType::NonNegativeInteger
                    | BuiltinType::Decimal
                    | BuiltinType::Rational
                    | BuiltinType::Float
            }
        );
    if !needs_transform {
        return Ok(());
    }
    out.push_str("if !v.is_null() {\n");
    match a.collection {
        Collection::Scalar(_) => transformed(out, a, id, names, encode)?,
        Collection::Bounded { .. } => {
            writeln!(out,"let array=v.as_array_mut().ok_or_else(||failure(ErrorReason::InvalidDocument,{}, {}, \"/input\", \"collection requires array\"))?; for slot in array {{ let mut v=slot.take();",q(id),q(&a.name)).unwrap();
            transformed(out, a, id, names, encode)?;
            out.push_str("*slot=v; }\n");
        }
    }
    out.push_str("}\n");
    Ok(())
}
fn resolved(value: &RenderAction, affirmative: RenderAction) -> RenderAction {
    match value {
        RenderAction::Absent | RenderAction::Boolean(false) => RenderAction::Omit,
        RenderAction::Boolean(true) => affirmative,
        v => v.clone(),
    }
}
fn check_rule(rule: &KeyRule, id: &str) -> ContractResult<()> {
    if rule.namespace != Namespace::Inherit
        || !matches!(rule.kind, XmlRuleKind::Unspecified)
        || rule.name.is_some()
        || rule.prefix.is_some()
        || rule.cdata.is_some()
        || rule.members.is_some()
        || rule.min.is_some()
        || rule.max.is_some()
        || rule.polymorphic_map.is_some()
    {
        return Err(fail(id, &rule.to, "unsupported JSON rule metadata"));
    }
    if let Some(policy) = &rule.render {
        if matches!(policy.nil, RenderAction::Other(_))
            || matches!(policy.empty, RenderAction::Other(_))
            || !matches!(
                policy.default,
                RenderAction::Absent | RenderAction::Boolean(_)
            )
        {
            return Err(fail(id, &rule.to, "unsupported render policy"));
        }
    }
    Ok(())
}
/// Whether `policy` emits a line that reads `applied` for this member.
fn reads_applied(rule: &KeyRule, a: &Attribute) -> bool {
    let renders_default = matches!(
        rule.render.as_ref().map(|p| &p.default),
        Some(RenderAction::Boolean(true))
    );
    !renders_default && matches!(a.default, DefaultValue::Present(_))
}
/// A value equal to its declared default is suppressed (`render default: false`) only when `apply_defaults`
/// supplied it: the model records that in `__lml_applied`, which `encode_*` reads as `applied`. A value the
/// caller provided (decoded, assigned or constructed) is always rendered, like Ruby's `using_default?`.
fn policy(out: &mut String, rule: &KeyRule, a: &Attribute) {
    let default = RenderPolicy {
        nil: RenderAction::Omit,
        empty: RenderAction::Omit,
        default: false.into(),
    };
    let p = rule.render.as_ref().unwrap_or(&default);
    if reads_applied(rule, a) {
        if let DefaultValue::Present(v) = &a.default {
            writeln!(out,"if applied.iter().any(|m|m=={}) && v==serde_json::from_str::<Value>({}).expect(\"generated literal\") {{ continue; }}",q(&a.name),q(&serde_json::to_string(&crate::generator::wire_default(a, v)).unwrap())).unwrap();
        }
    }
    match resolved(&p.nil, RenderAction::Nil) {
        RenderAction::Omit => out.push_str("if v.is_null(){continue;}\n"),
        RenderAction::Blank | RenderAction::AsBlank => {
            let blank = if matches!(a.collection, Collection::Bounded { .. }) {
                "Value::Array(Vec::new())"
            } else {
                "Value::String(String::new())"
            };
            writeln!(out, "if v.is_null(){{v={blank};}}").unwrap();
        }
        _ => {}
    }
    // A present model instance is not an empty container, even when all its
    // members serialize away. Hashes and model collections still have their
    // ordinary empty-container policy.
    let empty = if target(a).is_some() && matches!(a.collection, Collection::Scalar(_)) {
        "!v.is_object() && empty(&v)"
    } else {
        "empty(&v)"
    };
    match resolved(&p.empty, RenderAction::Blank) {
        RenderAction::Omit => writeln!(out, "if {empty}{{continue;}}").unwrap(),
        RenderAction::Nil | RenderAction::AsNil => {
            writeln!(out, "if {empty}{{v=Value::Null;}}").unwrap()
        }
        _ => {}
    }
}
impl JsonCodecEmitter for RustJsonEmitter {
    fn generate_json_codecs(
        &self,
        bundle: &Bundle,
        bindings: &TypeBindings,
    ) -> ContractResult<SourceMap> {
        // Sharing the actual model emitter validation prevents a codec artifact that
        // claims successful support for a type the static model cannot represent.
        crate::generator::RustEmitter.generate_models(bundle, bindings)?;
        let mut hoisted = crate::generator::effective_discriminators(bundle)?;
        crate::generator::named_values::realize_named_enums(&mut hoisted, bindings)?;
        let bundle = &hoisted;
        if bindings
            .values()
            .any(|b| b.codec.is_some() && b.xml_codec_module().is_none())
        {
            return Err(fail(
                "",
                "",
                "custom codec bindings require a declared callable signature",
            ));
        }

        let names = crate::generator::model_names(bundle)?;
        let mut out = include_str!("runtime.rs.txt").to_owned();
        // A bundle with a `hash` attribute reads numbers loosely at parse time (the
        // hash decides which spellings it can hold) and refuses them everywhere else
        // in a walk of the decoded model (`strict_*`); other bundles keep the strict
        // parse-time refusal.
        let has_hash = bundle.models.values().any(|m| {
            m.attributes.iter().any(|a| {
                matches!(
                    a.value_type,
                    TypeRef::Builtin {
                        name: BuiltinType::Hash
                    }
                )
            })
        });
        writeln!(out, "const NEUTRAL_NUMBERS: bool = {has_hash};").unwrap();
        // Role prefixes (read_, write_, decode_, hydrate_, unkey_, encode_, key_)
        // never prefix one another, so model names cannot alias across roles;
        // this check proves it for the emitted set instead of assuming it.
        let mut functions = BTreeSet::new();
        let hooked_ids = hooked_models(bundle);
        refuse_nested_hooks(bundle, &hooked_ids, &names)?;
        if !hooked_ids.is_empty() {
            out.push_str(HOOK_SUPPORT);
        }
        for (id, model) in &bundle.models {
            let name = &names[id];
            for function in [
                format!("read_{name}_json"),
                format!("write_{name}_json"),
                format!("decode_{name}"),
                format!("hydrate_{name}"),
                format!("unkey_{name}"),
                format!("encode_{name}"),
                format!("key_{name}"),
            ] {
                if !functions.insert(function) {
                    return Err(fail(id, "", "emitted function name collision"));
                }
            }
            if hooked_ids.contains(id) {
                for item in [
                    format!("read_{name}_json_with"),
                    format!("write_{name}_json_with"),
                    format!("{name}Hooks"),
                    format!("{name}NoHooks"),
                ] {
                    if names.values().any(|n| *n == item) || !functions.insert(item) {
                        return Err(fail(id, "", "emitted hook name collision"));
                    }
                }
            }
            if model
                .mappings
                .json
                .as_ref()
                .is_some_and(|m| m.root.is_some() || m.namespace.is_some())
            {
                return Err(fail(
                    id,
                    "",
                    "JSON root/namespace metadata is not implemented",
                ));
            }
            let rules = aliases(model, id)?;
            for rule in &rules {
                check_rule(rule, id)?;
            }

            let keyed = keyed(model, bundle, id)?;
            let guard = provenance_guard(bundle);
            let methods = hook_methods(&rules);
            if !methods.is_empty() && keyed.is_some() {
                return Err(fail(
                    id,
                    "",
                    "hooks on a keyed-collection mapping are not supported",
                ));
            }
            if let Some((bad, _, key)) = methods
                .iter()
                .find(|(n, _, _)| RUST_KEYWORDS.contains(&n.as_str()))
            {
                return Err(fail(id, key, &format!("hook name {bad} is a Rust keyword")));
            }
            let (ordered, strict) = if has_hash {
                (
                    format!("ordered_maps({})?;", q(id)),
                    format!("strict_{name}(&data,{})?;", q(id)),
                )
            } else {
                (String::new(), String::new())
            };
            // The Ruby key-value fast path defers nonempty choice batches,
            // while missing/empty required collections retain eager ingestion checks.
            let mut eager_choices = String::new();
            for attr in &model.attributes {
                if crate::generator::model_choices::choice_attribute(model, &attr.name)
                    && matches!(attr.collection, Collection::Bounded { min: 1.., .. })
                {
                    let field = crate::generator::member_identifier(&attr.name);
                    writeln!(eager_choices,"if !matches!(&model.{field},Presence::Value(v) if !v.is_empty()) {{return Err(failure(ErrorReason::InvalidDocument,{}, {}, \"/input\",\"required choice collection is empty\"));}}",q(id),q(&attr.name)).unwrap();
                }
            }
            if methods.is_empty() {
                writeln!(out,"#[allow(non_snake_case)]\npub fn read_{name}_json(input:&str)->ContractResult<{name}>{{{ordered}let data=decode_{name}(parse_json(input,{})?)?;{strict} let model:{name}=serde_json::from_value(data).map_err(|e|failure(ErrorReason::InvalidDocument,{}, \"\", \"/input\",e))?; {eager_choices}model.validate_shape().map_err(|e|failure(ErrorReason::InvalidDocument,e.model_id,e.attribute,\"/input\",e.reason))?;Ok(model)}}",q(id),q(id)).unwrap();
                writeln!(out,"#[allow(non_snake_case)]\npub fn write_{name}_json(model:&{name})->ContractResult<String>{{{ordered}model.validate_shape().map_err(|e|failure(ErrorReason::InvalidDocument,e.model_id,e.attribute,\"/output\",e.reason))?;{guard}let data=serde_json::to_value(model).map_err(|e|failure(ErrorReason::InvalidDocument,{}, \"\", \"/output\",e))?;serde_json::to_string(&encode_{name}(data)?).map_err(|e|failure(ErrorReason::InvalidDocument,{}, \"\", \"/output\",e))}}",q(id),q(id)).unwrap();
            } else {
                let pre = Prelude {
                    ordered: ordered.clone(),
                    strict: strict.clone(),
                    guard: guard.to_string(),
                    eager_choices,
                };
                emit_hooked_entry_points(&mut out, id, name, &rules, &methods, &pre);
            }
            if has_hash {
                emit_strict(&mut out, id, name, model, &names)?;
            }
            if let Some(k) = &keyed {
                emit_keyed(&mut out, id, name, k, bundle, &names, has_hash)?;
            }
            writeln!(out,"#[allow(non_snake_case)]\nfn decode_{name}(input:Value)->ContractResult<Value>{{let mut wire=object(input,{})?; let mut data=Map::new();",q(id)).unwrap();
            if keyed.as_ref().is_some_and(|k| k.root) {
                let k = keyed.as_ref().unwrap();
                writeln!(out,"data.insert({}.into(), unkey_{name}(Value::Object(std::mem::take(&mut wire)))?);",q(&k.attribute.name)).unwrap();
            } else {
                for rule in rules.iter().filter(|r| !read_by_hook(r)) {
                    out.push_str("{ let mut selected=None;\n");
                    for key in &rule.key {
                        writeln!(out,"if let Some(v)=wire.remove({}) {{ if selected.is_some() {{ return Err(failure(ErrorReason::InvalidDocument,{}, {}, \"/input\", \"multiple aliases for member\")); }} selected=Some(v); }}",q(key),q(id),q(&rule.to)).unwrap();
                    }
                    writeln!(
                        out,
                        "if let Some(v)=selected {{ data.insert({}.into(),v); }} }}",
                        q(&rule.to)
                    )
                    .unwrap();
                }
            }
            if model.ordered_content.is_some() {
                out.push_str("if let Some(v)=wire.remove(\"__lml_content\"){data.insert(\"__lml_content\".into(),v); }\n");
            }
            let loose = if has_hash {
                format!("for v in wire.values(){{no_loose(v,{})?;}}", q(id))
            } else {
                String::new()
            };
            writeln!(out,"if !wire.is_empty(){{{loose}return Err(failure(ErrorReason::InvalidDocument,{}, \"\", \"/input\", \"unknown wire key\"));}} hydrate_{name}(data) }}",q(id)).unwrap();
            writeln!(out,"#[allow(non_snake_case)]\nfn hydrate_{name}(mut data:Map<String,Value>)->ContractResult<Value>{{").unwrap();
            for a in &model.attributes {
                writeln!(out, "if let Some(mut v)=data.remove({}){{", q(&a.name)).unwrap();
                if keyed
                    .as_ref()
                    .is_some_and(|k| !k.root && k.attribute.name == a.name)
                {
                    writeln!(out, "if !v.is_null(){{v=unkey_{name}(v)?;}}").unwrap();
                } else if !keyed
                    .as_ref()
                    .is_some_and(|k| k.root && k.attribute.name == a.name)
                {
                    member(&mut out, a, id, &names, false)?;
                }
                writeln!(out, "data.insert({}.into(),v);}}", q(&a.name)).unwrap();
            }
            out.push_str("Ok(Value::Object(data))}\n");
            writeln!(out,"#[allow(non_snake_case)]\nfn encode_{name}(input:Value)->ContractResult<Value>{{let mut data=object(input,{})?;let mut wire=Map::new();",q(id)).unwrap();
            let root_keyed = keyed.as_ref().is_some_and(|k| k.root);
            let reads = !root_keyed
                && rules.iter().filter(|r| !write_by_hook(r)).any(|r| {
                    model
                        .attributes
                        .iter()
                        .find(|a| a.name == r.to)
                        .is_some_and(|a| reads_applied(r, a))
                });
            if tracks_defaults(model) && !reads {
                // The provenance list is consumed here (it is never wire output) but nothing reads it.
                out.push_str("data.remove(\"__lml_applied\");\n");
            } else if tracks_defaults(model) {
                out.push_str("let applied:Vec<String>=match data.remove(\"__lml_applied\"){Some(Value::Array(a))=>a.into_iter().filter_map(|v|v.as_str().map(str::to_owned)).collect(),_=>Vec::new()};\n");
            }
            if keyed.as_ref().is_some_and(|k| k.root) {
                let k = keyed.as_ref().unwrap();
                writeln!(out,"if let Some(v)=data.remove({}){{return key_{name}(v);}}Ok(Value::Object(wire))}}",q(&k.attribute.name)).unwrap();
            } else {
                for rule in rules.iter().filter(|r| !write_by_hook(r)) {
                    let a = model.attributes.iter().find(|a| a.name == rule.to).unwrap();
                    writeln!(
                        out,
                        "for mut v in data.remove({}).into_iter(){{",
                        q(&rule.to)
                    )
                    .unwrap();
                    policy(&mut out, rule, a);
                    if keyed.as_ref().is_some_and(|k| k.attribute.name == a.name) {
                        writeln!(out, "if !v.is_null(){{v=key_{name}(v)?;}}").unwrap();
                    } else {
                        member(&mut out, a, id, &names, true)?;
                    }
                    writeln!(out, "wire.insert({}.into(),v);}}", q(&rule.key[0])).unwrap();
                }
                if model.ordered_content.is_some() {
                    out.push_str("if let Some(v)=data.remove(\"__lml_content\"){wire.insert(\"__lml_content\".into(),v); }\n");
                }
                out.push_str("Ok(Value::Object(wire))}\n");
            }
        }
        Ok(BTreeMap::from([("json_codecs.rs".into(), out)]))
    }
}
/// The model records `apply_defaults` provenance exactly when a member declares a default.
fn tracks_defaults(model: &ModelDocument) -> bool {
    model
        .attributes
        .iter()
        .any(|a| matches!(a.default, DefaultValue::Present(_)))
}
/// Serializing models for the wire also emits their applied-default lists.
fn provenance_guard(bundle: &Bundle) -> &'static str {
    if bundle.models.values().any(tracks_defaults) {
        "let _provenance=ProvenanceWire::enter();"
    } else {
        ""
    }
}
/// Models whose JSON mapping carries at least one `custom` hook.
fn hooked_models(bundle: &Bundle) -> BTreeSet<String> {
    bundle
        .models
        .iter()
        .filter(|(_, m)| {
            m.mappings
                .json
                .as_ref()
                .is_some_and(|j| j.rules.iter().any(|r| r.custom.is_some()))
        })
        .map(|(id, _)| id.clone())
        .collect()
}
/// A hooked model is only reachable as a root: hooks need the typed model instance, which a
/// nested member does not have while its parent is still being decoded. Refused by name.
fn refuse_nested_hooks(
    bundle: &Bundle,
    hooked: &BTreeSet<String>,
    names: &BTreeMap<String, String>,
) -> ContractResult<()> {
    let _ = names;
    for (id, model) in &bundle.models {
        for a in &model.attributes {
            let mut targets: Vec<&str> = target(a).into_iter().collect();
            if let Some(d) = &a.discriminator {
                targets.extend(d.cases.values().map(String::as_str));
            }
            if let Some(t) = targets.into_iter().find(|t| hooked.contains(*t)) {
                return Err(fail(
                    id,
                    &a.name,
                    &format!("{t} has serialization hooks and cannot be a member of another model's json mapping"),
                ));
            }
        }
    }
    Ok(())
}
const HOOK_SUPPORT: &str = "// Handwritten serialization hooks: see the `<Model>Hooks` traits.\n";
/// Statements every generated entry point of one model starts with.
struct Prelude {
    ordered: String,
    strict: String,
    guard: String,
    eager_choices: String,
}
/// The hook trait with failing stubs, the no-hook stub type, and the `_with` entry points.
fn emit_hooked_entry_points(
    out: &mut String,
    id: &str,
    name: &str,
    rules: &[KeyRule],
    methods: &[(String, bool, String)],
    pre: &Prelude,
) {
    let (ordered, strict, guard) = (&pre.ordered, &pre.strict, &pre.guard);
    writeln!(out, "/// Trusted handwritten hooks for {id}. `S` is the explicit application state for values that live only in hooks.\n/// Every method defaults to a stub failing with `custom_serialization`; override only what the document declares.\n#[allow(non_camel_case_types)]\npub trait {name}Hooks<S> {{").unwrap();
    for (hook, read, key) in methods {
        if *read {
            writeln!(out,"/// READ hook (model, wire member value, state): runs only when a read reaches its key, after the declared\n/// data is built and BEFORE validation, so it may fill a declared (even required) member.\nfn {hook}(&self, _model:&mut {name}, _value:&Value, _state:&mut S)->ContractResult<()> {{ Err(failure(ErrorReason::CustomSerialization,{},\"\",{},{})) }}",q(id),q(&format!("/{key}")),q(&format!("the handwritten read hook {hook} is not implemented"))).unwrap();
        } else {
            writeln!(out,"/// WRITE hook (model, document object, state): runs on every write of the model; fills the document object.\nfn {hook}(&self, _model:&{name}, _document:&mut Map<String,Value>, _state:&mut S)->ContractResult<()> {{ Err(failure(ErrorReason::CustomSerialization,{},\"\",{},{})) }}",q(id),q(&format!("/{key}")),q(&format!("the handwritten write hook {hook} is not implemented"))).unwrap();
        }
    }
    writeln!(out,"}}\n/// No hooks installed: every attempted hook direction fails with `custom_serialization`.\n#[allow(non_camel_case_types)]\npub struct {name}NoHooks;\nimpl<S> {name}Hooks<S> for {name}NoHooks {{}}").unwrap();
    writeln!(out,"#[allow(non_snake_case)]\npub fn read_{name}_json(input:&str)->ContractResult<{name}>{{ read_{name}_json_with(input,&{name}NoHooks,&mut ()) }}").unwrap();
    writeln!(out,"#[allow(non_snake_case)]\npub fn write_{name}_json(model:&{name})->ContractResult<String>{{ write_{name}_json_with(model,&{name}NoHooks,&mut ()) }}").unwrap();
    // READ: hook-owned keys are taken off the wire first; declared data decodes as before; hooks run after.
    writeln!(out,"#[allow(non_snake_case)]\npub fn read_{name}_json_with<S,H:{name}Hooks<S>>(input:&str,hooks:&H,state:&mut S)->ContractResult<{name}>{{{ordered}let mut wire=object(parse_json(input,{})?,{})?; let mut pending:Vec<(usize,Value)>=Vec::new();",q(id),q(id)).unwrap();
    for (index, rule) in rules.iter().enumerate().filter(|(_, r)| read_by_hook(r)) {
        out.push_str("{ let mut selected=None;\n");
        for key in &rule.key {
            writeln!(out,"if let Some(v)=wire.remove({}) {{ if selected.is_some() {{ return Err(failure(ErrorReason::InvalidDocument,{},\"\",\"/input\",\"multiple aliases for member\")); }} selected=Some(({},v)); }}",q(key),q(id),q(key)).unwrap();
        }
        match rule.custom.as_ref().and_then(|c| c.from.as_ref()) {
            Some(_) => writeln!(out,"if let Some((_,v))=selected {{ pending.push(({index},v)); }} }}").unwrap(),
            None => writeln!(out,"if let Some((key,_))=selected {{ return Err(failure(ErrorReason::CustomSerialization,{},\"\",&format!(\"/{{key}}\"),\"no read hook is declared for this key\")); }} }}",q(id)).unwrap(),
        }
    }
    writeln!(out,"let data=decode_{name}(Value::Object(wire))?;{strict} let mut model:{name}=serde_json::from_value(data).map_err(|e|failure(ErrorReason::InvalidDocument,{}, \"\", \"/input\",e))?;\nfor (index,v) in pending {{ match index {{",q(id)).unwrap();
    for (index, rule) in rules.iter().enumerate().filter(|(_, r)| read_by_hook(r)) {
        if let Some(hook) = rule.custom.as_ref().and_then(|c| c.from.as_ref()) {
            writeln!(out, "{index} => hooks.{hook}(&mut model,&v,state)?,").unwrap();
        }
    }
    let eager_choices = &pre.eager_choices;
    out.push_str("_ => unreachable!(\"pending index comes from this function\") } }\n");
    out.push_str(eager_choices);
    out.push_str("model.validate_shape().map_err(|e|failure(ErrorReason::InvalidDocument,e.model_id,e.attribute,\"/input\",e.reason))?;\nOk(model)}\n");
    // WRITE: declared data encodes first; every write reaches each hook-owned rule in rule order.
    writeln!(out,"#[allow(non_snake_case)]\npub fn write_{name}_json_with<S,H:{name}Hooks<S>>(model:&{name},hooks:&H,state:&mut S)->ContractResult<String>{{{ordered}model.validate_shape().map_err(|e|failure(ErrorReason::InvalidDocument,e.model_id,e.attribute,\"/output\",e.reason))?;{guard}let data=serde_json::to_value(model).map_err(|e|failure(ErrorReason::InvalidDocument,{}, \"\", \"/output\",e))?;let Value::Object(mut wire)=encode_{name}(data)? else {{ return Err(failure(ErrorReason::InvalidDocument,{}, \"\", \"/output\", \"expected object\")); }};",q(id),q(id)).unwrap();
    for rule in rules.iter().filter(|r| write_by_hook(r)) {
        let key = rule.key.first().cloned().unwrap_or_default();
        match rule.custom.as_ref().and_then(|c| c.to.as_ref()) {
            Some(hook) => writeln!(out,"hooks.{hook}(model,&mut wire,state)?;").unwrap(),
            None => writeln!(out,"return Err(failure(ErrorReason::CustomSerialization,{},\"\",{},\"no write hook is declared for this key\"));",q(id),q(&format!("/{key}"))).unwrap(),
        }
    }
    // Keys follow rule order (hook-written ones included), like the source and the TS target.
    let order = rules
        .iter()
        .filter_map(|r| r.key.first())
        .map(|k| q(k))
        .collect::<Vec<_>>()
        .join(",");
    writeln!(out,"let mut ordered=Map::new(); for key in [{order}] {{ if let Some(v)=wire.remove(key) {{ ordered.insert(key.into(),v); }} }} ordered.extend(wire);\nserde_json::to_string(&Value::Object(ordered)).map_err(|e|failure(ErrorReason::InvalidDocument,{}, \"\", \"/output\",e))}}",q(id)).unwrap();
}
fn path_expr(path: &[String]) -> String {
    format!(
        "&[{}]",
        path.iter().map(|s| q(s)).collect::<Vec<_>>().join(",")
    )
}
/// Walks a decoded model refusing every number that is not a plain 64-bit integer,
/// except inside a `hash` attribute, which holds any exact neutral value itself.
fn emit_strict(
    out: &mut String,
    id: &str,
    name: &str,
    model: &ModelDocument,
    names: &BTreeMap<String, String>,
) -> ContractResult<()> {
    writeln!(out,"#[allow(non_snake_case)]\nfn strict_{name}(v:&Value,id:&str)->ContractResult<()>{{let Some(o)=v.as_object() else {{return no_loose(v,id);}}; for (k,x) in o {{ match k.as_str() {{").unwrap();
    for a in &model.attributes {
        if let Some(d) = &a.discriminator {
            let base = target(a)
                .and_then(|t| names.get(t))
                .ok_or_else(|| fail(id, &a.name, "unknown discriminator base"))?;
            let mut arms = String::new();
            for (tag, case) in &d.cases {
                let case = names
                    .get(case)
                    .ok_or_else(|| fail(id, &a.name, "unknown discriminator target"))?;
                write!(arms, "Some({})=>strict_{case}(p,id),", q(tag)).unwrap();
            }
            writeln!(out,"{} => strict_each(x,&|p|match p.get({}).and_then(Value::as_str){{{arms}_=>strict_{base}(p,id)}})?,",q(&a.name),q(&d.attribute)).unwrap();
        } else if let Some(t) = target(a) {
            let child = names
                .get(t)
                .ok_or_else(|| fail(id, &a.name, "unknown model reference"))?;
            writeln!(
                out,
                "{} => strict_each(x,&|p|strict_{child}(p,id))?,",
                q(&a.name)
            )
            .unwrap();
        } else if matches!(
            a.value_type,
            TypeRef::Builtin {
                name: BuiltinType::Hash
            }
        ) {
            writeln!(out, "{} => neutral_numbers(x,id)?,", q(&a.name)).unwrap();
        }
    }
    out.push_str("_ => no_loose(x,id)?, } } Ok(()) }\n");
    Ok(())
}
fn emit_keyed(
    out: &mut String,
    id: &str,
    parent: &str,
    k: &Keyed<'_>,
    bundle: &Bundle,
    names: &BTreeMap<String, String>,
    has_hash: bool,
) -> ContractResult<()> {
    let child = names.get(target(k.attribute).unwrap()).unwrap();
    let key = k
        .paths
        .iter()
        .find(|(_, p)| matches!(p, Path::Key))
        .unwrap()
        .0;
    let keyonly = k.paths.len() == 1;
    writeln!(out,"#[allow(non_snake_case)]\nfn unkey_{parent}(input:Value)->ContractResult<Value>{{let input=object(input,{})?;let mut items=Vec::new();for (key,body)in input{{",q(id)).unwrap();
    if keyonly {
        writeln!(
            out,
            "let mut item=object(decode_{child}(body)?,{})?;",
            q(id)
        )
        .unwrap();
    } else {
        out.push_str("let mut item=Map::new();\n");
        if has_hash {
            let mut keep = Vec::new();
            for path in k.paths.values() {
                match path {
                    Path::Key => {}
                    Path::Value => keep.push("&[][..]".to_owned()),
                    Path::Members(path) => keep.push(format!("&{}[..]", path_expr(path))),
                }
            }
            writeln!(
                out,
                "no_loose_except(&body,&[{}],{})?;",
                keep.join(","),
                q(id)
            )
            .unwrap();
        }
        for (member, path) in &k.paths {
            match path {
                Path::Key => {}
                Path::Value => {
                    writeln!(out, "item.insert({}.into(),body.clone());", q(member)).unwrap();
                }
                Path::Members(path) => {
                    writeln!(
                        out,
                        "if let Some(v)=get_path(&body,{}){{item.insert({}.into(),v);}}",
                        path_expr(path),
                        q(member)
                    )
                    .unwrap();
                }
            }
        }
    }
    writeln!(out, "item.insert({}.into(),Value::String(key));", q(key)).unwrap();
    if keyonly {
        out.push_str("items.push(Value::Object(item));\n");
    } else {
        writeln!(out, "items.push(hydrate_{child}(item)?);").unwrap();
    }
    out.push_str("}Ok(Value::Array(items))}\n");
    writeln!(out,"#[allow(non_snake_case)]\nfn key_{parent}(input:Value)->ContractResult<Value>{{let items=input.as_array().ok_or_else(||failure(ErrorReason::InvalidDocument,{}, {}, \"/output\", \"keyed collection requires array\"))?;let mut result=Map::new();for item in items{{let mut item=object(item.clone(),{})?;let key=item.remove({}).and_then(|v|v.as_str().map(str::to_owned)).ok_or_else(||failure(ErrorReason::InvalidDocument,{}, {}, \"/output\", \"key member requires string\"))?;",q(id),q(&k.attribute.name),q(id),q(key),q(id),q(key)).unwrap();
    if keyonly {
        writeln!(out, "let body=encode_{child}(Value::Object(item))?;").unwrap();
    } else {
        out.push_str("let mut body=Value::Null; let mut has_value=false;\n");
        for (member, path) in &k.paths {
            if matches!(path, Path::Key) {
                continue;
            }
            let a = k
                .child
                .attributes
                .iter()
                .find(|a| &a.name == member)
                .unwrap();
            writeln!(out, "if let Some(mut v)=item.remove({}){{", q(member)).unwrap();
            member_emit(out, a, id, names)?;
            if matches!(path, Path::Value) {
                out.push_str("has_value=true;\n");
            }
            let path = match path {
                Path::Value => Vec::new(),
                Path::Members(v) => v.clone(),
                Path::Key => unreachable!(),
            };
            writeln!(
                out,
                "put_path(&mut body,{},v,{})?;}}",
                path_expr(&path),
                q(id)
            )
            .unwrap();
        }
        out.push_str("if !has_value && body.is_null(){body=Value::Object(Map::new());}\n");
    }
    writeln!(out,"if result.insert(key,body).is_some(){{return Err(failure(ErrorReason::InvalidDocument,{}, {}, \"/output\", \"duplicate collection key\"));}}}}Ok(Value::Object(result))}}",q(id),q(&k.attribute.name)).unwrap();
    let _ = bundle;
    Ok(())
}
fn member_emit(
    out: &mut String,
    a: &Attribute,
    id: &str,
    names: &BTreeMap<String, String>,
) -> ContractResult<()> {
    member(out, a, id, names, true)
}
