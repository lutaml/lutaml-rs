use super::*;
fn mapping_keys(n: &Node, allowed: &[&str], at: &str) -> ContractResult<()> {
    keys(n, allowed, at).map_err(|mut e| {
        if e.reason == ErrorReason::InvalidDocument {
            e.reason = ErrorReason::UnsupportedMapping;
        }
        e
    })
}
pub(super) fn namespace(n: &Node, at: &str) -> ContractResult<()> {
    mapping_keys(n, &["mode", "uri", "prefix"], at)?;
    let mode = text(n, "mode", at)?;
    if !matches!(mode, "inherit" | "unqualified" | "uri") {
        return Err(failure(
            ErrorReason::UnsupportedMapping,
            &pointer(at, "mode"),
            "unknown namespace mode",
        ));
    }
    if let Some(v) = n.get("prefix") {
        prefix(v, &pointer(at, "prefix"))?;
    }
    if mode == "uri" {
        text(n, "uri", at)?;
        Ok(())
    } else if n.get("uri").is_some() || n.get("prefix").is_some() {
        Err(failure(
            ErrorReason::UnsupportedMapping,
            at,
            "uri/prefix only with mode uri",
        ))
    } else {
        Ok(())
    }
}
/// XML prefix lexical rule: `\A[A-Za-z_][A-Za-z0-9_.-]*\z` (Ruby XML_PREFIX).
fn prefix(n: &Node, at: &str) -> ContractResult<()> {
    let ok = n.string().is_some_and(|s| {
        let mut chars = s.chars();
        chars
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
            && chars.all(|c| c.is_ascii_alphanumeric() || "_.-".contains(c))
    });
    if ok {
        Ok(())
    } else {
        Err(invalid(
            at,
            "prefix must match \\A[A-Za-z_][A-Za-z0-9_.-]*\\z",
        ))
    }
}
/// Root metadata scalars: object, non-empty `element`, boolean `mixed`.
fn root(n: &Node, at: &str) -> ContractResult<()> {
    object(n, at)?;
    if let Some(element) = n.get("element") {
        string(element, &pointer(at, "element"))?;
    }
    if let Some(mixed) = n.get("mixed") {
        boolean(mixed, &pointer(at, "mixed")).map_err(|mut e| {
            e.reason = ErrorReason::UnsupportedMapping;
            e
        })?;
    }
    Ok(())
}
pub(super) fn render(n: &Node, at: &str) -> ContractResult<()> {
    mapping_keys(n, &["nil", "empty", "default"], at)?;
    for (key, value) in object(n, at)? {
        if !matches!(value, Node::Bool(_) | Node::String(_)) {
            return Err(failure(
                ErrorReason::UnsupportedMapping,
                &pointer(at, key),
                "render policy must be a string or boolean",
            ));
        }
    }
    Ok(())
}
fn flags(n: &Node, at: &str) -> ContractResult<()> {
    for key in ["ordered", "mixed_content"] {
        if let Some(value) = n.get(key) {
            boolean(value, &pointer(at, key)).map_err(|mut e| {
                e.reason = ErrorReason::UnsupportedMapping;
                e
            })?;
        }
    }
    if let Some(ns) = n.get("namespace") {
        namespace(ns, &pointer(at, "namespace"))?;
    }
    Ok(())
}
fn compositor(n: &Node, names: &BTreeSet<&str>, at: &str, xml: bool) -> ContractResult<()> {
    rule(n, names, at, xml)?;
    if !matches!(
        n.get("kind").and_then(Node::string),
        Some("sequence" | "choice")
    ) {
        return Err(failure(
            ErrorReason::UnsupportedMapping,
            &pointer(at, "kind"),
            "expected sequence or choice",
        ));
    }
    for (i, member) in array(field(n, "members", at)?, &pointer(at, "members"))?
        .iter()
        .enumerate()
    {
        let loc = format!("{at}/members/{i}");
        object(member, &loc)?;
        if member.get("kind").and_then(Node::string) == Some("any_attribute") {
            return Err(failure(
                ErrorReason::UnsupportedMapping,
                &pointer(&loc, "kind"),
                "any_attribute is not allowed inside a compositor",
            ));
        }
        if matches!(
            member.get("kind").and_then(Node::string),
            Some("sequence" | "choice")
        ) {
            compositor(member, names, &loc, xml)?;
        } else {
            rule(member, names, &loc, xml)?;
        }
    }
    Ok(())
}
fn rule(n: &Node, names: &BTreeSet<&str>, at: &str, xml: bool) -> ContractResult<()> {
    mapping_keys(
        n,
        &[
            "kind",
            "name",
            "key",
            "to",
            "namespace",
            "prefix",
            "cdata",
            "render",
            "discriminator",
            "members",
            "min",
            "max",
            "child_mappings",
            "root_mappings",
            "polymorphic_map",
            "custom",
        ],
        at,
    )?;
    if xml {
        if let Some(kind) = n.get("kind") {
            if ![
                "attribute",
                "element",
                "content",
                "any_attribute",
                "sequence",
                "choice",
            ]
            .contains(&string(kind, &pointer(at, "kind"))?)
            {
                return Err(failure(
                    ErrorReason::UnsupportedMapping,
                    &pointer(at, "kind"),
                    "unknown XML rule kind",
                ));
            }
        }
    }
    if let Some(to) = n.get("to") {
        let name = string(to, &pointer(at, "to"))?;
        if !names.contains(name) {
            return Err(failure(
                ErrorReason::DanglingReference,
                &pointer(at, "to"),
                "mapping target attribute missing",
            ));
        }
    }
    for key in ["name", "key"] {
        if let Some(value) = n.get(key) {
            strings(value, &pointer(at, key), false)?;
        }
    }
    if let Some(ns) = n.get("namespace") {
        namespace(ns, &pointer(at, "namespace"))?;
    }
    if let Some(value) = n.get("cdata") {
        boolean(value, &pointer(at, "cdata"))?;
    }
    if let Some(policy) = n.get("render") {
        render(policy, &pointer(at, "render"))?;
    }
    if let Some(d) = n.get("discriminator") {
        discriminator(d, &pointer(at, "discriminator"))?;
    }
    if let Some(c) = n.get("custom") {
        let kind = xml.then(|| n.get("kind").and_then(Node::string).unwrap_or(""));
        custom(c, &pointer(at, "custom"), kind)?;
    }
    if let Some(map) = n.get("polymorphic_map") {
        for (key, value) in object(map, &pointer(at, "polymorphic_map"))? {
            string(value, &pointer(&pointer(at, "polymorphic_map"), key))?;
        }
    }
    for key in ["child_mappings", "root_mappings"] {
        if let Some(value) = n.get(key) {
            object(value, &pointer(at, key))?;
        }
    }
    Ok(())
}
fn hook_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
        && !name.starts_with("__lml_absent_")
}
/// Names (never bodies) of handwritten serialization hooks on a key-value rule, or on an XML
/// `content`/`element`/`attribute` rule (never `any_attribute`, `sequence` or `choice`).
fn custom(n: &Node, at: &str, xml: Option<&str>) -> ContractResult<()> {
    if let Some(kind) = xml {
        if !["content", "element", "attribute"].contains(&kind) {
            return Err(failure(
                ErrorReason::UnsupportedMapping,
                at,
                "custom serialization hooks apply to xml content, element and attribute rules only",
            ));
        }
    }
    let entries = match n.object() {
        Some(entries) if !entries.is_empty() => entries,
        _ => {
            return Err(invalid(
                at,
                "custom must be a non-empty object of hook names",
            ))
        }
    };
    for (key, _) in entries {
        if key != "to" && key != "from" {
            return Err(invalid(
                &pointer(at, key),
                format!("unknown custom key {key}"),
            ));
        }
    }
    for key in ["to", "from"] {
        if let Some(value) = n.get(key) {
            if !value.string().is_some_and(hook_name) {
                return Err(invalid(
                    &pointer(at, key),
                    format!("custom {key} must match [A-Za-z_][A-Za-z0-9_]* (not __lml_absent_*)"),
                ));
            }
        }
    }
    if let (Some(to), Some(from)) = (n.get("to"), n.get("from")) {
        if to == from {
            return Err(invalid(at, "custom to and from must differ"));
        }
    }
    Ok(())
}
/// A hook name has one direction AND one signature kind (key-value or xml) per model, across every
/// rule and format.
pub(super) fn hook_directions(mappings: &Node, at: &str) -> ContractResult<()> {
    let mut seen: std::collections::BTreeMap<&str, (&str, bool)> =
        std::collections::BTreeMap::new();
    for (format, mapping) in mappings.object().unwrap_or(&[]) {
        let Some(rules) = mapping.get("rules").and_then(Node::array) else {
            continue;
        };
        let is_xml = format == "xml";
        for (i, rule) in rules.iter().enumerate() {
            for direction in ["to", "from"] {
                let Some(name) = rule
                    .get("custom")
                    .and_then(|c| c.get(direction))
                    .and_then(Node::string)
                else {
                    continue;
                };
                if seen
                    .insert(name, (direction, is_xml))
                    .is_some_and(|before| before != (direction, is_xml))
                {
                    let loc = format!(
                        "{}/rules/{i}/custom/{direction}",
                        pointer(&pointer(at, "mappings"), format)
                    );
                    return Err(invalid(
                        &loc,
                        format!("hook {name} is used with another direction or signature kind"),
                    ));
                }
            }
        }
    }
    Ok(())
}
pub(super) fn xml(
    n: &Node,
    names: &BTreeSet<&str>,
    hashes: &BTreeSet<&str>,
    at: &str,
) -> ContractResult<()> {
    mapping_keys(
        n,
        &[
            "root",
            "namespace",
            "rules",
            "root_mappings",
            "child_mappings",
            "ordered",
            "mixed_content",
            "sequence",
        ],
        at,
    )?;
    flags(n, at)?;
    // Root metadata (including mixed) is kept by the typed root instead of
    // throwing away fields the Ruby validator deliberately leaves declarative.
    if let Some(value) = n.get("root") {
        root(value, &pointer(at, "root"))?;
    }
    if let Some(rules) = n.get("rules") {
        for (i, r) in array(rules, &pointer(at, "rules"))?.iter().enumerate() {
            let loc = format!("{at}/rules/{i}");
            rule(r, names, &loc, true).map_err(|mut e| {
                e.rule = Some(i.to_string());
                e.attribute = r.get("to").and_then(Node::string).map(str::to_owned);
                e
            })?;
        }
    }
    if let Some(rules) = n.get("rules") {
        any_attribute_rules(rules.array().unwrap_or(&[]), hashes, at, true)?;
        let ordered = n.get("ordered") == Some(&Node::Bool(true))
            || n.get("mixed_content") == Some(&Node::Bool(true))
            || n.get("root").and_then(|r| r.get("mixed")) == Some(&Node::Bool(true));
        if ordered {
            for (i, r) in rules.array().unwrap_or(&[]).iter().enumerate() {
                if r.get("custom").is_some() {
                    return Err(failure(
                        ErrorReason::ImperativeElementOrder,
                        &format!("{at}/rules/{i}/custom"),
                        "an ordered or mixed mapping cannot be driven by a callback",
                    ));
                }
            }
        }
    }
    // Ruby's only compositor shape: an array of sequence/choice groups whose
    // members are rule objects or nested groups (no `choice` key, no bare
    // compositor object, no attribute-name members).
    if let Some(groups) = n.get("sequence") {
        for (i, group) in array(groups, &pointer(at, "sequence"))?.iter().enumerate() {
            compositor(group, names, &format!("{at}/sequence/{i}"), true)?;
        }
    }
    for key in ["root_mappings", "child_mappings"] {
        if let Some(value) = n.get(key) {
            object(value, &pointer(at, key))?;
        }
    }
    Ok(())
}
pub(super) fn key_mapping(n: &Node, names: &BTreeSet<&str>, at: &str) -> ContractResult<()> {
    mapping_keys(
        n,
        &[
            "root",
            "namespace",
            "rules",
            "root_mappings",
            "child_mappings",
            "ordered",
            "mixed_content",
            "sequence",
        ],
        at,
    )?;
    flags(n, at)?;
    if let Some(value) = n.get("root") {
        root(value, &pointer(at, "root"))?;
    }
    if n.get("sequence").is_some() {
        return Err(failure(
            ErrorReason::UnsupportedMapping,
            &pointer(at, "sequence"),
            "sequence is xml only",
        ));
    }
    for key in ["root_mappings", "child_mappings"] {
        if let Some(value) = n.get(key) {
            object(value, &pointer(at, key))?;
        }
    }
    if let Some(rules) = n.get("rules") {
        for (i, r) in array(rules, &pointer(at, "rules"))?.iter().enumerate() {
            let loc = format!("{at}/rules/{i}");
            rule(r, names, &loc, false).map_err(|mut e| {
                e.rule = Some(i.to_string());
                e.attribute = r.get("to").and_then(Node::string).map(str::to_owned);
                e
            })?;
        }
        any_attribute_rules(rules.array().unwrap_or(&[]), &BTreeSet::new(), at, false)?;
    }
    Ok(())
}
/// `any_attribute` (Ruby `map_all_attributes`): xml only, top level only, at
/// most one per mapping, exactly `kind` and `to`, `to` a declared
/// non-collection builtin `hash` attribute (mirrors Ruby `any_attribute_rules`).
fn any_attribute_rules(
    rules: &[Node],
    hashes: &BTreeSet<&str>,
    at: &str,
    xml: bool,
) -> ContractResult<()> {
    let found: Vec<usize> = rules
        .iter()
        .enumerate()
        .filter(|(_, r)| r.get("kind").and_then(Node::string) == Some("any_attribute"))
        .map(|(i, _)| i)
        .collect();
    let Some(&first) = found.first() else {
        return Ok(());
    };
    let rule_at = |i: usize| format!("{at}/rules/{i}");
    if !xml {
        return Err(failure(
            ErrorReason::UnsupportedMapping,
            &rule_at(first),
            "any_attribute is xml only",
        ));
    }
    if let Some(&second) = found.get(1) {
        return Err(failure(
            ErrorReason::UnsupportedMapping,
            &rule_at(second),
            "at most one any_attribute rule per mapping",
        ));
    }
    let loc = rule_at(first);
    mapping_keys(&rules[first], &["kind", "to"], &loc)?;
    let to = string(field(&rules[first], "to", &loc)?, &pointer(&loc, "to"))?;
    if !hashes.contains(to) {
        return Err(failure(
            ErrorReason::UnsupportedMapping,
            &pointer(&loc, "to"),
            "any_attribute `to` must be a non-collection hash attribute",
        ));
    }
    Ok(())
}
