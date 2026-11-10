use super::{failure, pointer, Node};
use crate::contracts::{ContractResult, ErrorReason};
use std::collections::BTreeSet;

const BUILTINS: &[&str] = &[
    "string",
    "token",
    "uri",
    "email",
    "hostname",
    "ip",
    "boolean",
    "integer",
    "decimal",
    "rational",
    "float",
    "date",
    "time",
    "date_time",
    "time_without_date",
    "duration",
    "qname",
    "base64_binary",
    "hex_binary",
    "symbol",
    "hash",
    "email_address",
    "ip_v4_address",
    "ip_v6_address",
    "uuid",
    "reference",
    "uri_reference",
    "date_with_timezone",
    "date_time_with_timezone",
    "day_time_duration",
    "year_month_duration",
    "positive_integer",
    "non_negative_integer",
];
pub(super) const INTEGER_BUILTINS: &[&str] =
    &["integer", "positive_integer", "non_negative_integer"];
const MAX_INTEGER: u64 = 9_007_199_254_740_991;
mod defaults;
mod graph;
mod mapping;
use defaults::{attribute, validate_member, value_type};
use graph::{bundle, discriminator, validate_refs};
use mapping::{key_mapping, xml};
fn invalid(at: &str, detail: impl Into<String>) -> Box<crate::contracts::ContractError> {
    failure(ErrorReason::InvalidDocument, at, detail)
}
fn object<'a>(n: &'a Node, at: &str) -> ContractResult<&'a [(String, Node)]> {
    n.object().ok_or_else(|| invalid(at, "expected object"))
}
fn array<'a>(n: &'a Node, at: &str) -> ContractResult<&'a [Node]> {
    n.array().ok_or_else(|| invalid(at, "expected array"))
}
fn string<'a>(n: &'a Node, at: &str) -> ContractResult<&'a str> {
    n.string()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| invalid(at, "expected nonempty string"))
}
fn field<'a>(n: &'a Node, k: &str, at: &str) -> ContractResult<&'a Node> {
    n.get(k)
        .ok_or_else(|| invalid(&pointer(at, k), "required member missing"))
}
fn text<'a>(n: &'a Node, k: &str, at: &str) -> ContractResult<&'a str> {
    string(field(n, k, at)?, &pointer(at, k))
}
fn boolean(n: &Node, at: &str) -> ContractResult<()> {
    if matches!(n, Node::Bool(_)) {
        Ok(())
    } else {
        Err(invalid(at, "expected boolean"))
    }
}
fn keys(n: &Node, allowed: &[&str], at: &str) -> ContractResult<()> {
    for (key, _) in object(n, at)? {
        if !allowed.contains(&key.as_str()) {
            let reason = match key.as_str() {
                "custom_initialize" => ErrorReason::CustomInitialize,
                "element_order" | "imperative_element_order" => ErrorReason::ImperativeElementOrder,
                "proc" | "lambda" => ErrorReason::LambdaDefault,
                "with" | "transform" | "custom_methods" | "delegate" | "method" | "callback" => {
                    ErrorReason::UnsupportedMapping
                }
                _ => ErrorReason::InvalidDocument,
            };
            return Err(failure(
                reason,
                &pointer(at, key),
                format!("unsupported member {key}"),
            ));
        }
    }
    Ok(())
}
fn bounded(n: &Node, at: &str) -> ContractResult<u64> {
    n.integer()
        .filter(|v| *v <= MAX_INTEGER)
        .ok_or_else(|| invalid(at, "expected bounded nonnegative integer"))
}
fn bounds(n: &Node, at: &str) -> ContractResult<()> {
    let min = bounded(field(n, "min", at)?, &pointer(at, "min"))?;
    let max = field(n, "max", at)?;
    if max != &Node::Null && bounded(max, &pointer(at, "max"))? < min {
        return Err(invalid(&pointer(at, "max"), "maximum is less than minimum"));
    }
    Ok(())
}
fn strings(n: &Node, at: &str, sorted: bool) -> ContractResult<()> {
    let mut seen = BTreeSet::new();
    let mut previous = None;
    for (i, v) in array(n, at)?.iter().enumerate() {
        let s = string(v, &pointer(at, &i.to_string()))?;
        if !seen.insert(s) {
            return Err(failure(
                ErrorReason::DuplicateId,
                at,
                "duplicate identifier/alias",
            ));
        }
        if sorted && previous.is_some_and(|p| p > s) {
            return Err(invalid(at, "array must be sorted"));
        }
        previous = Some(s);
    }
    Ok(())
}
/// Pinned names that break Serializable (Ruby RESERVED_ATTRIBUTE_NAMES).
const RESERVED_ATTRIBUTE_NAMES: &[&str] = &[
    "__id__",
    "__send__",
    "attribute_order",
    "class",
    "doctype",
    "element_order",
    "encoding",
    "extract_register_id",
    "import_declaration_plan",
    "initialize",
    "initialize_clone",
    "initialize_copy",
    "initialize_dup",
    "lutaml_register",
    "pending_plan_root_element",
    "prepare_instance_format_options",
    "public_send",
    "record_mutation",
    "to_format",
    "to_json",
    "to_xml",
    "to_yaml",
    "value_set_for",
    "xml_declaration",
    "xml_ns_prefixes",
];
/// Attribute names match `\\A[a-z_][a-zA-Z0-9_]*\\z` and avoid the reserved list.
fn attribute_name(name: &str, at: &str) -> ContractResult<()> {
    let mut chars = name.chars();
    let lexical = chars
        .next()
        .is_some_and(|c| c.is_ascii_lowercase() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_');
    if !lexical {
        return Err(invalid(
            at,
            format!("attribute name {name:?} must match \\A[a-z_][a-zA-Z0-9_]*\\z"),
        ));
    }
    if RESERVED_ATTRIBUTE_NAMES.contains(&name) {
        return Err(failure(
            ErrorReason::ReservedMember,
            at,
            format!("attribute name {name} breaks Serializable (reserved attribute name)"),
        ));
    }
    Ok(())
}
fn portable_name(s: &str) -> bool {
    !s.is_empty()
        && s.split("::").all(|part| {
            let mut c = part.chars();
            c.next().is_some_and(|c| c.is_ascii_uppercase())
                && c.all(|c| c.is_ascii_alphanumeric() || c == '_')
        })
}
fn register(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || "_.:-".contains(c))
}
fn neutral(n: &Node, at: &str) -> ContractResult<()> {
    match n {
        Node::Number(num) => {
            if num.as_u64().is_none_or(|v| v > MAX_INTEGER)
                && num.as_i64().is_none_or(|v| v.unsigned_abs() > MAX_INTEGER)
            {
                return Err(invalid(
                    at,
                    "structural number must be a bounded integer; exact values require strings",
                ));
            }
        }
        Node::Array(v) => {
            for (i, n) in v.iter().enumerate() {
                neutral(n, &pointer(at, &i.to_string()))?;
            }
        }
        Node::Object(v) => {
            for (k, n) in v {
                neutral(n, &pointer(at, k))?;
            }
        }
        _ => {}
    }
    Ok(())
}
pub fn validate(n: &Node) -> ContractResult<()> {
    object(n, "")?;
    if n.get("lml_bundle").is_some() {
        neutral(n, "")?;
        bundle(n)
    } else {
        let version = field(n, "lml", "")?.integer();
        match version {
            Some(1) => {
                // Mirrors Ruby DocumentValidator#document_v1 exactly: present
                // members must have their declared shape (null is not "absent").
                string(field(n, "model", "")?, "/model")?;
                if let Some(v) = n.get("register") {
                    if !matches!(v, Node::Null | Node::String(_)) {
                        return Err(invalid("/register", "register must be a string"));
                    }
                }
                if let Some(v) = n.get("attributes") {
                    for (i, attr) in array(v, "/attributes")?.iter().enumerate() {
                        object(attr, &format!("/attributes/{i}"))?;
                    }
                }
                if let Some(v) = n.get("mappings") {
                    object(v, "/mappings")?;
                }
                Ok(())
            }
            Some(2) => {
                neutral(n, "")?;
                model(n, None, "")?;
                let id = format!(
                    "{}@{}",
                    n.get("model").unwrap().string().unwrap(),
                    n.get("register").unwrap().string().unwrap()
                );
                let context = Node::Object(vec![
                    ("models".into(), Node::Object(vec![(id, n.clone())])),
                    ("value_types".into(), Node::Object(vec![])),
                ]);
                for (i, attr) in n
                    .get("attributes")
                    .unwrap()
                    .array()
                    .unwrap()
                    .iter()
                    .enumerate()
                {
                    validate_refs(attr, &context, &format!("/attributes/{i}"))?;
                }
                Ok(())
            }
            _ => Err(failure(
                ErrorReason::UnsupportedVersion,
                "/lml",
                "unsupported document version",
            )),
        }
    }
}
fn model(n: &Node, id: Option<&str>, at: &str) -> ContractResult<()> {
    keys(
        n,
        &[
            "lml",
            "model",
            "register",
            "attributes",
            "mappings",
            "ordered_content",
        ],
        at,
    )?;
    if field(n, "lml", at)?.integer() != Some(2) {
        return Err(failure(
            ErrorReason::UnsupportedVersion,
            &pointer(at, "lml"),
            "v2 document required",
        ));
    }
    let name = text(n, "model", at)?;
    let reg = text(n, "register", at)?;
    if !portable_name(name) {
        return Err(invalid(&pointer(at, "model"), "unportable model name"));
    }
    if !register(reg) {
        return Err(invalid(
            &pointer(at, "register"),
            "invalid effective register",
        ));
    }
    if id.is_some_and(|id| id != format!("{name}@{reg}")) {
        return Err(failure(
            ErrorReason::DuplicateId,
            at,
            "model key does not match declared identity",
        ));
    }
    let attrs = field(n, "attributes", at)?;
    let mut names = BTreeSet::new();
    // Non-collection builtin `hash` attributes: the only `any_attribute` targets.
    let mut hashes = BTreeSet::new();
    for (i, attr) in array(attrs, &pointer(at, "attributes"))?.iter().enumerate() {
        let loc = format!("{at}/attributes/{i}");
        let result = (|| {
            let name = text(attr, "name", &loc)?;
            attribute_name(name, &pointer(&loc, "name"))?;
            if name == "__lml_content" {
                return Err(failure(
                    ErrorReason::ReservedMember,
                    &pointer(&loc, "name"),
                    "reserved ordered content member",
                ));
            }
            if !names.insert(name) {
                return Err(failure(
                    ErrorReason::DuplicateId,
                    &pointer(&loc, "name"),
                    "duplicate attribute",
                ));
            }
            attribute(attr, &loc)?;
            let hash = attr.get("type").is_some_and(|t| {
                t.get("kind").and_then(Node::string) == Some("builtin")
                    && t.get("name").and_then(Node::string) == Some("hash")
            }) && attr
                .get("collection")
                .is_none_or(|c| c == &Node::Bool(false));
            if hash {
                hashes.insert(name);
            }
            Ok(())
        })();
        result.map_err(|mut e: Box<crate::contracts::ContractError>| {
            e.model_id = Some(format!("{name}@{reg}"));
            e.attribute = attr.get("name").and_then(Node::string).map(str::to_owned);
            e
        })?;
    }
    if let Some(order) = n.get("ordered_content") {
        let loc = pointer(at, "ordered_content");
        keys(order, &["member", "version"], &loc)?;
        if text(order, "member", &loc)? != "__lml_content"
            || field(order, "version", &loc)?.integer() != Some(1)
        {
            return Err(failure(
                ErrorReason::InvalidOrderedContent,
                &loc,
                "invalid ordered content declaration",
            ));
        }
    }
    let empty = Node::Object(vec![]);
    let mappings = n.get("mappings").unwrap_or(&empty);
    object(mappings, &pointer(at, "mappings"))?;
    for (format, mapping) in mappings.object().unwrap() {
        let loc = pointer(&pointer(at, "mappings"), format);
        let result = match format.as_str() {
            "xml" => xml(mapping, &names, &hashes, &loc),
            // toml validates like json/yaml; the typed view refuses it by name.
            "json" | "yaml" | "toml" => key_mapping(mapping, &names, &loc),
            _ => Err(failure(
                ErrorReason::UnsupportedFormat,
                &loc,
                "unsupported mapping format",
            )),
        };
        result.map_err(|mut e| {
            e.format = Some(format.clone());
            e.model_id = Some(format!("{name}@{reg}"));
            e
        })?;
    }
    Ok(())
}
