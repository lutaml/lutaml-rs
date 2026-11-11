//! Per-model XML reader generation, plus the shared mapping plan the writer
//! emitter consumes. The plan is built from the 09a projection, so rule
//! validation, aliases, namespaces and render policies are lowered once.
use super::contract::{Field, Operation};
use super::projection::compile_projection;
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

#[derive(Clone, PartialEq, Eq)]
pub(super) enum Kind {
    Str,
    Bool,
    Int,
    Decimal,
    Float,
    QName,
    Bound(String),
}

impl Kind {
    fn path(&self) -> String {
        let n = match self {
            Self::Str => "Str",
            Self::Bool => "Bool",
            Self::Int => "Int",
            Self::Decimal => "Decimal",
            Self::Float => "Float",
            Self::QName => "QName",
            Self::Bound(_) => unreachable!("bound scalars have separate calls"),
        };
        format!("rt::Kind::{n}")
    }

    fn decode(&self, ord: &str, text: &str) -> String {
        match self {
            Self::Bound(module) => format!("rt::decode_bound({text}, &at, {module}::parse)?"),
            _ => format!("rt::decode(doc, {ord}, {}, {text}, &at)?", self.path()),
        }
    }

    pub(super) fn encode(&self, id: &str, member: &str) -> String {
        match self {
            Self::Bound(module) => {
                format!("rt::encode_bound(x, {id}, {member}, {module}::format)?")
            }
            _ => format!("rt::encode(x, {}, {id}, {member})?", self.path()),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Pol {
    Omit,
    AsNil,
    AsEmpty,
    AsBlank,
}

impl Pol {
    pub(super) fn path(self) -> &'static str {
        match self {
            Self::Omit => "rt::Policy::Omit",
            Self::AsNil => "rt::Policy::AsNil",
            Self::AsEmpty => "rt::Policy::AsEmpty",
            Self::AsBlank => "rt::Policy::AsBlank",
        }
    }
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
    pub member: String,
    pub ident: String,
    pub names: Vec<String>,
    pub accept: Vec<String>,
    pub out: Ns,
    pub collection: bool,
    pub absent_list: bool,
    pub skip_empty_collection: bool,
    pub child: Child,
    pub cdata: bool,
    pub nil: Pol,
    pub empty: Pol,
}

pub(super) struct ContentPlan {
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
    pub simple_sequence: bool,
    pub initialize_missing_sequence_collections: bool,
    pub attrs: Vec<AttrPlan>,
    pub elems: Vec<ElemPlan>,
    pub content: Option<ContentPlan>,
    /// The `any_attribute` target: `(member, ident)`.
    pub any: Option<(String, String)>,
    /// Reasons the writer refuses this mapping by name.
    pub writer_refusals: Vec<String>,
}

fn scalar_kind(
    bundle: &Bundle,
    bindings: &TypeBindings,
    id: &str,
    attribute: &Attribute,
) -> ContractResult<Kind> {
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
        TypeRef::Builtin { name } => {
            if let Some(codec) = crate::generator::builtin_binding(name, bindings)
                .and_then(TypeBinding::xml_codec_module)
            {
                return Ok(Kind::Bound(codec.into()));
            }
            base(name)
        }
        TypeRef::Value { id: vt } => {
            if let Some(codec) = bindings.get(vt).and_then(TypeBinding::xml_codec_module) {
                return Ok(Kind::Bound(codec.into()));
            }
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
            RenderAction::Other(name) if name == "as_empty" => Pol::AsEmpty,
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

fn flat_sequence(mapping: &XmlMapping, ordered: bool, imported_fields: bool) -> bool {
    let Some([group]) = mapping.sequence.as_deref() else {
        return false;
    };
    if group.kind != XmlRuleKind::Sequence
        || group.min.is_some()
        || group.max.is_some()
        || mapping
            .rules
            .iter()
            .any(|r| matches!(r.kind, XmlRuleKind::Sequence | XmlRuleKind::Choice))
    {
        return false;
    }
    let Some(members) = &group.members else {
        return false;
    };
    if members.is_empty()
        || members
            .iter()
            .any(|member| member.kind != XmlRuleKind::Element || member.name.is_none())
    {
        return false;
    }
    let mut elements = mapping
        .rules
        .iter()
        .filter(|rule| rule.kind == XmlRuleKind::Element)
        .peekable();
    if !imported_fields {
        return ordered || elements.peek().is_none() || elements.eq(members);
    }
    // Imported field mappings can appear only in the expanded sequence.
    // Direct mappings must retain their relative order within that sequence.
    let mut remaining = members.iter();
    ordered || elements.all(|element| remaining.any(|member| member == element))
}

pub(super) fn plan(
    bundle: &Bundle,
    names: &BTreeMap<String, String>,
    bindings: &TypeBindings,
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
        simple_sequence: flat_sequence(mapping, model.ordered_content.is_some(), true),
        initialize_missing_sequence_collections: flat_sequence(mapping, model.ordered_content.is_some(), false),
        attrs: Vec::new(),
        elems: Vec::new(),
        content: None,
        any: None,
        writer_refusals: Vec::new(),
    };
    for op in &projection.operations {
        match op {
            Operation::Sequence(_) if out.simple_sequence => {}
            Operation::Sequence(_) | Operation::Choice(_) => out
                .writer_refusals
                .push("the sequence compositor is not written by this emitter".into()),
            Operation::Field(field) => {
                field_plan(bundle, names, bindings, id, &own, field, &mut out)?
            }
            _ => {}
        }
    }
    Ok(out)
}

fn field_plan(
    bundle: &Bundle,
    names: &BTreeMap<String, String>,
    bindings: &TypeBindings,
    id: &str,
    own: &Ns,
    field: &Field,
    out: &mut Plan,
) -> ContractResult<()> {
    let collection = matches!(field.attribute.collection, Collection::Bounded { .. });
    let ident = member_identifier(&field.member);
    let first = field.aliases.first().cloned().unwrap_or_default();
    let (nil, empty) = policy(id, field)?;
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
                member: field.member.clone(),
                ident,
                collection,
                cdata: field.cdata,
                kind: scalar_kind(bundle, bindings, id, &field.attribute)?,
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
            let kind = scalar_kind(bundle, bindings, id, &field.attribute)?;
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
                    Child::Scalar(scalar_kind(bundle, bindings, id, attribute)?),
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
            // Match native flat-sequence occurrence semantics for repeatable members.
            let repeated_sequence =
                out.initialize_missing_sequence_collections && collection && nil == Pol::Omit && empty == Pol::AsEmpty;
            out.elems.push(ElemPlan {
                member: field.member.clone(),
                ident,
                names: field.aliases.clone(),
                accept,
                out: outns,
                collection,
                absent_list: repeated_sequence
                    && matches!(field.attribute.default, DefaultValue::Absent),
                child,
                cdata: field.cdata,
                nil,
                empty,
                skip_empty_collection: repeated_sequence,
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
                "rt::ElemRule {{ to: {}, names: {}, ns: {}, collection: {}, nullable: {} }}, ",
                q(&e.member),
                str_list(&e.names),
                str_list(&e.accept),
                e.collection,
                nullable(&e.child)
            )
        })
        .collect();
    let content = p.content.as_ref().map_or("None".into(), |c| {
        format!(
            "Some(rt::ContentRule {{ to: {}, collection: {} }})",
            q(&c.member),
            c.collection
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
        "fn read_{name}_el(doc: &rt::Doc, ord: usize, loc: &str) -> ContractResult<Box<{name}>> {{\n let s = rt::scan(doc, ord, &RS_{name}, loc)?;\n let mut m = Box::new({name}::default());"
    )
    .unwrap();
    for (i, a) in p.attrs.iter().enumerate() {
        let first = q(&a.names[0]);
        let value = present(&a.kind.decode("ord", "raw"));
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
                    k.decode("hit.ord", "&doc.text_of(hit.ord)"),
                    true,
                ),
                Child::Model(n) => (
                    String::new(),
                    format!("read_{n}_el(doc, hit.ord, &hit.loc)?"),
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
                            "Some({}) => {union}::Case{index}(read_{case}_el(doc, hit.ord, &hit.loc)?), ",
                            q(tag)
                        );
                    }
                    // An absent tag reads as the base type, which the union can only
                    // hold through the case that maps to it.
                    if let Some(index) = cases.iter().position(|(_, case)| case == base) {
                        arms += &format!(
                            "None => {union}::Case{index}(read_{base}_el(doc, hit.ord, &hit.loc)?), "
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
            let body = match (e.collection, optional) {
                (false, true) => format!("m.{} = {};", e.ident, present(&value)),
                (false, false) => format!("m.{} = Presence::Value({value});", e.ident),
                (true, true) => format!("if let Some(v) = {value} {{ {} }}", push(&e.ident, "v")),
                (true, false) => format!("let v = {value}; {}", push(&e.ident, "v")),
            };
            writeln!(out, "   {i} => {{ {prelude} {body} }}").unwrap();
        }
        out.push_str("   _ => {}\n  }\n }\n");
    }
    for e in p.elems.iter().filter(|e| e.absent_list) {
        let field = &e.ident;
        writeln!(out, " if matches!(m.{field}, Presence::Absent) {{ m.{field} = Presence::Value(Vec::new()); }}").unwrap();
    }
    if let Some(c) = &p.content {
        let at = format!(
            "let at = rt::Where {{ id: {id}, attribute: {}, rule: {}, loc }};",
            q(&c.member),
            q(&c.member)
        );
        if c.collection {
            let decode = c.kind.decode("ord", "t");
            writeln!(out, " if !s.content_list.is_empty() {{ {at} let mut list = Vec::new(); for t in &s.content_list {{ if let Some(v) = {decode} {{ list.push(v); }} }} m.{} = Presence::Value(list); }}", c.ident).unwrap();
        } else {
            let value = present(&c.kind.decode("ord", "&s.content_text"));
            writeln!(out, " {{ {at} m.{} = {value}; }}", c.ident).unwrap();
        }
    }
    if let Some((_, ident)) = &p.any {
        writeln!(out, " if !s.any.is_empty() {{ m.{ident} = Presence::Value(NeutralHash::from_string_pairs(s.any)); }}").unwrap();
    }
    if p.tokens {
        writeln!(out, " m.__lml_content = Presence::Value(model_tokens(&s.tokens));").unwrap();
    }
    out.push_str(" Ok(m)\n}\n");
    let root = p
        .root
        .as_ref()
        .map_or("None".to_string(), |r| format!("Some({})", q(r)));
    let accept = str_list(&accept_list(&[p.own.as_ref().map(|n| n.0.as_str())]));
    writeln!(out,
        "pub fn read_{name}_xml(input: &[u8]) -> ContractResult<{name}> {{\n let doc = rt::parse({id}, input)?;\n doc.check_root({id}, {root}, {accept})?;\n let loc = format!(\"/{{}}\", doc.root_local());\n let m = read_{name}_el(&doc, 0, &loc)?;\n m.validate_shape().map_err(|e| rt::fail(ErrorReason::InvalidDocument, e.model_id, (!e.attribute.is_empty()).then_some(e.attribute), None, \"/input\", e.reason))?;\n Ok(*m)\n}}").unwrap();
}
