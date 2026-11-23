//! Per-model XML reader generation, plus the shared mapping plan the writer
//! emitter consumes. The plan is built from the 09a projection, so rule
//! validation, aliases, namespaces and render policies are lowered once.
use super::contract::{Field, Operation};
use super::projection::compile_projection;
use super::writer_emitter::{cxa, cxp, gen};
use crate::contracts::*;
use crate::generator::member_identifier;
use std::collections::BTreeMap;
use std::fmt::Write;

pub(super) fn q(s: &str) -> String {
    format!("{s:?}")
}

pub(super) fn refuse(
    id: &str,
    reason: ErrorReason,
    member: &str,
    detail: &str,
) -> Box<ContractError> {
    Box::new(ContractError {
        reason,
        model_id: Some(id.into()),
        attribute: (!member.is_empty()).then(|| member.into()),
        format: Some("xml".into()),
        rule: None,
        location: format!(
            "/models/{}/mappings/xml",
            id.replace('~', "~0").replace('/', "~1")
        ),
        klass: None,
        detail: Some(detail.into()),
    })
}

/// `(uri, prefix)`; None is no namespace.
pub(super) type Ns = Option<(String, Option<String>)>;

fn ns_of(spec: &Namespace) -> Ns {
    match spec {
        Namespace::Uri { uri, prefix } => Some((uri.clone(), prefix.clone())),
        _ => None,
    }
}

fn accept_list(uris: &[Option<&str>]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for u in uris {
        let u = u.unwrap_or("").to_string();
        if !out.contains(&u) {
            out.push(u);
        }
    }
    out
}

pub(super) fn str_list(items: &[String]) -> String {
    format!(
        "&[{}]",
        items.iter().map(|s| q(s)).collect::<Vec<_>>().join(", ")
    )
}

pub(super) fn ns_lit(ns: &Ns) -> String {
    match ns {
        None => "None".into(),
        Some((uri, prefix)) => format!(
            "Some(rt::NsOut {{ uri: {}, prefix: {} }})",
            q(uri),
            prefix
                .as_ref()
                .map_or("None".to_string(), |p| format!("Some({})", q(p)))
        ),
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Kind {
    Str,
    Bool,
    Int,
    Decimal,
    Float,
    QName,
}

impl Kind {
    pub(super) fn path(self) -> String {
        let n = match self {
            Self::Str => "Str",
            Self::Bool => "Bool",
            Self::Int => "Int",
            Self::Decimal => "Decimal",
            Self::Float => "Float",
            Self::QName => "QName",
        };
        format!("rt::Kind::{n}")
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Pol {
    Omit,
    AsNil,
    AsBlank,
}

impl Pol {
    pub(super) fn path(self) -> &'static str {
        match self {
            Self::Omit => "rt::Policy::Omit",
            Self::AsNil => "rt::Policy::AsNil",
            Self::AsBlank => "rt::Policy::AsBlank",
        }
    }
}

/// Handwritten hooks of one rule, and the default-suppression literal of its member.
#[derive(Clone, Default)]
pub(super) struct Hooks {
    pub read: Option<String>,
    pub write: Option<String>,
    /// The rule has `custom` but declares no attribute.
    pub only: bool,
    /// An element rule: its READ hook receives the parsed element, not flattened text.
    pub element: bool,
    /// Declared position among the mapping's rules (write order of hook output).
    pub pos: usize,
    /// JSON literal of the declared default when an applied default may be suppressed
    /// (`render default` is not true).
    pub suppress: Option<String>,
}

pub(super) enum Child {
    Scalar(Kind),
    Model(String),
    Poly {
        base: String,
        /// The generated `<Model>_<member>_Union` holding the field.
        union: String,
        names: Vec<String>,
        accept: Vec<String>,
        attribute: String,
        /// Tag to case model type name.
        cases: Vec<(String, String)>,
    },
}

pub(super) struct AttrPlan {
    pub hooks: Hooks,
    pub member: String,
    pub ident: String,
    pub names: Vec<String>,
    pub accept: Vec<String>,
    pub out: Ns,
    pub kind: Kind,
    pub nil: Pol,
    pub empty: Pol,
}

pub(super) struct ElemPlan {
    pub hooks: Hooks,
    pub member: String,
    pub ident: String,
    pub names: Vec<String>,
    pub accept: Vec<String>,
    pub out: Ns,
    pub collection: bool,
    pub child: Child,
    pub cdata: bool,
    pub nil: Pol,
    pub empty: Pol,
}

pub(super) struct ContentPlan {
    pub hooks: Hooks,
    pub member: String,
    pub ident: String,
    pub collection: bool,
    pub cdata: bool,
    pub kind: Kind,
}

pub(super) struct Plan {
    pub id: String,
    pub name: String,
    pub root: Option<String>,
    pub own: Ns,
    pub ordered: bool,
    pub mixed: bool,
    pub tokens: bool,
    pub attrs: Vec<AttrPlan>,
    pub elems: Vec<ElemPlan>,
    pub content: Option<ContentPlan>,
    /// The bundle has XML hooks: every generated function threads a hook context.
    pub hooked: bool,
    /// The `any_attribute` target: `(member, ident)`.
    pub any: Option<(String, String)>,
    /// Reasons the writer refuses this mapping by name.
    pub writer_refusals: Vec<String>,
}

pub(super) fn bundle_has_xml_hooks(bundle: &Bundle) -> bool {
    bundle.models.values().any(|m| {
        m.mappings
            .xml
            .as_ref()
            .is_some_and(|x| x.rules.iter().any(|r| r.custom.is_some()))
    })
}

fn scalar_kind(bundle: &Bundle, id: &str, attribute: &Attribute) -> ContractResult<Kind> {
    let base = |name: &BuiltinType| -> ContractResult<Kind> {
        Ok(match name {
            BuiltinType::String
            | BuiltinType::Token
            | BuiltinType::Symbol
            | BuiltinType::Uri
            | BuiltinType::Email
            | BuiltinType::Hostname
            | BuiltinType::Ip
            | BuiltinType::Date
            | BuiltinType::Time
            | BuiltinType::DateTime
            | BuiltinType::TimeWithoutDate
            | BuiltinType::Duration
            | BuiltinType::EmailAddress
            | BuiltinType::IpV4Address
            | BuiltinType::IpV6Address
            | BuiltinType::Uuid
            | BuiltinType::Reference
            | BuiltinType::UriReference
            | BuiltinType::DateWithTimezone
            | BuiltinType::DateTimeWithTimezone
            | BuiltinType::DayTimeDuration
            | BuiltinType::YearMonthDuration => Kind::Str,
            BuiltinType::Boolean => Kind::Bool,
            BuiltinType::Integer
            | BuiltinType::PositiveInteger
            | BuiltinType::NonNegativeInteger => Kind::Int,
            BuiltinType::Decimal | BuiltinType::Rational => Kind::Decimal,
            BuiltinType::Float => Kind::Float,
            BuiltinType::Qname => Kind::QName,
            BuiltinType::Hash => {
                return Err(refuse(
                    id,
                    ErrorReason::UnsupportedMapping,
                    &attribute.name,
                    "hash values have no XML text form",
                ))
            }
            BuiltinType::Base64Binary | BuiltinType::HexBinary => {
                return Err(refuse(
                    id,
                    ErrorReason::UnsupportedType,
                    &attribute.name,
                    "binary values need an explicit lexical codec",
                ))
            }
        })
    };
    match &attribute.value_type {
        TypeRef::Builtin { name } => base(name),
        TypeRef::Value { id: vt } => {
            let spec = bundle
                .value_types
                .get(vt)
                .ok_or_else(|| refuse(id, ErrorReason::DanglingReference, &attribute.name, vt))?;
            base(&spec.base)
        }
        TypeRef::Model { .. } => Err(refuse(
            id,
            ErrorReason::UnsupportedMapping,
            &attribute.name,
            "a model-typed attribute cannot be an XML attribute or text",
        )),
    }
}

fn policy(id: &str, field: &Field) -> ContractResult<(Pol, Pol)> {
    let default = RenderPolicy::default();
    let p = field.render.as_ref().unwrap_or(&default);
    if !matches!(p.default, RenderAction::Absent | RenderAction::Boolean(_)) {
        return Err(refuse(
            id,
            ErrorReason::UnsupportedMapping,
            &field.member,
            "render default must be boolean",
        ));
    }
    let lower = |action: &RenderAction, affirmative: Pol| -> ContractResult<Pol> {
        Ok(match action {
            RenderAction::Absent | RenderAction::Boolean(false) | RenderAction::Omit => Pol::Omit,
            RenderAction::Boolean(true) => affirmative,
            RenderAction::Nil | RenderAction::AsNil => Pol::AsNil,
            RenderAction::Blank | RenderAction::AsBlank => Pol::AsBlank,
            RenderAction::Other(name) => {
                return Err(refuse(
                    id,
                    ErrorReason::UnsupportedMapping,
                    &field.member,
                    &format!("render policy {name:?} is not an XML render policy"),
                ))
            }
        })
    };
    Ok((lower(&p.nil, Pol::AsNil)?, lower(&p.empty, Pol::AsBlank)?))
}

fn model_ns(bundle: &Bundle, id: &str, target: &str, member: &str) -> ContractResult<Ns> {
    bundle
        .models
        .get(target)
        .and_then(|m| m.mappings.xml.as_ref())
        .map(|m| ns_of(&m.namespace))
        .ok_or_else(|| {
            refuse(
                id,
                ErrorReason::UnsupportedMapping,
                member,
                &format!("referenced model {target} has no xml mapping"),
            )
        })
}

pub(super) fn plan(
    bundle: &Bundle,
    names: &BTreeMap<String, String>,
    id: &str,
) -> ContractResult<Plan> {
    let model = &bundle.models[id];
    let mapping = model
        .mappings
        .xml
        .as_ref()
        .expect("caller checks the xml mapping");
    let projection = compile_projection(bundle, id)?;
    let own = ns_of(&mapping.namespace);
    let mut out = Plan {
        id: id.into(),
        name: names[id].clone(),
        root: mapping.root.as_ref().map(|r| r.element.clone()),
        own: own.clone(),
        ordered: mapping.ordered == Some(true),
        mixed: mapping.mixed_content == Some(true)
            || mapping.root.as_ref().is_some_and(|r| r.mixed == Some(true)),
        tokens: model.ordered_content.is_some(),
        attrs: Vec::new(),
        elems: Vec::new(),
        content: None,
        hooked: bundle_has_xml_hooks(bundle),
        any: None,
        writer_refusals: Vec::new(),
    };
    let mut pos = 0usize;
    for op in &projection.operations {
        match op {
            Operation::Sequence(_) | Operation::Choice(_) => out
                .writer_refusals
                .push("the sequence compositor is not written by this emitter".into()),
            Operation::Field(field) => {
                field_plan(bundle, names, id, &own, field, pos, &mut out)?;
                pos += 1;
            }
            _ => {}
        }
    }
    Ok(out)
}

fn field_plan(
    bundle: &Bundle,
    names: &BTreeMap<String, String>,
    id: &str,
    own: &Ns,
    field: &Field,
    pos: usize,
    out: &mut Plan,
) -> ContractResult<()> {
    let collection = matches!(field.attribute.collection, Collection::Bounded { .. });
    let only = field.member.is_empty() && field.custom.is_some();
    let ident = if only {
        String::new()
    } else {
        member_identifier(&field.member)
    };
    let first = field.aliases.first().cloned().unwrap_or_default();
    let (nil, empty) = policy(id, field)?;
    for hook in field.custom.iter().flat_map(|c| [&c.to, &c.from]).flatten() {
        if crate::codecs::RUST_KEYWORDS.contains(&hook.as_str()) {
            return Err(refuse(
                id,
                ErrorReason::UnsupportedMapping,
                &field.member,
                &format!("hook name {hook} is a Rust keyword"),
            ));
        }
    }
    let default_true = field
        .render
        .as_ref()
        .is_some_and(|r| matches!(r.default, RenderAction::Boolean(true)));
    let suppress = match (&field.attribute.default, default_true) {
        (DefaultValue::Present(v), false) if !collection => Some(
            serde_json::to_string(&crate::generator::wire_default(&field.attribute, v)).unwrap(),
        ),
        _ => None,
    };
    let hooks = Hooks {
        read: field.custom.as_ref().and_then(|c| c.from.clone()),
        write: field.custom.as_ref().and_then(|c| c.to.clone()),
        only,
        element: field.kind == XmlRuleKind::Element,
        pos,
        suppress,
    };
    match field.kind {
        XmlRuleKind::Content => {
            if out.content.is_some() {
                return Err(refuse(
                    id,
                    ErrorReason::UnsupportedMapping,
                    &field.member,
                    "several content rules",
                ));
            }
            out.content = Some(ContentPlan {
                hooks: hooks.clone(),
                member: field.member.clone(),
                ident,
                collection,
                cdata: field.cdata,
                kind: scalar_kind(bundle, id, &field.attribute)?,
            });
        }
        XmlRuleKind::AnyAttribute => {
            out.any = Some((field.member.clone(), ident));
        }
        XmlRuleKind::Attribute => {
            if collection {
                return Err(refuse(
                    id,
                    ErrorReason::UnsupportedMapping,
                    &field.member,
                    "a collection cannot be one XML attribute",
                ));
            }
            let kind = scalar_kind(bundle, id, &field.attribute)?;
            let (accept, outns) = match &field.namespace {
                Namespace::Uri { uri, prefix } => (
                    accept_list(&[Some(uri)]),
                    Some((uri.clone(), field.prefix.clone().or(prefix.clone()))),
                ),
                _ => (accept_list(&[None]), None),
            };
            if let Some((uri, None)) = &outns {
                out.writer_refusals.push(format!(
                    "attribute {first} is in namespace {uri} but declares no prefix"
                ));
            }
            out.attrs.push(AttrPlan {
                hooks: hooks.clone(),
                member: field.member.clone(),
                ident,
                names: field.aliases.clone(),
                accept,
                out: outns,
                kind,
                nil,
                empty,
            });
        }
        XmlRuleKind::Element => {
            let attribute = &field.attribute;
            let target = attribute.reference.clone().or(match &attribute.value_type {
                TypeRef::Model { id } => Some(id.clone()),
                _ => None,
            });
            let (child, target_ns) = match &target {
                Some(target) => {
                    let tns = model_ns(bundle, id, target, &field.member)?;
                    let base = names.get(target).cloned().ok_or_else(|| {
                        refuse(id, ErrorReason::DanglingReference, &field.member, target)
                    })?;
                    match &field.discriminator {
                        None => (Child::Model(base), tns),
                        Some(d) => {
                            let rule = bundle.models[target]
                                .mappings
                                .xml
                                .as_ref()
                                .and_then(|m| {
                                    m.rules.iter().find(|r| {
                                        r.kind == XmlRuleKind::Attribute && r.to == d.attribute
                                    })
                                })
                                .filter(|r| r.name.is_some())
                                .ok_or_else(|| {
                                    refuse(
                                        id,
                                        ErrorReason::UnsupportedMapping,
                                        &field.member,
                                        &format!(
                                            "discriminator attribute {} has no xml attribute rule on {target}",
                                            d.attribute
                                        ),
                                    )
                                })?;
                            let dns = ns_of(&rule.namespace);
                            let mut cases = Vec::new();
                            for (tag, case) in &d.cases {
                                bundle.models[case].mappings.xml.as_ref().ok_or_else(|| {
                                    refuse(
                                        id,
                                        ErrorReason::UnsupportedMapping,
                                        &field.member,
                                        &format!("discriminator case {case} has no xml mapping"),
                                    )
                                })?;
                                cases.push((tag.clone(), names[case].clone()));
                            }
                            (
                                Child::Poly {
                                    base,
                                    union: format!("{}_{ident}_Union", names[id]),
                                    names: rule.name.clone().unwrap_or_default(),
                                    accept: accept_list(&[dns.as_ref().map(|n| n.0.as_str())]),
                                    attribute: d.attribute.clone(),
                                    cases,
                                },
                                tns,
                            )
                        }
                    }
                }
                None => (
                    Child::Scalar(scalar_kind(bundle, id, attribute)?),
                    own.clone(),
                ),
            };
            let (accept, outns) = match &field.namespace {
                Namespace::Uri { uri, prefix } => (
                    accept_list(&[Some(uri)]),
                    Some((uri.clone(), field.prefix.clone().or(prefix.clone()))),
                ),
                Namespace::Unqualified => (accept_list(&[None]), None),
                Namespace::Inherit => match &target_ns {
                    None => (accept_list(&[None]), None),
                    Some((uri, _)) => (accept_list(&[Some(uri), None]), target_ns.clone()),
                },
            };
            out.elems.push(ElemPlan {
                hooks,
                member: field.member.clone(),
                ident,
                names: field.aliases.clone(),
                accept,
                out: outns,
                collection,
                child,
                cdata: field.cdata,
                nil,
                empty,
            });
        }
        _ => {}
    }
    Ok(())
}

fn nullable(child: &Child) -> bool {
    matches!(child, Child::Scalar(k) if *k != Kind::Str)
}

fn present(expr: &str) -> String {
    format!("match {expr} {{ Some(v) => Presence::Value(v), None => Presence::Null }}")
}

fn push(ident: &str, value: &str) -> String {
    format!("match &mut m.{ident} {{ Presence::Value(list) => list.push({value}), other => *other = Presence::Value(vec![{value}]) }}")
}

/// The reader for one model: a `const` rule table and a typed element reader.
pub(super) fn emit_reader(out: &mut String, p: &Plan) {
    let (id, name) = (q(&p.id), &p.name);
    let (g, cp, ca) = (gen(p.hooked), cxp(p.hooked), cxa(p.hooked));
    emit_hook_trait(out, p);
    let attrs: String = p
        .attrs
        .iter()
        .map(|a| {
            format!(
                "rt::AttrRule {{ to: {}, names: {}, ns: {} }}, ",
                q(&a.member),
                str_list(&a.names),
                str_list(&a.accept)
            )
        })
        .collect();
    let elems: String = p
        .elems
        .iter()
        .map(|e| {
            format!(
                "rt::ElemRule {{ to: {}, names: {}, ns: {}, collection: {}, nullable: {}, scalar: {}, cdata: {} }}, ",
                q(&e.member),
                str_list(&e.names),
                str_list(&e.accept),
                e.collection || e.hooks.only,
                nullable(&e.child),
                matches!(e.child, Child::Scalar(_)),
                e.cdata
            )
        })
        .collect();
    let content = p.content.as_ref().map_or("None".into(), |c| {
        format!(
            "Some(rt::ContentRule {{ to: {}, collection: {}, cdata: {} }})",
            q(&c.member),
            c.collection,
            c.cdata
        )
    });
    writeln!(
        out,
        "const RS_{name}: rt::ModelSpec = rt::ModelSpec {{ id: {id}, ordered: {}, mixed: {}, tokens: {}, attrs: &[{attrs}], elems: &[{elems}], content: {content}, any: {} }};",
        p.ordered, p.mixed, p.tokens, p.any.is_some()
    )
    .unwrap();
    writeln!(
        out,
        "fn read_{name}_el{g}(doc: &rt::Doc, ord: usize, loc: &str{cp}) -> ContractResult<{name}> {{\n let s = rt::scan(doc, ord, &RS_{name}, loc)?;\n let mut m = {name}::default();"
    )
    .unwrap();
    for (i, a) in p.attrs.iter().enumerate() {
        let first = q(&a.names[0]);
        if let Some(call) = read_hook(p, &a.hooks, "raw.as_str()", "&at_loc", &first) {
            writeln!(out,
                " if let Some(raw) = &s.attrs[{i}] {{ let at_loc = format!(\"{{loc}}/@{{}}\", {first}); {call} }}").unwrap();
            continue;
        }
        let value = present(&format!(
            "rt::decode(doc, ord, {}, raw, &at)?",
            a.kind.path()
        ));
        writeln!(out,
            " if let Some(raw) = &s.attrs[{i}] {{ let at_loc = format!(\"{{loc}}/@{{}}\", {first}); let at = rt::Where {{ id: {id}, attribute: {}, rule: {first}, loc: &at_loc }}; m.{} = {value}; }}",
            q(&a.member), a.ident).unwrap();
    }
    if !p.elems.is_empty() {
        out.push_str(" for hit in &s.elems {\n  match hit.rule {\n");
        for (i, e) in p.elems.iter().enumerate() {
            let first = q(&e.names[0]);
            let at = format!(
                "let at = rt::Where {{ id: {id}, attribute: {}, rule: {first}, loc: &hit.loc }};",
                q(&e.member)
            );
            let (prelude, value, optional) = match &e.child {
                Child::Scalar(k) => (
                    at.clone(),
                    format!("rt::decode(doc, hit.ord, {}, &text, &at)?", k.path()),
                    true,
                ),
                Child::Model(n) => (
                    String::new(),
                    format!("Box::new(read_{n}_el(doc, hit.ord, &hit.loc{ca})?)"),
                    false,
                ),
                Child::Poly {
                    base,
                    union,
                    names,
                    accept,
                    attribute,
                    cases,
                } => {
                    let mut arms = String::new();
                    for (index, (tag, case)) in cases.iter().enumerate() {
                        arms += &format!(
                            "Some({}) => {union}::Case{index}(Box::new(read_{case}_el(doc, hit.ord, &hit.loc{ca})?)), ",
                            q(tag)
                        );
                    }
                    // An absent tag reads as the base type, which the union can only
                    // hold through the case that maps to it.
                    if let Some(index) = cases.iter().position(|(_, case)| case == base) {
                        arms += &format!(
                            "None => {union}::Case{index}(Box::new(read_{base}_el(doc, hit.ord, &hit.loc{ca})?)), "
                        );
                    }
                    let dloc = format!(
                        "let dloc = format!(\"{{}}/@{{}}\", hit.loc, {});",
                        q(&names[0])
                    );
                    let _ = attribute;
                    (
                        format!("{at} {dloc}"),
                        format!(
                            "{{ let tag = rt::discriminator(doc, hit.ord, {}, {}); let v: {union} = match tag.as_deref() {{ {arms}other => return Err(rt::unknown_discriminator(&rt::Where {{ id: {id}, attribute: {}, rule: {first}, loc: &dloc }}, other.unwrap_or(\"\"))) }}; v }}",
                            str_list(names),
                            str_list(accept),
                            q(&e.member)
                        ),
                        false,
                    )
                }
            };
            if let Some(call) = read_hook(
                p,
                &e.hooks,
                "&rt::XmlElement::new(doc, hit.ord)",
                "&hit.loc",
                &first,
            ) {
                writeln!(out, "   {i} => {{ {call} }}").unwrap();
                continue;
            }
            let body = match (e.collection, optional) {
                (false, true) => format!("m.{} = {};", e.ident, present(&value)),
                (false, false) => format!("m.{} = Presence::Value({value});", e.ident),
                (true, true) => format!("if let Some(v) = {value} {{ {} }}", push(&e.ident, "v")),
                (true, false) => format!("let v = {value}; {}", push(&e.ident, "v")),
            };
            // A scalar leaf reads through the source's CDATA/ordinary-text rules; nil leaves the member unset.
            let body = if matches!(e.child, Child::Scalar(_)) {
                let nil = if e.collection {
                    "if s.counts[HIT_RULE] > 1 { return Err(rt::nil_item(&at)); }"
                } else {
                    ""
                };
                format!(
                    "match doc.leaf_texts(hit.ord, {}, s.counts[{i}] > 1) {{ Some(items) => {{ for text in items {{ {body} }} }} None => {{ {} }} }}",
                    e.cdata,
                    nil.replace("HIT_RULE", &i.to_string())
                )
            } else {
                body
            };
            writeln!(out, "   {i} => {{ {prelude} {body} }}").unwrap();
        }
        out.push_str("   _ => {}\n  }\n }\n");
    }
    if let Some(c) = p
        .content
        .as_ref()
        .filter(|c| read_hook(p, &c.hooks, "s.content_text.as_str()", "loc", "\"\"").is_some())
    {
        let call = read_hook(p, &c.hooks, "s.content_text.as_str()", "loc", "\"\"").unwrap();
        writeln!(out, " if !s.content_text.is_empty() {{ {call} }}").unwrap();
    } else if let Some(c) = &p.content {
        let at = format!(
            "let at = rt::Where {{ id: {id}, attribute: {}, rule: {}, loc }};",
            q(&c.member),
            q(&c.member)
        );
        if c.collection {
            writeln!(out, " if !s.content_list.is_empty() {{ {at} let mut list = Vec::new(); for t in &s.content_list {{ if let Some(v) = rt::decode(doc, ord, {}, t, &at)? {{ list.push(v); }} }} m.{} = Presence::Value(list); }}", c.kind.path(), c.ident).unwrap();
        } else {
            let value = present(&format!(
                "rt::decode(doc, ord, {}, &s.content_text, &at)?",
                c.kind.path()
            ));
            writeln!(
                out,
                " if !s.content_unset {{ {at} m.{} = {value}; }}",
                c.ident
            )
            .unwrap();
        }
    }
    if let Some((_, ident)) = &p.any {
        writeln!(out, " if !s.any.is_empty() {{ m.{ident} = Presence::Value(NeutralHash::from_string_pairs(s.any)); }}").unwrap();
    }
    if p.tokens {
        writeln!(out, " m.__lml_content = Presence::Value(rt::retype(&s.tokens, &rt::Where {{ id: {id}, attribute: \"__lml_content\", rule: \"\", loc }})?);").unwrap();
    }
    out.push_str(" Ok(m)\n}\n");
    let root = p
        .root
        .as_ref()
        .map_or("None".to_string(), |r| format!("Some({})", q(r)));
    // A namespaced model also accepts a root with no namespace, read as if it applied (Ruby does);
    // any other namespace is refused.
    let own = p.own.as_ref().map(|n| n.0.as_str());
    let accept = str_list(&accept_list(&if own.is_some() {
        vec![own, None]
    } else {
        vec![own]
    }));
    let entry = if p.hooked {
        format!(
            "pub fn read_{name}_xml(input: &[u8]) -> ContractResult<{name}> {{\n read_{name}_xml_with(input, &NoXmlHooks, &mut ())\n}}\npub fn read_{name}_xml_with<S, H: XmlHooks<S>>(input: &[u8], hooks: &H, state: &mut S) -> ContractResult<{name}> {{\n let mut cx = XmlCx {{ hooks, state }};\n let doc = rt::parse({id}, input)?;\n doc.check_root({id}, {root}, {accept})?;\n let loc = format!(\"/{{}}\", doc.root_local());\n let m = read_{name}_el(&doc, 0, &loc, &mut cx)?;\n m.validate().map_err(|e| rt::fail(ErrorReason::InvalidDocument, e.model_id, (!e.attribute.is_empty()).then_some(e.attribute), None, \"/input\", e.reason))?;\n Ok(m)\n}}"
        )
    } else {
        format!(
            "pub fn read_{name}_xml(input: &[u8]) -> ContractResult<{name}> {{\n let doc = rt::parse({id}, input)?;\n doc.check_root({id}, {root}, {accept})?;\n let loc = format!(\"/{{}}\", doc.root_local());\n let m = read_{name}_el(&doc, 0, &loc)?;\n m.validate().map_err(|e| rt::fail(ErrorReason::InvalidDocument, e.model_id, (!e.attribute.is_empty()).then_some(e.attribute), None, \"/input\", e.reason))?;\n Ok(m)\n}}"
        )
    };
    writeln!(out, "{entry}").unwrap();
}

/// The call that hands a scanned value to the rule's handwritten READ hook, or the named failure for a
/// hook-only rule with no read direction. None: an ordinary rule decoded by the generated code.
fn read_hook(p: &Plan, h: &Hooks, value: &str, loc: &str, rule: &str) -> Option<String> {
    let id = q(&p.id);
    match (&h.read, h.only) {
        (Some(hook), _) => Some(format!(
            "<H as {}XmlHooks<S>>::{hook}(cx.hooks, &mut m, {value}, cx.state)?;",
            p.name
        )),
        (None, true) => Some(format!(
            "return Err(rt::fail(ErrorReason::CustomSerialization, {id}, None, Some({rule}), {loc}, \"no read hook is declared for this rule\"));"
        )),
        (None, false) => None,
    }
}

/// The failing-stub trait of one model's XML hooks (READ `(model, value, state)`, WRITE
/// `(model, parent, state)`), declared once per hook name.
fn emit_hook_trait(out: &mut String, p: &Plan) {
    let hooks: Vec<&Hooks> = p
        .attrs
        .iter()
        .map(|a| &a.hooks)
        .chain(p.elems.iter().map(|e| &e.hooks))
        .chain(p.content.iter().map(|c| &c.hooks))
        .collect();
    let mut seen: Vec<&str> = Vec::new();
    let (id, name) = (q(&p.id), &p.name);
    let mut methods = String::new();
    for h in hooks {
        if let Some(hook) = &h.read {
            if !seen.contains(&hook.as_str()) {
                seen.push(hook);
                let value = if h.element {
                    "&rt::XmlElement<'_>"
                } else {
                    "&str"
                };
                writeln!(methods, "/// READ hook (model, parsed value, state): runs only when a read reaches its rule, before validation, with\n/// mutable declared data. An element rule receives the parsed element; attribute and content rules the text.\nfn {hook}(&self, _model: &mut {name}, _value: {value}, _state: &mut S) -> ContractResult<()> {{ Err(rt::fail(ErrorReason::CustomSerialization, {id}, None, Some({}), \"/\", \"the handwritten read hook {hook} is not implemented\")) }}", q(hook)).unwrap();
            }
        }
        if let Some(hook) = &h.write {
            if !seen.contains(&hook.as_str()) {
                seen.push(hook);
                writeln!(methods, "/// WRITE hook (model, parent element, state): writes the rule's whole output into `parent`.\nfn {hook}(&self, _model: &{name}, _parent: &mut rt::XmlParent, _state: &mut S) -> ContractResult<()> {{ Err(rt::fail(ErrorReason::CustomSerialization, {id}, None, Some({}), \"/\", \"the handwritten write hook {hook} is not implemented\")) }}", q(hook)).unwrap();
            }
        }
    }
    if !seen.is_empty() {
        writeln!(out, "pub trait {name}XmlHooks<S> {{\n{methods}}}").unwrap();
    }
}
