//! Pure compile-time lowering of portable XML metadata, independent of XML I/O.
use super::contract::{Field, Operation, Projection};
use crate::contracts::*;
use std::collections::{BTreeMap, BTreeSet};

fn error(id: &str, reason: ErrorReason, location: &str, detail: &str) -> Box<ContractError> {
    Box::new(ContractError {
        reason,
        model_id: Some(id.into()),
        attribute: None,
        format: Some("xml".into()),
        rule: None,
        location: location.into(),
        klass: None,
        detail: Some(detail.into()),
    })
}

fn pointer(value: &str) -> String {
    value.replace('~', "~0").replace('/', "~1")
}

/// XML 1.0 (5th edition) NCName: `Name` without the colon. Checked at generate
/// time so a bad name is a named refusal here, not a NotSerializable failure on
/// the first write.
fn is_ncname(name: &str) -> bool {
    let start = |c: char| {
        matches!(c,
            'A'..='Z' | '_' | 'a'..='z'
            | '\u{C0}'..='\u{D6}' | '\u{D8}'..='\u{F6}' | '\u{F8}'..='\u{2FF}'
            | '\u{370}'..='\u{37D}' | '\u{37F}'..='\u{1FFF}' | '\u{200C}'..='\u{200D}'
            | '\u{2070}'..='\u{218F}' | '\u{2C00}'..='\u{2FEF}' | '\u{3001}'..='\u{D7FF}'
            | '\u{F900}'..='\u{FDCF}' | '\u{FDF0}'..='\u{FFFD}' | '\u{10000}'..='\u{EFFFF}')
    };
    let mut chars = name.chars();
    chars.next().is_some_and(start)
        && chars.all(|c| {
            start(c)
                || matches!(c, '-' | '.' | '0'..='9' | '\u{B7}' | '\u{300}'..='\u{36F}' | '\u{203F}'..='\u{2040}')
        })
}

fn xml_name(id: &str, name: &str, location: &str) -> ContractResult<()> {
    if is_ncname(name) {
        Ok(())
    } else {
        Err(error(
            id,
            ErrorReason::UnsupportedMapping,
            location,
            &format!("invalid XML name {name:?}"),
        ))
    }
}

fn namespace(id: &str, ns: &Namespace, location: &str) -> ContractResult<()> {
    if let Namespace::Uri { uri, prefix } = ns {
        if let Some(prefix) = prefix {
            if !prefix.is_empty() {
                xml_name(id, prefix, &format!("{location}/prefix"))?;
            }
        }
        if uri.is_empty()
            || prefix
                .as_ref()
                .is_some_and(|p| p.is_empty() || p.contains(':'))
        {
            return Err(error(
                id,
                ErrorReason::UnsupportedMapping,
                location,
                "namespace",
            ));
        }
    }
    Ok(())
}

fn discriminator(
    bundle: &Bundle,
    id: &str,
    value: &Discriminator,
    location: &str,
) -> ContractResult<()> {
    if value.attribute.is_empty() || value.cases.is_empty() {
        return Err(error(
            id,
            ErrorReason::UnsupportedMapping,
            location,
            "discriminator",
        ));
    }
    for target in value.cases.values() {
        if !bundle.models.contains_key(target) {
            return Err(error(id, ErrorReason::DanglingReference, location, target));
        }
    }
    Ok(())
}

fn compositor(
    id: &str,
    group: &Compositor,
    members: &BTreeSet<&str>,
    location: &str,
) -> ContractResult<()> {
    let (children, min, max) = match group {
        Compositor::Sequence { members, min, max } | Compositor::Choice { members, min, max } => {
            (members, min, max)
        }
    };
    if children.is_empty() || max.is_some_and(|max| max < *min) {
        return Err(error(
            id,
            ErrorReason::UnsupportedMapping,
            location,
            "compositor bounds/members",
        ));
    }
    for (index, member) in children.iter().enumerate() {
        let path = format!("{location}/members/{index}");
        match member {
            CompositorMember::Attribute(name) if !members.contains(name.as_str()) => {
                return Err(error(id, ErrorReason::UnsupportedMapping, &path, name));
            }
            CompositorMember::Group(group) => compositor(id, group, members, &path)?,
            _ => (),
        }
    }
    Ok(())
}

fn grouped_fields<'a>(
    id: &str,
    groups: &'a [XmlRule],
    rules: &mut Vec<&'a XmlRule>,
    at: &str,
) -> ContractResult<()> {
    for (i, group) in groups.iter().enumerate() {
        let location = format!("{at}/{i}");
        if matches!(group.kind, XmlRuleKind::Sequence | XmlRuleKind::Choice) {
            let members = group.members.as_ref().ok_or_else(|| {
                error(
                    id,
                    ErrorReason::UnsupportedMapping,
                    &location,
                    "group members missing",
                )
            })?;
            grouped_fields(id, members, rules, &format!("{location}/members"))?;
        } else {
            if group.kind != XmlRuleKind::Element {
                return Err(error(
                    id,
                    ErrorReason::UnsupportedMapping,
                    &location,
                    "compositor requires element rules",
                ));
            }
            if let Some(existing) = rules.iter().find(|r| r.to == group.to) {
                // Legacy attribute-name compositors normalize to a reference-only rule.
                if group.name.is_some() && *existing != group {
                    return Err(error(
                        id,
                        ErrorReason::UnsupportedMapping,
                        &location,
                        "conflicting grouped field metadata",
                    ));
                }
            } else if group.name.is_none() {
                return Err(error(
                    id,
                    ErrorReason::UnsupportedMapping,
                    &location,
                    "legacy compositor member is not mapped",
                ));
            } else {
                rules.push(group);
            }
        }
    }
    Ok(())
}
fn lower_group(id: &str, group: &XmlRule, at: &str) -> ContractResult<Compositor> {
    if group.name.is_some()
        || !group.to.is_empty()
        || !group.key.is_empty()
        || group.namespace != Namespace::Inherit
        || group.prefix.is_some()
        || group.cdata.is_some()
        || group.render.is_some()
        || group.discriminator.is_some()
        || group.root_mappings.is_some()
        || group.child_mappings.is_some()
        || group.polymorphic_map.is_some()
    {
        return Err(error(
            id,
            ErrorReason::UnsupportedMapping,
            at,
            "unsupported compositor metadata",
        ));
    }
    let number = |value: &NeutralValue| match value {
        NeutralValue::Unsigned(v) => Some(*v),
        NeutralValue::Signed(v) => u64::try_from(*v).ok(),
        _ => None,
    };
    let min = match &group.min {
        None => 1,
        Some(v) => number(v).ok_or_else(|| {
            error(
                id,
                ErrorReason::UnsupportedMapping,
                at,
                "compositor minimum",
            )
        })?,
    };
    let max = match &group.max {
        None => Some(1),
        Some(NeutralValue::Null) => None,
        Some(v) => Some(number(v).ok_or_else(|| {
            error(
                id,
                ErrorReason::UnsupportedMapping,
                at,
                "compositor maximum",
            )
        })?),
    };
    let members = group
        .members
        .as_ref()
        .ok_or_else(|| {
            error(
                id,
                ErrorReason::UnsupportedMapping,
                at,
                "compositor members",
            )
        })?
        .iter()
        .enumerate()
        .map(|(i, rule)| {
            if matches!(rule.kind, XmlRuleKind::Sequence | XmlRuleKind::Choice) {
                Ok(CompositorMember::Group(Box::new(lower_group(
                    id,
                    rule,
                    &format!("{at}/members/{i}"),
                )?)))
            } else {
                Ok(CompositorMember::Attribute(rule.to.clone()))
            }
        })
        .collect::<ContractResult<Vec<_>>>()?;
    match group.kind {
        XmlRuleKind::Sequence => Ok(Compositor::Sequence { members, min, max }),
        XmlRuleKind::Choice => Ok(Compositor::Choice { members, min, max }),
        _ => Err(error(
            id,
            ErrorReason::UnsupportedMapping,
            at,
            "expected compositor",
        )),
    }
}

/// Compile only the requested model. Bundle loading/graph validation remains the
/// loader's responsibility; referenced discriminator targets are checked here.
pub fn compile_projection(bundle: &Bundle, id: &str) -> ContractResult<Projection> {
    let base = format!("/models/{}", pointer(id));
    let model = bundle
        .models
        .get(id)
        .ok_or_else(|| error(id, ErrorReason::DanglingReference, &base, id))?;
    let path = format!("{base}/mappings/xml");
    let mapping = model
        .mappings
        .xml
        .as_ref()
        .ok_or_else(|| error(id, ErrorReason::UnsupportedFormat, &path, "xml"))?;
    namespace(id, &mapping.namespace, &format!("{path}/namespace"))?;
    if mapping.root_mappings.is_some() || mapping.child_mappings.is_some() {
        return Err(error(
            id,
            ErrorReason::UnsupportedMapping,
            &path,
            "XML keyed mapping metadata is not implemented",
        ));
    }
    if mapping
        .root
        .as_ref()
        .is_some_and(|r| !r.metadata.is_empty())
    {
        return Err(error(
            id,
            ErrorReason::UnsupportedMapping,
            &format!("{path}/root"),
            "unsupported XML root metadata",
        ));
    }
    if (mapping.ordered == Some(true)
        || mapping.mixed_content == Some(true)
        || mapping.root.as_ref().is_some_and(|r| r.mixed == Some(true)))
        && model.ordered_content.is_none()
    {
        return Err(error(
            id,
            ErrorReason::InvalidOrderedContent,
            &path,
            "ordered/mixed flags require portable instance tokens",
        ));
    }

    if mapping.root.as_ref().is_some_and(|r| r.element.is_empty()) {
        return Err(error(
            id,
            ErrorReason::UnsupportedMapping,
            &format!("{path}/root"),
            "root",
        ));
    }
    if let Some(root) = &mapping.root {
        xml_name(id, &root.element, &format!("{path}/root/element"))?;
    }
    let attributes: BTreeMap<_, _> = model
        .attributes
        .iter()
        .map(|a| (a.name.as_str(), a))
        .collect();
    let mut operations = vec![Operation::Root {
        element: mapping.root.as_ref().map(|r| r.element.clone()),
        namespace: mapping.namespace.clone(),
    }];
    let mut element_members = BTreeSet::new();
    let mut mapped = BTreeSet::new();
    let mut rules: Vec<_> = mapping
        .rules
        .iter()
        .filter(|r| !matches!(r.kind, XmlRuleKind::Sequence | XmlRuleKind::Choice))
        .collect();
    let mut groups: Vec<XmlRule> = mapping
        .rules
        .iter()
        .filter(|r| matches!(r.kind, XmlRuleKind::Sequence | XmlRuleKind::Choice))
        .cloned()
        .collect();
    if let Some(sequence) = &mapping.sequence {
        groups.extend(sequence.iter().cloned());
    }
    for (index, rule) in rules.iter().enumerate() {
        if !attributes.contains_key(rule.to.as_str()) {
            return Err(error(
                id,
                ErrorReason::UnsupportedMapping,
                &format!("{path}/rules/{index}/to"),
                &rule.to,
            ));
        }
    }
    let top_level = rules.len();
    grouped_fields(id, &groups, &mut rules, &format!("{path}/sequence"))?;
    let mut any_attribute = false;
    for (index, rule) in rules.iter().enumerate() {
        if !matches!(
            rule.kind,
            XmlRuleKind::Attribute
                | XmlRuleKind::Element
                | XmlRuleKind::Content
                | XmlRuleKind::AnyAttribute
        ) || !rule.key.is_empty()
            || rule.members.is_some()
            || rule.min.is_some()
            || rule.max.is_some()
            || rule.root_mappings.is_some()
            || rule.child_mappings.is_some()
            || rule.polymorphic_map.is_some()
        {
            return Err(error(
                id,
                ErrorReason::UnsupportedMapping,
                &format!("{path}/rules/{index}"),
                "unsupported XML field metadata",
            ));
        }

        let location = format!("{path}/rules/{index}");
        if rule.kind == XmlRuleKind::AnyAttribute {
            let refuse = || {
                error(
                    id,
                    ErrorReason::UnsupportedMapping,
                    &location,
                    "any_attribute needs one bare top-level rule onto a scalar hash attribute",
                )
            };
            let attribute = *attributes.get(rule.to.as_str()).ok_or_else(refuse)?;
            let hash = matches!(
                attribute.value_type,
                TypeRef::Builtin {
                    name: BuiltinType::Hash
                }
            ) && matches!(attribute.collection, Collection::Scalar(_))
                && attribute.reference.is_none();
            let bare = rule.name.is_none()
                && rule.prefix.is_none()
                && rule.cdata.is_none()
                && rule.render.is_none()
                && rule.discriminator.is_none()
                && rule.namespace == Namespace::Inherit;
            if index >= top_level || any_attribute || !hash || !bare {
                return Err(refuse());
            }
            any_attribute = true;
            mapped.insert(rule.to.as_str());
            operations.push(Operation::Field(Box::new(Field {
                member: rule.to.clone(),
                attribute: attribute.clone(),
                value_type: attribute.value_type.clone(),
                kind: XmlRuleKind::AnyAttribute,
                aliases: Vec::new(),
                namespace: Namespace::Inherit,
                prefix: None,
                cdata: false,
                render: None,
                discriminator: None,
            })));
            continue;
        }
        let attribute = attributes.get(rule.to.as_str()).ok_or_else(|| {
            error(
                id,
                ErrorReason::UnsupportedMapping,
                &format!("{location}/to"),
                &rule.to,
            )
        })?;
        if !mapped.insert(rule.to.as_str()) {
            return Err(error(
                id,
                ErrorReason::UnsupportedMapping,
                &location,
                "duplicate member mapping",
            ));
        }
        namespace(id, &rule.namespace, &format!("{location}/namespace"))?;
        let aliases = rule.name.clone().unwrap_or_default();
        if (rule.kind == XmlRuleKind::Content && rule.name.is_some())
            || (rule.kind != XmlRuleKind::Content
                && (aliases.is_empty() || aliases.iter().any(String::is_empty)))
            || aliases.iter().collect::<BTreeSet<_>>().len() != aliases.len()
        {
            return Err(error(
                id,
                ErrorReason::UnsupportedMapping,
                &format!("{location}/name"),
                "name aliases",
            ));
        }
        if rule.kind != XmlRuleKind::Content {
            for (i, alias) in aliases.iter().enumerate() {
                // The `xml` prefix is bound by the XML namespaces spec itself,
                // so an attribute may be spelled `xml:lang` with no declaration.
                let local = if rule.kind == XmlRuleKind::Attribute {
                    alias.strip_prefix("xml:").unwrap_or(alias)
                } else {
                    alias
                };
                xml_name(id, local, &format!("{location}/name/{i}"))?;
            }
        }
        if let Some(prefix) = rule.prefix.as_ref().filter(|p| !p.is_empty()) {
            xml_name(id, prefix, &format!("{location}/prefix"))?;
        }
        if rule
            .prefix
            .as_ref()
            .is_some_and(|p| p.is_empty() || p.contains(':'))
        {
            return Err(error(
                id,
                ErrorReason::UnsupportedMapping,
                &format!("{location}/prefix"),
                "prefix",
            ));
        }
        if rule.kind == XmlRuleKind::Attribute && rule.cdata == Some(true) {
            return Err(error(
                id,
                ErrorReason::UnsupportedMapping,
                &format!("{location}/cdata"),
                "cdata on attribute",
            ));
        }
        if let (Some(a), Some(b)) = (&rule.discriminator, &attribute.discriminator) {
            if a != b {
                return Err(error(
                    id,
                    ErrorReason::UnsupportedMapping,
                    &format!("{location}/discriminator"),
                    "conflicting discriminator",
                ));
            }
        }
        let selected = rule
            .discriminator
            .as_ref()
            .or(attribute.discriminator.as_ref());
        if let Some(value) = selected {
            discriminator(bundle, id, value, &format!("{location}/discriminator"))?;
        }
        if rule.kind == XmlRuleKind::Element {
            element_members.insert(rule.to.as_str());
        }
        operations.push(Operation::Field(Box::new(Field {
            member: rule.to.clone(),
            attribute: (*attribute).clone(),
            value_type: attribute.value_type.clone(),
            kind: rule.kind.clone(),
            aliases,
            namespace: rule.namespace.clone(),
            prefix: rule.prefix.clone(),
            cdata: rule.cdata.unwrap_or(false),
            render: rule.render.clone(),
            discriminator: selected.cloned(),
        })));
    }
    for (index, group) in groups.iter().enumerate() {
        let location = format!("{path}/sequence/{index}");
        let lowered = lower_group(id, group, &location)?;
        compositor(id, &lowered, &element_members, &location)?;
        operations.push(match lowered {
            Compositor::Sequence { .. } => Operation::Sequence(lowered),
            Compositor::Choice { .. } => Operation::Choice(lowered),
        });
    }
    if let Some(order) = &model.ordered_content {
        if attributes.contains_key("__lml_content") {
            return Err(error(
                id,
                ErrorReason::ReservedMember,
                &base,
                "__lml_content",
            ));
        }
        if order.member != "__lml_content" || order.version != 1 {
            return Err(error(
                id,
                ErrorReason::InvalidOrderedContent,
                &format!("{base}/ordered_content"),
                "ordered_content",
            ));
        }
        operations.push(Operation::InstanceOrder {
            member: order.member.clone(),
        });
    }
    Ok(Projection {
        model_id: id.into(),
        operations,
    })
}

/// Validate instance references before an adapter emits anything. Counts are
/// supplied by generated typed field accessors, never inferred from schema data.
/// The returned tokens retain literal text and every interleaving unchanged.
pub fn validate_order<'a>(
    projection: &Projection,
    tokens: &'a [ContentToken],
    counts: &BTreeMap<String, usize>,
) -> ContractResult<&'a [ContentToken]> {
    let id = &projection.model_id;
    let fail = |index, detail: &str| {
        error(
            id,
            ErrorReason::InvalidOrderedContent,
            &format!("/__lml_content/{index}"),
            detail,
        )
    };
    if !projection
        .operations
        .iter()
        .any(|op| matches!(op, Operation::InstanceOrder { .. }))
    {
        return Err(fail(0, "ordered_content is not declared"));
    }
    for op in &projection.operations {
        if let Operation::Field(field) = op {
            if matches!(field.attribute.collection, Collection::Scalar(_))
                && counts.get(&field.member).copied().unwrap_or(0) > 1
            {
                return Err(fail(0, &field.member));
            }
        }
    }
    let mut seen = BTreeSet::new();
    for (position, token) in tokens.iter().enumerate() {
        let (member, index, element) = match token {
            ContentToken::Element { attribute, index } => (attribute, index, true),
            ContentToken::Text {
                source: TextSource::Member { attribute, index },
            }
            | ContentToken::Cdata {
                source: TextSource::Member { attribute, index },
            } => (attribute, index, false),
            _ => continue,
        };
        let field = projection
            .operations
            .iter()
            .find_map(|op| match op {
                Operation::Field(field) if field.member == *member => Some(field),
                _ => None,
            })
            .ok_or_else(|| fail(position, member))?;
        if (element && field.kind != XmlRuleKind::Element)
            || (!element && field.kind != XmlRuleKind::Content)
            || counts
                .get(member)
                .is_none_or(|count| (*index as u128) >= (*count as u128))
            || !seen.insert((member, index))
        {
            return Err(fail(position, member));
        }
    }
    // Dropping a mapped value would silently produce a partial artifact.
    for op in &projection.operations {
        if let Operation::Field(field) = op {
            if field.kind != XmlRuleKind::Attribute {
                for index in 0..counts.get(&field.member).copied().unwrap_or(0) {
                    if !seen.contains(&(&field.member, &(index as u64))) {
                        return Err(fail(tokens.len(), &field.member));
                    }
                }
            }
        }
    }
    Ok(tokens)
}
