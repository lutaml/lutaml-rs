use super::*;
use std::collections::HashSet;

pub(super) fn builtin(n: &Node, at: &str) -> ContractResult<()> {
    let name = string(n, at)?;
    if name == "double" {
        return Err(failure(
            ErrorReason::UnsupportedType,
            at,
            "double is not a portable type; declare float",
        ));
    }
    if !BUILTINS.contains(&name) {
        return Err(failure(
            ErrorReason::UnknownType,
            at,
            format!("unknown builtin {name}"),
        ));
    }
    Ok(())
}
pub(super) fn type_ref(n: &Node, at: &str) -> ContractResult<()> {
    match text(n, "kind", at)? {
        "builtin" => {
            keys(n, &["kind", "name"], at)?;
            builtin(field(n, "name", at)?, &pointer(at, "name"))
        }
        "model" | "value" => {
            keys(n, &["kind", "id"], at)?;
            text(n, "id", at)?;
            Ok(())
        }
        _ => Err(failure(
            ErrorReason::UnknownType,
            &pointer(at, "kind"),
            "unknown type kind",
        )),
    }
}
pub(super) fn attribute(n: &Node, at: &str) -> ContractResult<()> {
    keys(
        n,
        &[
            "name",
            "type",
            "required",
            "collection",
            "default",
            "initialize_empty",
            "discriminator",
            "values",
        ],
        at,
    )?;
    let typ = field(n, "type", at)?;
    type_ref(typ, &pointer(at, "type"))?;
    for key in ["required", "initialize_empty"] {
        if let Some(value) = n.get(key) {
            boolean(value, &pointer(at, key))?;
        }
    }
    let collection = n.get("collection").unwrap_or(&Node::Bool(false));
    if collection != &Node::Bool(false) {
        keys(
            collection,
            &["min", "max", "class"],
            &pointer(at, "collection"),
        )?;
        bounds(collection, &pointer(at, "collection"))?;
        if let Some(c) = collection.get("class") {
            string(c, &format!("{at}/collection/class"))?;
        }
    }
    let absent = Node::Object(vec![("present".into(), Node::Bool(false))]);
    let default = n.get("default").unwrap_or(&absent);
    let loc = pointer(at, "default");
    if default.get("proc").is_some() || default.get("lambda").is_some() {
        return Err(failure(
            ErrorReason::LambdaDefault,
            &loc,
            "executable default",
        ));
    }
    keys(default, &["present", "value"], &loc)?;
    match (field(default, "present", &loc)?, default.get("value")) {
        (Node::Bool(false), None) => {}
        (Node::Bool(true), Some(value)) => {
            // A float default is one canonical float string (Ruby's float_default):
            // never null, never a collection of floats.
            let float = typ.get("kind").and_then(Node::string) == Some("builtin")
                && typ.get("name").and_then(Node::string) == Some("float");
            if float && (value == &Node::Null || collection != &Node::Bool(false)) {
                return Err(invalid(
                    &pointer(&loc, "value"),
                    "float default must be a canonical float string",
                ));
            }
            if collection == &Node::Bool(false) {
                typed_value(value, typ, &pointer(&loc, "value"))?;
            } else if value != &Node::Null {
                let items = array(value, &pointer(&loc, "value"))?;
                check_count(items.len(), collection, &loc)?;
                for (i, item) in items.iter().enumerate() {
                    typed_value(item, typ, &format!("{loc}/value/{i}"))?;
                }
            }
        }
        _ => {
            return Err(failure(
                ErrorReason::NonSerializableDefault,
                &loc,
                "invalid default presence/value",
            ))
        }
    }
    if let Some(values) = n.get("values") {
        let mut keys = HashSet::new();
        let mut seen = Vec::new();
        for (i, value) in array(values, &pointer(at, "values"))?.iter().enumerate() {
            enum_value(value, typ, &format!("{at}/values/{i}"))?;
            // Hashed identity keeps the duplicate check linear in the list.
            let identity = serde_json::to_string(value).expect("tree nodes serialize");
            if !keys.insert(identity) {
                return Err(invalid(&pointer(at, "values"), "duplicate enum value"));
            }
            seen.push(value);
        }
        if let Some(value) = default.get("value") {
            if value != &Node::Null
                && collection == &Node::Bool(false)
                && !seen.iter().any(|entry| enum_equal(value, entry, typ))
            {
                return Err(failure(
                    ErrorReason::NonSerializableDefault,
                    &loc,
                    "default not in enum",
                ));
            }
        }
    }
    if let Some(d) = n.get("discriminator") {
        discriminator(d, &pointer(at, "discriminator"))?;
    }
    Ok(())
}
pub(super) fn check_count(len: usize, bounds: &Node, at: &str) -> ContractResult<()> {
    let len = len as u64;
    let min = bounds.get("min").and_then(Node::integer).unwrap();
    let max = bounds.get("max").and_then(Node::integer);
    if len < min || max.is_some_and(|m| len > m) {
        return Err(failure(
            ErrorReason::NonSerializableDefault,
            at,
            "collection default violates bounds",
        ));
    }
    Ok(())
}
/// The one lexical rule for exact numbers, shared with `values::ExactDecimal`
/// and `values::ExactInteger` (see `values::is_exact_decimal`/`is_exact_integer`).
pub(super) fn exact(s: &str, decimal: bool) -> bool {
    if decimal {
        crate::values::is_exact_decimal(s)
    } else {
        crate::values::is_exact_integer(s)
    }
}
fn is_integer_name(typ: &Node) -> bool {
    typ.get("name")
        .and_then(Node::string)
        .is_some_and(|n| INTEGER_BUILTINS.contains(&n))
}
pub(super) fn typed_value(value: &Node, typ: &Node, at: &str) -> ContractResult<()> {
    if value == &Node::Null {
        return Ok(());
    }
    let valid = match typ.get("kind").and_then(Node::string) {
        Some("builtin") => match typ.get("name").and_then(Node::string).unwrap() {
            "boolean" => matches!(value, Node::Bool(_)),
            // Ruby exports integer defaults as JSON numbers and refuses strings.
            name if INTEGER_BUILTINS.contains(&name) => {
                matches!(value, Node::Number(v) if v.as_i64().is_some() || v.as_u64().is_some())
            }
            "float" => value.string().is_some_and(crate::values::canonical_float),
            "hash" => matches!(value, Node::Object(_)),
            // Every other builtin (decimal, rational, date, time, qname, ...) is its
            // string spelling, exactly as Ruby's exporter writes it (a qname is
            // "prefix:local" or "{uri}local", never an object; decimal and rational
            // are shape-checked only, like Ruby and TS). Float keeps its own rule above.
            _ => matches!(value, Node::String(_)),
        },
        Some("model") => matches!(value, Node::Object(_)),
        Some("value") => true,
        _ => false,
    };
    if !valid {
        return Err(failure(
            ErrorReason::NonSerializableDefault,
            at,
            "default/enum value does not match declared type",
        ));
    }
    Ok(())
}
fn enum_equal(a: &Node, b: &Node, typ: &Node) -> bool {
    if a == b {
        return true;
    }
    if !is_integer_name(typ) {
        return false;
    }
    let integer = |value: &Node| -> Option<num_bigint::BigInt> {
        match value {
            Node::String(s) => s.parse().ok(),
            Node::Number(v) => v
                .as_i64()
                .map(num_bigint::BigInt::from)
                .or_else(|| v.as_u64().map(num_bigint::BigInt::from)),
            _ => None,
        }
    };
    matches!((integer(a),integer(b)),(Some(a),Some(b))if a==b)
}
/// Integer enum literals are typed schema metadata, not instance wire values.
/// Their bounded JSON integer spelling remains numeric in the immutable tree.
pub(super) fn enum_value(value: &Node, typ: &Node, at: &str) -> ContractResult<()> {
    if is_integer_name(typ)
        && matches!(value,Node::Number(v) if v.as_i64().is_some()||v.as_u64().is_some())
    {
        return Ok(());
    }
    typed_value(value, typ, at)
}
pub(super) fn value_type(n: &Node, at: &str) -> ContractResult<()> {
    keys(n, &["base", "facets"], at)?;
    builtin(field(n, "base", at)?, &pointer(at, "base"))?;
    let facets = field(n, "facets", at)?;
    for (key, value) in object(facets, &pointer(at, "facets"))? {
        let loc = pointer(&pointer(at, "facets"), key);
        match key.as_str() {
            "length" | "min_length" | "max_length" | "total_digits" | "fraction_digits" => {
                bounded(value, &loc)?;
            }
            "pattern" | "white_space" => {
                string(value, &loc)?;
            }
            "min_inclusive" | "max_inclusive" | "min_exclusive" | "max_exclusive" => {
                if !value.string().is_some_and(|s| exact(s, true)) {
                    return Err(invalid(&loc, "exact facet bound requires decimal string"));
                }
            }
            "enumeration" => {
                array(value, &loc)?;
            }
            _ => return Err(failure(ErrorReason::UnsupportedType, &loc, "unknown facet")),
        }
    }
    for (min, max) in [
        ("min_length", "max_length"),
        ("fraction_digits", "total_digits"),
    ] {
        if let (Some(min), Some(max)) = (
            facets.get(min).and_then(Node::integer),
            facets.get(max).and_then(Node::integer),
        ) {
            if min > max {
                return Err(invalid(&pointer(at, "facets"), "inconsistent facet bounds"));
            }
        }
    }
    Ok(())
}
// Model defaults are neutral instance data, validated in the resolved graph
// context. A recursive schema is legal; finite default data drives recursion.
pub(super) fn validate_member(
    value: &Node,
    attr: &Node,
    bundle: &Node,
    at: &str,
) -> ContractResult<()> {
    if value == &Node::Null {
        return Ok(());
    }
    let typ = attr.get("type").unwrap();
    let validate_one = |value: &Node, at: &str| -> ContractResult<()> {
        if value == &Node::Null {
            return Ok(());
        }
        let resolved = if let Some(d) = attr.get("discriminator") {
            let member = d.get("attribute").unwrap().string().unwrap();
            let tag = value.get(member).ok_or_else(|| {
                failure(
                    ErrorReason::UnknownDiscriminator,
                    &pointer(at, member),
                    "default discriminator missing",
                )
            })?;
            let wire = match tag {
                Node::String(s) => s.clone(),
                Node::Bool(v) => v.to_string(),
                Node::Number(v) => v.to_string(),
                _ => {
                    return Err(failure(
                        ErrorReason::UnknownDiscriminator,
                        &pointer(at, member),
                        "invalid discriminator value",
                    ))
                }
            };
            Some(
                d.get("cases")
                    .unwrap()
                    .get(&wire)
                    .and_then(Node::string)
                    .ok_or_else(|| {
                        failure(
                            ErrorReason::UnknownDiscriminator,
                            &pointer(at, member),
                            "unknown default discriminator",
                        )
                    })?,
            )
        } else if typ.get("kind").and_then(Node::string) == Some("model") {
            typ.get("id").and_then(Node::string)
        } else {
            None
        };
        if let Some(id) = resolved {
            let model = bundle.get("models").unwrap().get(id).ok_or_else(|| {
                failure(ErrorReason::DanglingReference, at, "unknown default model")
            })?;
            let attrs = model
                .get("attributes")
                .and_then(Node::array)
                .ok_or_else(|| invalid(at, "invalid target model"))?;
            for (key, _) in object(value, at)? {
                if !attrs
                    .iter()
                    .any(|a| a.get("name").and_then(Node::string) == Some(key))
                {
                    return Err(failure(
                        ErrorReason::NonSerializableDefault,
                        &pointer(at, key),
                        "unknown model default member",
                    ));
                }
            }
            for attr in attrs {
                let name = text(attr, "name", at)?;
                if let Some(value) = value.get(name) {
                    validate_member(value, attr, bundle, &pointer(at, name))?;
                } else if attr.get("required") == Some(&Node::Bool(true))
                    && attr.get("default").and_then(|d| d.get("present")) != Some(&Node::Bool(true))
                {
                    return Err(failure(
                        ErrorReason::NonSerializableDefault,
                        &pointer(at, name),
                        "required model default member missing",
                    ));
                }
            }
        } else if typ.get("kind").and_then(Node::string) == Some("value") {
            let id = typ.get("id").unwrap().string().unwrap();
            let definition =
                bundle.get("value_types").unwrap().get(id).ok_or_else(|| {
                    failure(ErrorReason::DanglingReference, at, "unknown value type")
                })?;
            let base = Node::Object(vec![
                ("kind".into(), Node::String("builtin".into())),
                ("name".into(), definition.get("base").unwrap().clone()),
            ]);
            typed_value(value, &base, at)?;
        } else {
            typed_value(value, typ, at)?;
        }
        if let Some(values) = attr.get("values") {
            if !values
                .array()
                .unwrap()
                .iter()
                .any(|entry| enum_equal(value, entry, typ))
            {
                return Err(failure(
                    ErrorReason::NonSerializableDefault,
                    at,
                    "value is not in enum",
                ));
            }
        }
        Ok(())
    };
    if attr
        .get("collection")
        .is_none_or(|v| v == &Node::Bool(false))
    {
        validate_one(value, at)
    } else {
        let items = array(value, at)?;
        check_count(items.len(), attr.get("collection").unwrap(), at)?;
        for (i, item) in items.iter().enumerate() {
            validate_one(item, &pointer(at, &i.to_string()))?;
        }
        Ok(())
    }
}
