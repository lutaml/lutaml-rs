//! Per-model XML writer generation. Typed values are encoded by generated
//! code; `rt::assemble` places them (attributes first, then ordered tokens or
//! mapping order) and never regroups or silently drops a mapped value.
use super::reader_emitter::{ns_lit, q, Child, Plan};
use std::fmt::Write;

const EL: &str = "rt::moxml::document::ElementSpec";

pub(super) fn emit_writer(out: &mut String, p: &Plan) {
    let (id, name) = (q(&p.id), &p.name);
    let refusal = if p.writer_refusals.is_empty() {
        "None".to_string()
    } else {
        format!("Some({})", q(&p.writer_refusals.join("; ")))
    };
    let attrs: String = p
        .attrs
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
    let elems: String = p
        .elems
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
    let content = p.content.as_ref().map_or("None".into(), |c| {
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
    for (i, e) in p.elems.iter().enumerate() {
        if let Child::Poly { union, cases, .. } = &e.child {
            let mut arms = String::new();
            for (index, (_, case)) in cases.iter().enumerate() {
                arms += &format!("{union}::Case{index}(c) => write_{case}_el(c, name, ns, None), ");
            }
            writeln!(
                out,
                "fn write_{name}_{i}_poly(x: &{union}, name: &str, ns: Option<&rt::NsOut>) -> ContractResult<{EL}> {{\n match x {{ {arms}}}\n}}"
            )
            .unwrap();
        }
    }

    writeln!(
        out,
        "fn write_{name}_el(v: &{name}, name: &str, ns: Option<&rt::NsOut>, order: Option<&[super::contracts::ContentToken]>) -> ContractResult<{EL}> {{\n let mut values = rt::Values::default();"
    )
    .unwrap();
    for a in &p.attrs {
        writeln!(
            out,
            " values.attrs.push(match &v.{} {{ Presence::Value(x) => Some({}), _ => None }});",
            a.ident,
            a.kind.encode(&id, &q(&a.member))
        )
        .unwrap();
    }
    for (i, e) in p.elems.iter().enumerate() {
        let nm = format!("WS_{name}.elems[{i}].name");
        let ns = format!("WS_{name}.elems[{i}].ns.as_ref()");
        let one = match &e.child {
            Child::Scalar(k) => format!("rt::Item::Text({})", k.encode(&id, &q(&e.member))),
            Child::Model(n) => format!("rt::Item::Child(write_{n}_el(x, {nm}, {ns}, None)?)"),
            Child::Poly { .. } => {
                format!("rt::Item::Child(write_{name}_{i}_poly(x, {nm}, {ns})?)")
            }
        };
        let arm = if e.collection {
            format!("Presence::Value(l) => {{ let mut items = Vec::new(); for x in l {{ items.push({one}); }} Some(items) }}")
        } else {
            format!("Presence::Value(x) => Some(vec![{one}])")
        };
        writeln!(
            out,
            " values.elems.push(match &v.{} {{ {arm}, _ => None }});",
            e.ident
        )
        .unwrap();
    }
    if let Some(c) = &p.content {
        let enc = c.kind.encode(&id, &q(&c.member));
        if c.collection {
            writeln!(out, " if let Presence::Value(l) = &v.{} {{ for x in l {{ values.content.push({enc}); }} }}", c.ident).unwrap();
        } else {
            writeln!(
                out,
                " if let Presence::Value(x) = &v.{} {{ values.content.push({enc}); }}",
                c.ident
            )
            .unwrap();
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
    if p.tokens {
        writeln!(out, " let at = rt::Where {{ id: {id}, attribute: \"__lml_content\", rule: \"\", loc: \"/__lml_content\" }};\n values.tokens = match order {{ Some(t) => Some(t.to_vec()), None => match &v.__lml_content {{ Presence::Value(t) => Some(rt::retype(t, &at)?), _ => None }} }};").unwrap();
    } else {
        writeln!(out, " if order.is_some() {{ return Err(rt::fail(ErrorReason::InvalidOrderedContent, {id}, None, None, \"/order\", \"the model carries no ordered content\")); }}").unwrap();
    }
    writeln!(out, " rt::assemble(&WS_{name}, name, ns, values)\n}}").unwrap();

    let root = match &p.root {
        Some(r) => format!(
            "let ns = {};\n write_{name}_el(v, {}, ns.as_ref(), order)",
            ns_lit(&p.own),
            q(r)
        ),
        None => format!(
            "let _ = order;\n Err(rt::fail(ErrorReason::UnsupportedMapping, {id}, None, None, \"\", \"the model has no root element\"))"
        ),
    };
    writeln!(
        out,
        "fn write_{name}_root(v: &{name}, order: Option<&[super::contracts::ContentToken]>) -> ContractResult<String> {{\n v.validate_shape().map_err(|e| rt::fail(ErrorReason::NotSerializable, e.model_id, (!e.attribute.is_empty()).then_some(e.attribute), None, \"/output\", e.reason))?;\n let el = {{ {root} }}?;\n rt::to_xml({id}, el)\n}}\npub fn write_{name}_xml(v: &{name}) -> ContractResult<String> {{\n write_{name}_root(v, None)\n}}"
    )
    .unwrap();
    writeln!(
        out,
        "pub struct XmlCodec_{name};\nimpl XmlReader for XmlCodec_{name} {{\n type Input = Vec<u8>;\n type Model = {name};\n fn read_batch(&mut self, projection: &Projection, input: Vec<u8>) -> ContractResult<{name}> {{\n  rt::check_projection(projection, {id})?;\n  read_{name}_xml(&input)\n }}\n}}\nimpl XmlWriter for XmlCodec_{name} {{\n type Model = {name};\n type Output = String;\n fn write_batch(&mut self, projection: &Projection, model: &{name}, order: Option<&[super::contracts::ContentToken]>) -> ContractResult<String> {{\n  rt::check_projection(projection, {id})?;\n  write_{name}_root(model, order)\n }}\n}}"
    )
    .unwrap();
}
