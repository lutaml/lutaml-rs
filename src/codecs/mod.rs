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
            || !members.insert(rule.to.clone())
            || !model.attributes.iter().any(|a| a.name == rule.to)
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
fn policy(out: &mut String, rule: &KeyRule, a: &Attribute) {
    let default = RenderPolicy {
        nil: RenderAction::Omit,
        empty: RenderAction::Omit,
        default: false.into(),
    };
    let p = rule.render.as_ref().unwrap_or(&default);
    if !matches!(p.default, RenderAction::Boolean(true)) {
        if let DefaultValue::Present(v) = &a.default {
            writeln!(out,"if v==serde_json::from_str::<Value>({}).expect(\"generated literal\") {{ continue; }}",q(&serde_json::to_string(&crate::generator::wire_default(a, v)).unwrap())).unwrap();
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
    match resolved(&p.empty, RenderAction::Blank) {
        RenderAction::Omit => out.push_str("if empty(&v){continue;}\n"),
        RenderAction::Nil | RenderAction::AsNil => out.push_str("if empty(&v){v=Value::Null;}\n"),
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
            writeln!(out,"#[allow(non_snake_case)]\npub fn read_{name}_json(input:&str)->ContractResult<{name}>{{{ordered}let data=decode_{name}(parse_json(input,{})?)?;{strict} let model:{name}=serde_json::from_value(data).map_err(|e|failure(ErrorReason::InvalidDocument,{}, \"\", \"/input\",e))?; {eager_choices}model.validate_shape().map_err(|e|failure(ErrorReason::InvalidDocument,e.model_id,e.attribute,\"/input\",e.reason))?;Ok(model)}}",q(id),q(id)).unwrap();
            writeln!(out,"#[allow(non_snake_case)]\npub fn write_{name}_json(model:&{name})->ContractResult<String>{{{ordered}model.validate_shape().map_err(|e|failure(ErrorReason::InvalidDocument,e.model_id,e.attribute,\"/output\",e.reason))?;let data=serde_json::to_value(model).map_err(|e|failure(ErrorReason::InvalidDocument,{}, \"\", \"/output\",e))?;serde_json::to_string(&encode_{name}(data)?).map_err(|e|failure(ErrorReason::InvalidDocument,{}, \"\", \"/output\",e))}}",q(id),q(id)).unwrap();
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
                for rule in &rules {
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
            if keyed.as_ref().is_some_and(|k| k.root) {
                let k = keyed.as_ref().unwrap();
                writeln!(out,"if let Some(v)=data.remove({}){{return key_{name}(v);}}Ok(Value::Object(wire))}}",q(&k.attribute.name)).unwrap();
            } else {
                for rule in &rules {
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
