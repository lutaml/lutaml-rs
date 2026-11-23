//! Per-model XML writer generation. Typed values are encoded by generated
//! code; `rt::assemble` places them (attributes first, then ordered tokens or
//! mapping order) and never regroups or silently drops a mapped value.
use super::reader_emitter::{ns_lit, q, Child, Plan};
use std::fmt::Write;

const EL: &str = "rt::moxml::document::ElementSpec";

/// `<S, H: XmlHooks<S>>` / `, cx: &mut XmlCx<S, H>` / `, cx`: the hook context threaded through every
/// generated function of a bundle that has XML hooks (empty otherwise, leaving ordinary output unchanged).
pub(super) fn gen(hooked: bool) -> &'static str {
    if hooked {
        "<S, H: XmlHooks<S>>"
    } else {
        ""
    }
}
pub(super) fn cxp(hooked: bool) -> &'static str {
    if hooked {
        ", cx: &mut XmlCx<S, H>"
    } else {
        ""
    }
}
pub(super) fn cxa(hooked: bool) -> &'static str {
    if hooked {
        ", cx"
    } else {
        ""
    }
}

pub(super) fn emit_writer(out: &mut String, p: &Plan) {
    let (id, name) = (q(&p.id), &p.name);
    let (g, cp, ca) = (gen(p.hooked), cxp(p.hooked), cxa(p.hooked));
    // Hook-owned rules write their whole output through the handwritten hook: they are not in the tables.
    let attrs_w: Vec<_> = p.attrs.iter().filter(|a| !hook_owned(&a.hooks)).collect();
    let elems_w: Vec<_> = p.elems.iter().filter(|e| !hook_owned(&e.hooks)).collect();
    let content_w = p.content.as_ref().filter(|c| !hook_owned(&c.hooks));
    let refusal = if p.writer_refusals.is_empty() {
        "None".to_string()
    } else {
        format!("Some({})", q(&p.writer_refusals.join("; ")))
    };
    let attrs: String = attrs_w
        .iter()
        .map(|a| {
            format!(
                "rt::WAttr {{ to: {}, name: {}, ns: {}, nil: {}, empty: {} }}, ",
                q(&a.member),
                q(&a.names[0]),
                ns_lit(&a.out),
                a.nil.path(),
                a.empty.path()
            )
        })
        .collect();
    let elems: String = elems_w
        .iter()
        .map(|e| {
            format!(
                "rt::WElem {{ to: {}, name: {}, ns: {}, collection: {}, cdata: {}, nil: {}, empty: {} }}, ",
                q(&e.member),
                q(&e.names[0]),
                ns_lit(&e.out),
                e.collection,
                e.cdata,
                e.nil.path(),
                e.empty.path()
            )
        })
        .collect();
    let content = content_w.map_or("None".into(), |c| {
        format!(
            "Some(rt::WContent {{ to: {}, collection: {}, cdata: {} }})",
            q(&c.member),
            c.collection,
            c.cdata
        )
    });
    writeln!(
        out,
        "const WS_{name}: rt::WSpec = rt::WSpec {{ id: {id}, ordered: {}, refusal: {refusal}, attrs: &[{attrs}], elems: &[{elems}], content: {content} }};",
        p.tokens
    )
    .unwrap();

    // Discriminated element rules choose the case writer by tag.
    for (i, e) in elems_w.iter().enumerate() {
        if let Child::Poly { union, cases, .. } = &e.child {
            let mut arms = String::new();
            for (index, (_, case)) in cases.iter().enumerate() {
                arms +=
                    &format!("{union}::Case{index}(c) => write_{case}_el(c, name, ns, None{ca}), ");
            }
            writeln!(
                out,
                "fn write_{name}_{i}_poly{g}(x: &{union}, name: &str, ns: Option<&rt::NsOut>{cp}) -> ContractResult<{EL}> {{\n match x {{ {arms}}}\n}}"
            )
            .unwrap();
        }
    }

    writeln!(
        out,
        "fn write_{name}_el{g}(v: &{name}, name: &str, ns: Option<&rt::NsOut>, order: Option<&[super::contracts::ContentToken]>{cp}) -> ContractResult<{EL}> {{\n let mut values = rt::Values::default();"
    )
    .unwrap();
    for a in &attrs_w {
        let guard = keep(&a.hooks, &a.member);
        writeln!(
            out,
            " values.attrs.push(match &v.{} {{ Presence::Value(x){} => Some(rt::encode(x, {}, {id}, {})?), _ => None }});",
            a.ident,
            arm_guard(&guard),
            a.kind.path(),
            q(&a.member)
        )
        .unwrap();
    }
    for (i, e) in elems_w.iter().enumerate() {
        let nm = format!("WS_{name}.elems[{i}].name");
        let ns = format!("WS_{name}.elems[{i}].ns.as_ref()");
        let one = match &e.child {
            Child::Scalar(k) => format!(
                "rt::Item::Text(rt::encode(x, {}, {id}, {})?)",
                k.path(),
                q(&e.member)
            ),
            Child::Model(n) => format!("rt::Item::Child(write_{n}_el(x, {nm}, {ns}, None{ca})?)"),
            Child::Poly { .. } => {
                format!("rt::Item::Child(write_{name}_{i}_poly(x, {nm}, {ns}{ca})?)")
            }
        };
        let arm = if e.collection {
            format!("Presence::Value(l) => {{ let mut items = Vec::new(); for x in l {{ items.push({one}); }} Some(items) }}")
        } else {
            let guard = keep(&e.hooks, &e.member);
            format!(
                "Presence::Value(x){} => Some(vec![{one}])",
                arm_guard(&guard)
            )
        };
        writeln!(
            out,
            " values.elems.push(match &v.{} {{ {arm}, _ => None }});",
            e.ident
        )
        .unwrap();
    }
    if let Some(c) = content_w {
        let k = c.kind.path();
        let enc = format!("rt::encode(x, {k}, {id}, {})?", q(&c.member));
        if c.collection {
            writeln!(out, " if let Presence::Value(l) = &v.{} {{ for x in l {{ values.content.push({enc}); }} }}", c.ident).unwrap();
        } else {
            let guard = keep(&c.hooks, &c.member);
            if guard.is_empty() {
                writeln!(
                    out,
                    " if let Presence::Value(x) = &v.{} {{ values.content.push({enc}); }}",
                    c.ident
                )
                .unwrap();
            } else {
                writeln!(
                    out,
                    " if let Presence::Value(x) = &v.{} {{ if {guard} {{ values.content.push({enc}); }} }}",
                    c.ident
                )
                .unwrap();
            }
        }
    }
    if let Some((member, ident)) = &p.any {
        writeln!(
            out,
            " if let Presence::Value(h) = &v.{ident} {{ values.any = h.attribute_pairs().map_err(|d| rt::fail(ErrorReason::NotSerializable, {id}, Some({}), None, \"/output\", d))?; }}",
            q(member)
        )
        .unwrap();
    }
    if p.hooked && has_write_hooks(p) {
        // Declared positions let `assemble` keep ordinary and handwritten output in rule order.
        let pos: Vec<String> = elems_w.iter().map(|e| e.hooks.pos.to_string()).collect();
        writeln!(out, " values.elem_pos = vec![{}];", pos.join(", ")).unwrap();
        if let Some(c) = content_w {
            writeln!(out, " values.content_pos = Some({});", c.hooks.pos).unwrap();
        }
        // Each hook writes into its own scratch element; its attributes join the start tag, its content is
        // placed at the rule's position.
        for (kind, hooks, member) in write_hook_rules(p) {
            match &hooks.write {
                Some(hook) => writeln!(out, " {{ let mut scratch = {EL}::new(rt::moxml::document::QName::local(\"lml-hook\")); <H as {name}XmlHooks<S>>::{hook}(cx.hooks, v, &mut rt::XmlParent::new(&mut scratch), cx.state)?; values.hooks.push(({}, scratch)); }}", hooks.pos).unwrap(),
                None => writeln!(out, " return Err(rt::fail(ErrorReason::CustomSerialization, {id}, None, Some({}), \"/\", \"no write hook is declared for this {kind} rule\"));", q(&member)).unwrap(),
            }
        }
    }
    if p.tokens {
        writeln!(out, " let at = rt::Where {{ id: {id}, attribute: \"__lml_content\", rule: \"\", loc: \"/__lml_content\" }};\n values.tokens = match order {{ Some(t) => Some(t.to_vec()), None => match &v.__lml_content {{ Presence::Value(t) => Some(rt::retype(t, &at)?), _ => None }} }};").unwrap();
    } else {
        writeln!(out, " if order.is_some() {{ return Err(rt::fail(ErrorReason::InvalidOrderedContent, {id}, None, None, \"/order\", \"the model carries no ordered content\")); }}").unwrap();
    }
    writeln!(out, " rt::assemble(&WS_{name}, name, ns, values)\n}}").unwrap();
    let root = match &p.root {
        Some(r) => format!(
            "let ns = {};\n write_{name}_el(v, {}, ns.as_ref(), order{ca})",
            ns_lit(&p.own),
            q(r)
        ),
        None => format!(
            "let _ = order;\n Err(rt::fail(ErrorReason::UnsupportedMapping, {id}, None, None, \"\", \"the model has no root element\"))"
        ),
    };
    writeln!(
        out,
        "fn write_{name}_root{g}(v: &{name}, order: Option<&[super::contracts::ContentToken]>{cp}) -> ContractResult<String> {{\n v.validate().map_err(|e| rt::fail(ErrorReason::NotSerializable, e.model_id, (!e.attribute.is_empty()).then_some(e.attribute), None, \"/output\", e.reason))?;\n let el = {{ {root} }}?;\n rt::to_xml({id}, el)\n}}"
    )
    .unwrap();
    if p.hooked {
        writeln!(
            out,
            "pub fn write_{name}_xml(v: &{name}) -> ContractResult<String> {{\n write_{name}_xml_with(v, &NoXmlHooks, &mut ())\n}}\npub fn write_{name}_xml_with<S, H: XmlHooks<S>>(v: &{name}, hooks: &H, state: &mut S) -> ContractResult<String> {{\n write_{name}_root(v, None, &mut XmlCx {{ hooks, state }})\n}}"
        )
        .unwrap();
    } else {
        writeln!(
            out,
            "pub fn write_{name}_xml(v: &{name}) -> ContractResult<String> {{\n write_{name}_root(v, None)\n}}"
        )
        .unwrap();
    }
    let (read_call, write_call) = if p.hooked {
        (
            format!("read_{name}_xml(&input)"),
            format!("write_{name}_root(model, order, &mut XmlCx {{ hooks: &NoXmlHooks, state: &mut () }})"),
        )
    } else {
        (
            format!("read_{name}_xml(&input)"),
            format!("write_{name}_root(model, order)"),
        )
    };
    writeln!(
        out,
        "pub struct XmlCodec_{name};\nimpl XmlReader for XmlCodec_{name} {{\n type Input = Vec<u8>;\n type Model = {name};\n fn read_batch(&mut self, projection: &Projection, input: Vec<u8>) -> ContractResult<{name}> {{\n  rt::check_projection(projection, {id})?;\n  {read_call}\n }}\n}}\nimpl XmlWriter for XmlCodec_{name} {{\n type Model = {name};\n type Output = String;\n fn write_batch(&mut self, projection: &Projection, model: &{name}, order: Option<&[super::contracts::ContentToken]>) -> ContractResult<String> {{\n  rt::check_projection(projection, {id})?;\n  {write_call}\n }}\n}}"
    )
    .unwrap();
}

/// A rule whose write direction is handwritten (or declared hook-only): it is not in the writer tables.
fn hook_owned(h: &super::reader_emitter::Hooks) -> bool {
    h.write.is_some() || h.only
}
fn arm_guard(guard: &str) -> String {
    if guard.is_empty() {
        String::new()
    } else {
        format!(" if {guard}")
    }
}
/// The condition that keeps a value (empty when every value is kept). A default `apply_defaults` supplied (`render default` not true): the model
/// records provenance in `__lml_applied`, exactly like the key-value codecs.
fn keep(h: &super::reader_emitter::Hooks, member: &str) -> String {
    match &h.suppress {
        Some(literal) => format!(
            "!(v.__lml_applied.contains({}) && rt::is_default(x, {}))",
            q(member),
            q(literal)
        ),
        None => String::new(),
    }
}
fn write_hook_rules(p: &Plan) -> Vec<(&'static str, super::reader_emitter::Hooks, String)> {
    let mut rules = Vec::new();
    for a in &p.attrs {
        if hook_owned(&a.hooks) {
            rules.push(("attribute", a.hooks.clone(), a.names[0].clone()));
        }
    }
    for e in &p.elems {
        if hook_owned(&e.hooks) {
            rules.push(("element", e.hooks.clone(), e.names[0].clone()));
        }
    }
    if let Some(c) = &p.content {
        if hook_owned(&c.hooks) {
            rules.push(("content", c.hooks.clone(), String::new()));
        }
    }
    rules
}
fn has_write_hooks(p: &Plan) -> bool {
    !write_hook_rules(p).is_empty()
}
