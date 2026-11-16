use super::defaults::typed_value;
use super::*;

pub(super) fn bundle(n: &Node) -> ContractResult<()> {
    keys(
        n,
        &[
            "lml_bundle",
            "roots",
            "models",
            "value_types",
            "capabilities",
            "provenance",
        ],
        "",
    )?;
    if field(n, "lml_bundle", "")?.integer() != Some(1) {
        return Err(failure(
            ErrorReason::UnsupportedVersion,
            "/lml_bundle",
            "unsupported bundle version",
        ));
    }
    let roots = field(n, "roots", "")?;
    strings(roots, "/roots", true)?;
    let models = field(n, "models", "")?;
    object(models, "/models")?;
    let values = field(n, "value_types", "")?;
    object(values, "/value_types")?;
    let caps = field(n, "capabilities", "")?;
    strings(caps, "/capabilities", true)?;
    if let Some(p) = n.get("provenance") {
        object(p, "/provenance")?;
    }
    for (id, value) in values.object().unwrap() {
        value_type(value, &pointer("/value_types", id))?;
    }
    for (id, m) in models.object().unwrap() {
        model(m, Some(id), &pointer("/models", id)).map_err(|mut e| {
            e.model_id = Some(id.clone());
            e.klass = m.get("model").and_then(Node::string).map(str::to_owned);
            e
        })?;
    }
    for (id, m) in models.object().unwrap() {
        let at = pointer("/models", id);
        let result = (|| {
            for (i, attr) in m
                .get("attributes")
                .unwrap()
                .array()
                .unwrap()
                .iter()
                .enumerate()
            {
                let loc = format!("{at}/attributes/{i}");
                validate_refs(attr, n, &loc).map_err(|mut e| {
                    e.attribute = attr.get("name").and_then(Node::string).map(str::to_owned);
                    e
                })?;
            }
            for (format, mapping) in m.get("mappings").and_then(Node::object).unwrap_or(&[]) {
                if !caps
                    .array()
                    .unwrap()
                    .iter()
                    .any(|cap| cap.string() == Some(format))
                {
                    return Err(failure(
                        ErrorReason::CapabilityMissing,
                        &pointer(&format!("{at}/mappings"), format),
                        "mapping format absent from capabilities",
                    ));
                }
                for key in ["rules", "sequence"] {
                    if let Some(rules) = mapping.get(key).and_then(Node::array) {
                        for (i, rule) in rules.iter().enumerate() {
                            rule_refs(rule, n, &format!("{at}/mappings/{format}/{key}/{i}"))?;
                        }
                    }
                }
            }
            Ok(())
        })();
        result.map_err(|mut e: Box<crate::contracts::ContractError>| {
            e.model_id = Some(id.clone());
            e.klass = m.get("model").and_then(Node::string).map(str::to_owned);
            e
        })?;
    }
    for (i, root) in roots.array().unwrap().iter().enumerate() {
        target(models, root.string().unwrap(), &format!("/roots/{i}"))?;
    }
    Ok(())
}
pub(super) fn target(models: &Node, id: &str, at: &str) -> ContractResult<()> {
    if models.get(id).is_none() {
        return Err(failure(
            ErrorReason::DanglingReference,
            at,
            format!("unknown target {id}"),
        ));
    }
    Ok(())
}
pub(super) fn discriminator(n: &Node, at: &str) -> ContractResult<()> {
    keys(n, &["attribute", "cases"], at)?;
    text(n, "attribute", at)?;
    let cases = field(n, "cases", at)?;
    if object(cases, &pointer(at, "cases"))?.is_empty() {
        return Err(failure(
            ErrorReason::UnknownDiscriminator,
            at,
            "empty discriminator cases",
        ));
    }
    for (wire, id) in cases.object().unwrap() {
        if wire.is_empty() {
            return Err(failure(
                ErrorReason::UnknownDiscriminator,
                at,
                "empty discriminator wire value",
            ));
        }
        string(id, &pointer(&pointer(at, "cases"), wire))?;
    }
    Ok(())
}
pub(super) fn discriminator_refs(n: &Node, bundle: &Node, at: &str) -> ContractResult<()> {
    discriminator(n, at)?;
    for (wire, id) in n.get("cases").unwrap().object().unwrap() {
        target(
            bundle.get("models").unwrap(),
            id.string().unwrap(),
            &pointer(&pointer(at, "cases"), wire),
        )?;
        let target = bundle
            .get("models")
            .unwrap()
            .get(id.string().unwrap())
            .unwrap();
        let member = n.get("attribute").unwrap().string().unwrap();
        if !target
            .get("attributes")
            .and_then(Node::array)
            .is_some_and(|attrs| {
                attrs
                    .iter()
                    .any(|attr| attr.get("name").and_then(Node::string) == Some(member))
            })
        {
            return Err(failure(
                ErrorReason::UnknownDiscriminator,
                at,
                "discriminator attribute absent on case model",
            ));
        }
    }
    Ok(())
}
pub(super) fn validate_refs(attr: &Node, bundle: &Node, at: &str) -> ContractResult<()> {
    let typ = attr.get("type").unwrap();
    let kind = typ.get("kind").unwrap().string().unwrap();
    if kind == "model" {
        target(
            bundle.get("models").unwrap(),
            typ.get("id").unwrap().string().unwrap(),
            &format!("{at}/type/id"),
        )?;
    }
    if kind == "value" {
        let id = typ.get("id").unwrap().string().unwrap();
        let def = bundle.get("value_types").unwrap().get(id).ok_or_else(|| {
            failure(
                ErrorReason::DanglingReference,
                &format!("{at}/type/id"),
                "unknown value type",
            )
        })?;
        let base = Node::Object(vec![
            ("kind".into(), Node::String("builtin".into())),
            ("name".into(), def.get("base").unwrap().clone()),
        ]);
        if let Some(value) = attr.get("default").and_then(|d| d.get("value")) {
            if let Some(items) = value.array().filter(|_| {
                attr.get("collection")
                    .is_some_and(|v| v != &Node::Bool(false))
            }) {
                for (i, item) in items.iter().enumerate() {
                    typed_value(item, &base, &format!("{at}/default/value/{i}"))?;
                }
            } else {
                typed_value(value, &base, &format!("{at}/default/value"))?;
            }
        }
        if let Some(values) = attr.get("values") {
            for (i, value) in values.array().unwrap().iter().enumerate() {
                super::defaults::enum_value(value, &base, &format!("{at}/values/{i}"))?;
            }
        }
    }
    if let Some(value) = attr.get("default").and_then(|d| d.get("value")) {
        validate_member(value, attr, bundle, &format!("{at}/default/value"))?;
    }
    if let Some(d) = attr.get("discriminator") {
        discriminator_refs(d, bundle, &pointer(at, "discriminator"))?;
    }
    Ok(())
}

fn rule_refs(rule: &Node, bundle: &Node, at: &str) -> ContractResult<()> {
    if let Some(d) = rule.get("discriminator") {
        discriminator_refs(d, bundle, &pointer(at, "discriminator"))?;
    }
    if let Some(members) = rule.get("members").and_then(Node::array) {
        for (i, member) in members.iter().enumerate() {
            rule_refs(member, bundle, &format!("{at}/members/{i}"))?;
        }
    }
    Ok(())
}
