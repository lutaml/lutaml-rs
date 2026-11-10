//! Native XML binding over the pinned moxml-rs `document` faces.
//!
//! Generated, model-specific codecs (see `reader_emitter` / `writer_emitter`)
//! call only the static helpers below. Nothing here interprets a schema: the
//! generated code passes `const` rule tables and typed closures-by-code, never
//! schema-provided code. The helpers read one batch (`Document::element_rows`)
//! and write one batch (`ElementSpec` + `Document::build`).
//!
//! Faces used: `Document::parse_with` (policy), `element_rows`, `ElementSpec`,
//! `Document::build`, `to_xml`. The reference engine is pure Rust; the binding
//! is engine-agnostic because it only touches `moxml::document`.
use crate::contracts::{ContentToken, ContractError, ContractResult, ErrorReason, TextSource};
pub use moxml;
use moxml::document::{
    Attribute, ContentRow, ContentSpec, Document, ElementRow, ElementSpec, ParserPolicy, QName,
    XML_NS,
};
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::HashMap;

/// Same limit as the TS runtime: larger inputs never reach the parser.
pub const MAX_INPUT: usize = 16 * 1024 * 1024;
pub const XSI: &str = "http://www.w3.org/2001/XMLSchema-instance";

/// Structured, never-partial failure. `format` is always `xml`.
pub fn fail(
    reason: ErrorReason,
    id: &str,
    attribute: Option<&str>,
    rule: Option<&str>,
    location: &str,
    detail: impl ToString,
) -> Box<ContractError> {
    Box::new(ContractError {
        reason,
        model_id: Some(id.into()),
        attribute: attribute.map(Into::into),
        format: Some("xml".into()),
        rule: rule.map(Into::into),
        location: location.into(),
        klass: None,
        detail: Some(detail.to_string()),
    })
}

/// Where a scalar or discriminator failure is reported.
#[derive(Clone, Copy)]
pub struct Where<'a> {
    pub id: &'a str,
    pub attribute: &'a str,
    pub rule: &'a str,
    pub loc: &'a str,
}

impl Where<'_> {
    fn fail(&self, reason: ErrorReason, detail: impl ToString) -> Box<ContractError> {
        fail(
            reason,
            self.id,
            Some(self.attribute),
            Some(self.rule),
            self.loc,
            detail,
        )
    }
}

// ------------------------------------------------------------------ reading

/// A parsed document as batch rows (document order, root = ordinal 0).
pub struct Doc {
    rows: Vec<ElementRow>,
}

/// Parses under the safe default policy: DOCTYPE is refused, external entities
/// are never resolved, nesting is bounded.
pub fn parse(id: &str, input: &[u8]) -> ContractResult<Doc> {
    parse_with(id, input, &ParserPolicy::default())
}

pub fn parse_with(id: &str, input: &[u8], policy: &ParserPolicy) -> ContractResult<Doc> {
    if input.len() > MAX_INPUT {
        return Err(fail(
            ErrorReason::InvalidDocument,
            id,
            None,
            None,
            "/input",
            format!("input exceeds {MAX_INPUT} bytes"),
        ));
    }
    let doc = Document::parse_with(input, policy).map_err(|e| {
        fail(
            ErrorReason::InvalidDocument,
            id,
            None,
            None,
            "/input",
            e.to_string(),
        )
    })?;
    Ok(Doc {
        rows: doc.element_rows(),
    })
}

fn ns_of(row: &ElementRow) -> &str {
    row.name.namespace.as_deref().unwrap_or("")
}

fn accepts(ns: &[&str], actual: &str) -> bool {
    ns.contains(&actual)
}

impl Doc {
    pub fn root_local(&self) -> &str {
        &self.rows[0].name.local
    }

    /// Checks the root element against the model's name and namespace.
    pub fn check_root(&self, id: &str, root: Option<&str>, ns: &[&str]) -> ContractResult<()> {
        let Some(root) = root else {
            return Err(fail(
                ErrorReason::UnsupportedMapping,
                id,
                None,
                None,
                "",
                "model has no root element mapping",
            ));
        };
        let row = &self.rows[0];
        if row.name.local != root || !accepts(ns, ns_of(row)) {
            let expected = match ns.first() {
                Some(uri) if !uri.is_empty() => format!(" in {uri}"),
                _ => String::new(),
            };
            let found = match ns_of(row) {
                "" => String::new(),
                uri => format!(" in {uri}"),
            };
            return Err(fail(
                ErrorReason::InvalidDocument,
                id,
                None,
                None,
                "/",
                format!(
                    "expected root element {root}{expected}, found {}{found}",
                    row.name.local
                ),
            ));
        }
        Ok(())
    }

    /// Direct Text and CDATA content of an element, concatenated.
    pub fn text_of(&self, ord: usize) -> String {
        let mut out = String::new();
        for kid in &self.rows[ord].content {
            if let ContentRow::Text(t) | ContentRow::CData(t) = kid {
                out.push_str(t);
            }
        }
        out
    }

    /// First attribute whose local name and namespace are accepted.
    pub fn attribute(&self, ord: usize, names: &[&str], ns: &[&str]) -> Option<&str> {
        self.rows[ord]
            .attributes
            .iter()
            .find(|a| {
                names.contains(&a.name.local.as_str())
                    && accepts(ns, a.name.namespace.as_deref().unwrap_or(""))
            })
            .map(|a| a.value.as_str())
    }

    fn in_scope(&self, mut ord: usize, prefix: Option<&str>) -> Option<String> {
        if prefix == Some("xml") {
            return Some(XML_NS.into());
        }
        loop {
            let row = &self.rows[ord];
            if let Some(d) = row
                .namespaces
                .iter()
                .find(|d| d.prefix.as_deref() == prefix)
            {
                return Some(d.uri.clone());
            }
            ord = row.parent?;
        }
    }
}

/// Reader rule tables are `const` in generated code.
pub struct AttrRule {
    pub to: &'static str,
    pub names: &'static [&'static str],
    /// Accepted namespaces; "" is no namespace.
    pub ns: &'static [&'static str],
}

pub struct ElemRule {
    pub to: &'static str,
    pub names: &'static [&'static str],
    pub ns: &'static [&'static str],
    pub collection: bool,
    /// An empty element reads as nil (every scalar type but strings).
    pub nullable: bool,
}

pub struct ContentRule {
    pub to: &'static str,
    pub collection: bool,
}

pub struct ModelSpec {
    pub id: &'static str,
    pub ordered: bool,
    pub mixed: bool,
    /// The model declares the ordered-content member, so tokens are captured.
    pub tokens: bool,
    pub attrs: &'static [AttrRule],
    pub elems: &'static [ElemRule],
    pub content: Option<ContentRule>,
    /// The model collects unclaimed attributes (`any_attribute`).
    pub any: bool,
}

pub struct ElemHit {
    pub rule: usize,
    pub ord: usize,
    /// Index among accepted items of the same rule.
    pub index: usize,
    pub loc: String,
}

pub struct Scan {
    /// One entry per `ModelSpec::attrs`.
    pub attrs: Vec<Option<String>>,
    /// Accepted element children in document order.
    pub elems: Vec<ElemHit>,
    pub content_list: Vec<String>,
    pub content_text: String,
    pub tokens: Vec<ContentToken>,
    /// Unclaimed attributes as `(local name, value)` in document order; empty
    /// unless the model has an `any_attribute` rule.
    pub any: Vec<(String, String)>,
}

/// Ruby `map_all_attributes`: every attribute no explicit attribute rule claims
/// (by local name; `schemaLocation` is excluded), keyed by local name with the
/// prefix stripped. A repeated local name keeps its first position and takes the
/// later value. `xmlns` declarations are not attributes.
fn unclaimed(row: &ElementRow, spec: &ModelSpec) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for a in &row.attributes {
        let local = a.name.local.as_str();
        if local == "schemaLocation" || spec.attrs.iter().any(|r| r.names.contains(&local)) {
            continue;
        }
        match out.iter_mut().find(|e| e.0 == local) {
            Some(e) => e.1 = a.value.clone(),
            None => out.push((local.into(), a.value.clone())),
        }
    }
    out
}

/// One pass over an element row: attributes, accepted children, text and the
/// ordered-content token list. Typing is left to generated code.
pub fn scan(doc: &Doc, ord: usize, spec: &ModelSpec, loc: &str) -> ContractResult<Scan> {
    let row = &doc.rows[ord];
    let attrs = spec
        .attrs
        .iter()
        .map(|r| doc.attribute(ord, r.names, r.ns).map(String::from))
        .collect();
    let text_nodes = row
        .content
        .iter()
        .filter(|k| matches!(k, ContentRow::Text(_)))
        .count();
    let positional = spec.mixed || spec.ordered;
    let mut seen: HashMap<&str, usize> = HashMap::new();
    let mut counts = vec![0usize; spec.elems.len()];
    let mut out = Scan {
        attrs,
        elems: Vec::new(),
        content_list: Vec::new(),
        content_text: doc.text_of(ord),
        tokens: Vec::new(),
        any: if spec.any {
            unclaimed(row, spec)
        } else {
            Vec::new()
        },
    };
    let mut texts: Vec<String> = Vec::new();
    let mut text_index = 0u64;
    for kid in &row.content {
        match kid {
            ContentRow::Element(child) => {
                let crow = &doc.rows[*child];
                let n = seen.entry(crow.name.local.as_str()).or_insert(0);
                let child_loc = format!("{loc}/{}[{n}]", crow.name.local);
                *n += 1;
                let Some(rule) = spec.elems.iter().position(|r| {
                    r.names.contains(&crow.name.local.as_str()) && accepts(r.ns, ns_of(crow))
                }) else {
                    continue;
                };
                let r = &spec.elems[rule];
                let taken = counts[rule];
                if !r.collection && taken > 0 {
                    continue;
                }
                if r.collection && r.nullable && doc.text_of(*child).trim().is_empty() {
                    continue;
                }
                counts[rule] += 1;
                out.elems.push(ElemHit {
                    rule,
                    ord: *child,
                    index: taken,
                    loc: child_loc,
                });
                if spec.tokens {
                    out.tokens.push(ContentToken::Element {
                        attribute: r.to.into(),
                        index: taken as u64,
                    });
                }
            }
            ContentRow::Text(t) => {
                if !spec.mixed && spec.ordered && t.trim().is_empty() {
                    continue;
                }
                if spec.content.is_some() && positional {
                    texts.push(t.clone());
                }
                if positional {
                    let index = text_index;
                    text_index += 1;
                    let mapped = spec
                        .content
                        .as_ref()
                        .is_some_and(|c| c.collection || (text_nodes == 1 && index == 0));
                    if spec.tokens {
                        let source = match (&spec.content, mapped) {
                            (Some(c), true) => TextSource::Member {
                                attribute: c.to.into(),
                                index,
                            },
                            _ => TextSource::Literal { literal: t.clone() },
                        };
                        out.tokens.push(ContentToken::Text { source });
                    }
                }
            }
            ContentRow::CData(t) => {
                if spec.tokens {
                    out.tokens.push(ContentToken::Cdata {
                        source: TextSource::Literal { literal: t.clone() },
                    });
                }
            }
            ContentRow::Comment(_) | ContentRow::ProcessingInstruction { .. } => {
                if spec.tokens {
                    return Err(fail(
                        ErrorReason::InvalidOrderedContent,
                        spec.id,
                        None,
                        None,
                        loc,
                        "comments and processing instructions have no ordered-content token",
                    ));
                }
            }
        }
    }
    out.content_list = if positional {
        texts
    } else {
        row.content
            .iter()
            .filter_map(|k| match k {
                ContentRow::Text(t) => Some(t.clone()),
                _ => None,
            })
            .collect()
    };
    Ok(out)
}

/// The XML lexical family of a scalar. Decoding and encoding go through a
/// transient `serde_json::Value`, so typed models stay static and exact
/// numbers keep their lexical digits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Str,
    Bool,
    Int,
    Decimal,
    Float,
    QName,
}

/// Decodes XML text into the member type. `None` is nil: an empty value of any
/// kind but `Str`.
pub fn decode<T: DeserializeOwned>(
    doc: &Doc,
    ord: usize,
    kind: Kind,
    text: &str,
    at: &Where<'_>,
) -> ContractResult<Option<T>> {
    let t = text.trim();
    let value = match kind {
        Kind::Str => Value::String(text.into()),
        _ if t.is_empty() => return Ok(None),
        Kind::Bool => match t {
            "true" | "1" => Value::Bool(true),
            "false" | "0" => Value::Bool(false),
            _ => return Err(at.fail(ErrorReason::InvalidDocument, "expected true, false, 1 or 0")),
        },
        Kind::Int => {
            let digits = t.strip_prefix(['+', '-']).unwrap_or(t);
            if digits.is_empty() || !digits.bytes().all(|c| c.is_ascii_digit()) {
                return Err(at.fail(ErrorReason::InvalidDocument, "expected an exact integer"));
            }
            Value::String(t.strip_prefix('+').unwrap_or(t).into())
        }
        Kind::Decimal | Kind::Float => Value::String(t.into()),
        Kind::QName => {
            let (prefix, local) = match t.split_once(':') {
                Some((p, l)) => (Some(p), l),
                None => (None, t),
            };
            if local.is_empty() || local.contains(':') {
                return Err(at.fail(ErrorReason::InvalidDocument, "expected a QName"));
            }
            let uri = match doc.in_scope(ord, prefix) {
                Some(uri) => uri,
                None if prefix.is_none() => String::new(),
                None => {
                    return Err(at.fail(
                        ErrorReason::InvalidDocument,
                        format!("QName prefix {} is not bound", prefix.unwrap_or("")),
                    ))
                }
            };
            json!({ "namespace": uri, "local": local })
        }
    };
    serde_json::from_value(value)
        .map(Some)
        .map_err(|e| at.fail(ErrorReason::InvalidDocument, e))
}

/// A discriminator selector: the attribute on the child element, if present.
pub fn discriminator(doc: &Doc, ord: usize, names: &[&str], ns: &[&str]) -> Option<String> {
    doc.attribute(ord, names, ns).map(String::from)
}

pub fn unknown_discriminator(at: &Where<'_>, tag: &str) -> Box<ContractError> {
    at.fail(
        ErrorReason::UnknownDiscriminator,
        format!("unknown discriminator value {tag:?}"),
    )
}

/// Moves a value between two static types through its serde form (a discriminator
/// case and its declared base share one wire shape; the token list is shared
/// between the contract and the model-local `ContentToken`).
pub fn retype<A: Serialize, B: DeserializeOwned>(a: &A, at: &Where<'_>) -> ContractResult<B> {
    serde_json::to_value(a)
        .and_then(serde_json::from_value)
        .map_err(|e| at.fail(ErrorReason::InvalidDocument, e))
}

/// The discriminator tag of a typed value, read from its serde form.
pub fn tag_of<A: Serialize>(a: &A, attribute: &str) -> Option<String> {
    serde_json::to_value(a)
        .ok()?
        .get(attribute)?
        .as_str()
        .map(String::from)
}

// ------------------------------------------------------------------ writing

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Policy {
    Omit,
    /// Write `xsi:nil="true"`.
    AsNil,
    /// Write an empty value.
    AsBlank,
}

pub struct NsOut {
    pub uri: &'static str,
    pub prefix: Option<&'static str>,
}

pub struct WAttr {
    pub to: &'static str,
    pub name: &'static str,
    pub ns: Option<NsOut>,
    pub nil: Policy,
    pub empty: Policy,
}

pub struct WElem {
    pub to: &'static str,
    pub name: &'static str,
    pub ns: Option<NsOut>,
    pub collection: bool,
    pub cdata: bool,
    pub nil: Policy,
    pub empty: Policy,
}

pub struct WContent {
    pub to: &'static str,
    pub collection: bool,
    pub cdata: bool,
}

pub struct WSpec {
    pub id: &'static str,
    /// The model carries ordered-content tokens.
    pub ordered: bool,
    /// A reason the writer refuses this mapping by name (`unsupported_mapping`).
    pub refusal: Option<&'static str>,
    pub attrs: &'static [WAttr],
    pub elems: &'static [WElem],
    pub content: Option<WContent>,
}

pub enum Item {
    Text(String),
    Child(ElementSpec),
}

/// Typed values already encoded by generated code, parallel to the `WSpec`.
#[derive(Default)]
pub struct Values {
    /// Per attribute rule: encoded text, None for nil/absent.
    pub attrs: Vec<Option<String>>,
    /// Per element rule: items, None for nil/absent.
    pub elems: Vec<Option<Vec<Item>>>,
    pub content: Vec<String>,
    pub tokens: Option<Vec<ContentToken>>,
    /// `any_attribute` entries: one unprefixed, un-namespaced attribute each.
    pub any: Vec<(String, String)>,
}

fn named(name: &str, ns: Option<&NsOut>) -> QName {
    match ns {
        Some(ns) => QName {
            prefix: ns.prefix.map(String::from),
            local: name.into(),
            namespace: Some(ns.uri.into()),
        },
        None => QName::local(name),
    }
}

fn nil_mark(el: &mut ElementSpec) {
    el.attributes
        .push(Attribute::new(QName::ns(Some("xsi"), "nil", XSI), "true"));
}

fn text_node(text: &str, cdata: bool) -> ContentSpec {
    if cdata {
        ContentSpec::CData(text.into())
    } else {
        ContentSpec::Text(text.into())
    }
}

fn leaf(rule: &WElem, text: &str) -> ElementSpec {
    let mut el = ElementSpec::new(named(rule.name, rule.ns.as_ref()));
    if !text.is_empty() {
        el.content.push(text_node(text, rule.cdata));
    }
    el
}

fn write_elem(el: &mut ElementSpec, rule: &WElem, item: Item) {
    match item {
        Item::Child(child) => el.content.push(ContentSpec::Element(child)),
        Item::Text(text) => {
            if text.is_empty() && rule.empty == Policy::Omit {
                return;
            }
            let mut leaf = leaf(rule, &text);
            if text.is_empty() && rule.empty == Policy::AsNil {
                nil_mark(&mut leaf);
            }
            el.content.push(ContentSpec::Element(leaf));
        }
    }
}

fn write_nil(el: &mut ElementSpec, rule: &WElem, policy: Policy) {
    if policy == Policy::Omit {
        return;
    }
    let mut leaf = leaf(rule, "");
    if policy == Policy::AsNil {
        nil_mark(&mut leaf);
    }
    el.content.push(ContentSpec::Element(leaf));
}

/// Builds one element from encoded values: attributes first, then either the
/// ordered-content tokens (never regrouped) or the element rules in mapping
/// order. A mapped value that the token list omits is an error, never dropped.
pub fn assemble(
    spec: &WSpec,
    name: &str,
    ns: Option<&NsOut>,
    values: Values,
) -> ContractResult<ElementSpec> {
    let refuse = |reason, detail: &str| fail(reason, spec.id, None, None, "", detail);
    if let Some(reason) = spec.refusal {
        return Err(refuse(
            ErrorReason::UnsupportedMapping,
            &format!("unsupported XML writer mapping: {reason}"),
        ));
    }
    let mut el = ElementSpec::new(named(name, ns));
    for (rule, raw) in spec.attrs.iter().zip(&values.attrs) {
        let qn = named(rule.name, rule.ns.as_ref());
        match raw {
            None => {
                if rule.nil == Policy::AsBlank {
                    el.attributes.push(Attribute::new(qn, ""));
                }
            }
            Some(text) => {
                if text.is_empty() && rule.empty == Policy::Omit {
                    continue;
                }
                el.attributes.push(Attribute::new(qn, text));
            }
        }
    }
    for (name, value) in &values.any {
        el.attributes
            .push(Attribute::new(QName::local(name.as_str()), value.as_str()));
    }
    let content_cdata = spec.content.as_ref().is_some_and(|c| c.cdata);
    if let (true, Some(tokens)) = (spec.ordered, &values.tokens) {
        let mut elems: Vec<Option<Vec<Option<Item>>>> = values
            .elems
            .into_iter()
            .map(|list| list.map(|l| l.into_iter().map(Some).collect()))
            .collect();
        let mut used_content = vec![false; values.content.len()];
        for (position, token) in tokens.iter().enumerate() {
            let bad = |detail: &str| {
                fail(
                    ErrorReason::InvalidOrderedContent,
                    spec.id,
                    None,
                    None,
                    &format!("/__lml_content/{position}"),
                    detail,
                )
            };
            match token {
                ContentToken::Text {
                    source: TextSource::Literal { literal },
                } => el.content.push(ContentSpec::Text(literal.clone())),
                ContentToken::Cdata {
                    source: TextSource::Literal { literal },
                } => el.content.push(ContentSpec::CData(literal.clone())),
                ContentToken::Text {
                    source: TextSource::Member { index, .. },
                }
                | ContentToken::Cdata {
                    source: TextSource::Member { index, .. },
                } => {
                    let slot = usize::try_from(*index)
                        .ok()
                        .and_then(|i| used_content.get_mut(i).map(|u| (i, u)))
                        .filter(|(_, used)| !**used)
                        .ok_or_else(|| bad("text token references no remaining content"))?;
                    *slot.1 = true;
                    let cdata = content_cdata || matches!(token, ContentToken::Cdata { .. });
                    el.content.push(text_node(&values.content[slot.0], cdata));
                }
                ContentToken::Element { attribute, index } => {
                    let rule = spec
                        .elems
                        .iter()
                        .position(|r| r.to == attribute)
                        .ok_or_else(|| bad("element token names an unmapped member"))?;
                    let item = usize::try_from(*index)
                        .ok()
                        .and_then(|i| elems[rule].as_mut()?.get_mut(i)?.take())
                        .ok_or_else(|| bad("element token references no remaining value"))?;
                    write_elem(&mut el, &spec.elems[rule], item);
                }
            }
        }
        let dropped = elems.iter().flatten().flatten().any(Option::is_some)
            || used_content.iter().any(|u| !u);
        if dropped {
            return Err(fail(
                ErrorReason::InvalidOrderedContent,
                spec.id,
                None,
                None,
                &format!("/__lml_content/{}", tokens.len()),
                "a mapped value is not referenced by the token list",
            ));
        }
        return Ok(el);
    }
    for (rule, list) in spec.elems.iter().zip(values.elems) {
        match list {
            None => write_nil(&mut el, rule, rule.nil),
            Some(items) if rule.collection && items.is_empty() => {
                write_nil(&mut el, rule, rule.empty)
            }
            Some(items) => {
                for item in items {
                    write_elem(&mut el, rule, item);
                }
            }
        }
    }
    for text in &values.content {
        el.content.push(text_node(text, content_cdata));
    }
    Ok(el)
}

/// Encodes one typed member to XML text.
pub fn encode<T: Serialize>(x: &T, kind: Kind, id: &str, member: &str) -> ContractResult<String> {
    let bad = |detail: &str| {
        fail(
            ErrorReason::NotSerializable,
            id,
            Some(member),
            None,
            "/output",
            detail,
        )
    };
    let value = serde_json::to_value(x).map_err(|e| bad(&e.to_string()))?;
    match (kind, value) {
        (_, Value::String(s)) => Ok(s),
        (_, Value::Bool(b)) => Ok(b.to_string()),
        (_, Value::Number(n)) => Ok(n.to_string()),
        (Kind::QName, Value::Object(o)) => match (o.get("namespace"), o.get("local")) {
            (Some(Value::String(ns)), Some(Value::String(local))) if ns.is_empty() => {
                Ok(local.clone())
            }
            (Some(Value::String(_)), Some(Value::String(_))) => Err(bad(
                "a namespaced QName needs a prefix binding that no mapping declares",
            )),
            _ => Err(bad("malformed QName value")),
        },
        _ => Err(bad("value has no XML text form")),
    }
}

/// Serializes a built element as a document, compact and deterministic.
pub fn to_xml(id: &str, root: ElementSpec) -> ContractResult<String> {
    let bad = |e: moxml::document::Error| {
        fail(
            ErrorReason::NotSerializable,
            id,
            None,
            None,
            "/output",
            e.to_string(),
        )
    };
    Document::build(root).map_err(bad)?.to_xml().map_err(bad)
}

pub fn check_projection(projection: &super::contract::Projection, id: &str) -> ContractResult<()> {
    if projection.model_id == id {
        Ok(())
    } else {
        Err(fail(
            ErrorReason::UnsupportedMapping,
            id,
            None,
            None,
            "",
            format!("projection is for {}", projection.model_id),
        ))
    }
}

/// Probes the exact faces this binding needs; used by tests and consumers to
/// fail loudly when the pinned dependency lacks one.
pub fn probe() -> Result<(), String> {
    let doc = Document::parse(b"<a xmlns=\"urn:a\" x=\"1\">t<![CDATA[c]]><b/></a>")
        .map_err(|e| e.to_string())?;
    let rows = doc.element_rows();
    let ok = rows.len() == 2
        && rows[0].name.namespace.as_deref() == Some("urn:a")
        && rows[0].attributes.len() == 1
        && matches!(rows[0].content[1], ContentRow::CData(_));
    if !ok {
        return Err("element_rows does not expose names, attributes and CDATA".into());
    }
    if Document::parse(b"<!DOCTYPE a><a/>").is_ok() {
        return Err("default policy must reject DOCTYPE".into());
    }
    let built = Document::build(ElementSpec::new(QName::local("r")).cdata("x"))
        .and_then(|d| d.to_xml())
        .map_err(|e| e.to_string())?;
    if built != "<r><![CDATA[x]]></r>" {
        return Err(format!("unexpected serialization {built}"));
    }
    Ok(())
}
