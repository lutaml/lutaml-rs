//! Strict portable graph loading and immutable canonical identities.
//! Imported directly by module-card tests until public wiring in card08f.
mod tree;
mod validate;
mod yaml;

use crate::contracts::*;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
pub use tree::Node;

pub(crate) fn pointer(at: &str, member: &str) -> String {
    format!("{at}/{}", member.replace('~', "~0").replace('/', "~1"))
}
pub(crate) fn failure(
    reason: ErrorReason,
    location: &str,
    detail: impl Into<String>,
) -> Box<ContractError> {
    Box::new(ContractError {
        reason,
        model_id: None,
        attribute: None,
        format: None,
        rule: None,
        location: location.into(),
        klass: None,
        detail: Some(detail.into()),
    })
}

/// Owns a validated copy. No mutable tree or model accessor can invalidate its
/// pinned canonical bytes/checksum. Callers receive a fresh typed graph.
#[derive(Clone, Debug)]
pub struct Snapshot {
    tree: Node,
    canonical: String,
    checksum: String,
}
impl Snapshot {
    pub fn tree(&self) -> &Node {
        &self.tree
    }
    pub fn canonical_json(&self) -> &str {
        &self.canonical
    }
    pub fn checksum(&self) -> &str {
        &self.checksum
    }
    pub fn verify_checksum(&self, expected: &str) -> ContractResult<()> {
        if expected != self.checksum {
            return Err(failure(
                ErrorReason::ChecksumMismatch,
                "",
                format!("expected {expected}, got {}", self.checksum),
            ));
        }
        Ok(())
    }
    pub fn bundle(&self) -> ContractResult<Bundle> {
        if self.tree.get("lml_bundle").is_none() {
            return Err(failure(
                ErrorReason::InvalidDocument,
                "",
                "snapshot is not a bundle",
            ));
        }
        let wire: serde_json::Value = serde_json::from_str(&self.canonical)
            .map_err(|e| failure(ErrorReason::InvalidDocument, "", e.to_string()))?;
        // The typed view has no TOML mapping: refuse it by name instead of
        // silently dropping a declaration the validator accepted (Ruby accepts
        // `mappings.toml`; the schema identity still covers it).
        for (id, model) in wire["models"].as_object().unwrap() {
            if model["mappings"].get("toml").is_some() {
                let mut error = failure(
                    ErrorReason::UnsupportedFormat,
                    &format!("{}/mappings/toml", pointer("/models", id)),
                    "the typed Rust graph has no toml mapping",
                );
                error.model_id = Some(id.clone());
                error.format = Some("toml".into());
                return Err(error);
            }
        }
        serde_json::from_value(wire).map_err(|e| {
            failure(
                ErrorReason::UnsupportedMapping,
                "",
                format!("typed graph cannot represent declaration: {e}"),
            )
        })
    }
}

pub fn canonical_json(node: &Node) -> ContractResult<String> {
    serde_json::to_string(&node.sorted())
        .map_err(|e| failure(ErrorReason::InvalidDocument, "", e.to_string()))
}
pub fn checksum(node: &Node) -> ContractResult<String> {
    Ok(format!(
        "sha256:{:x}",
        Sha256::digest(canonical_json(node)?.as_bytes())
    ))
}

/// Load a validated immutable schema snapshot.
///
/// Legacy v1 YAML floating scalars refuse with `UnsupportedType`: a YAML float's
/// spelling cannot reproduce version-dependent Ruby JSON checksum formatting.
/// Load producer-canonical v1 JSON instead, or use v2 exact decimal strings.
/// Arbitrary-size legacy YAML integers remain lossless; v2 never admits floats.
pub fn load(bytes: &[u8], format: InputFormat) -> ContractResult<Snapshot> {
    let mut tree = tree::parse(bytes, format)?;
    validate::validate(&tree)?;
    // Existing v1 Document.from_hash reconstructs these root fields, ignoring
    // unknown legacy keys and omitting nil register; never rewrite its version.
    if tree.get("lml").and_then(Node::integer) == Some(1) {
        let mut fields = Vec::new();
        for key in ["lml", "model", "register", "attributes", "mappings"] {
            if let Some(value) = tree.get(key) {
                if key != "register" || value != &Node::Null {
                    fields.push((key.into(), value.clone()));
                }
            } else if key == "attributes" {
                fields.push((key.into(), Node::Array(vec![])));
            } else if key == "mappings" {
                fields.push((key.into(), Node::Object(vec![])));
            }
        }
        tree = Node::Object(fields);
        {
            let name = if format == InputFormat::Yaml {
                "yaml"
            } else {
                "json"
            };
            reject_legacy_floats(&tree, "", format).map_err(|mut e| {
                e.format = Some(name.into());
                e.klass = tree.get("model").and_then(Node::string).map(str::to_owned);
                let parts: Vec<_> = e.location.split('/').collect();
                if parts.get(1) == Some(&"attributes") {
                    e.attribute = parts
                        .get(2)
                        .and_then(|i| i.parse::<usize>().ok())
                        .and_then(|i| {
                            tree.get("attributes")
                                .and_then(Node::array)
                                .and_then(|attrs| attrs.get(i))
                        })
                        .and_then(|attr| attr.get("name"))
                        .and_then(Node::string)
                        .map(str::to_owned);
                }
                e
            })?;
        }
    }
    let legacy = tree.get("lml").and_then(Node::integer) == Some(1);
    let canonical = if legacy {
        serde_json::to_string(&tree)
            .map_err(|e| failure(ErrorReason::InvalidDocument, "", e.to_string()))?
    } else {
        canonical_json(&tree)?
    };
    let semantic = if tree.get("lml_bundle").is_some() {
        Node::Object(
            tree.object()
                .unwrap()
                .iter()
                .filter(|(k, _)| k != "provenance")
                .cloned()
                .collect(),
        )
    } else {
        tree.clone()
    };
    let checksum = if legacy {
        format!("sha256:{:x}", Sha256::digest(canonical.as_bytes()))
    } else {
        checksum(&semantic)?
    };
    Ok(Snapshot {
        tree,
        canonical,
        checksum,
    })
}
/// A v1 identity is the Ruby `JSON.generate` of the parsed hash, so a float
/// is safe only when its spelling is what every Ruby JSON generator prints:
/// fixed notation and the shortest round-trip digits (`1.5`, `100.0`,
/// `0.0001`, `-0.0`). `1.50`, `1e2` and every exponent form re-render
/// differently (and differently per Ruby/json version), so they would change
/// the checksum; they refuse here, as every YAML float does.
fn reject_legacy_floats(node: &Node, at: &str, format: InputFormat) -> ContractResult<()> {
    match node {
        Node::Number(number) => {
            let text = number.to_string();
            if text.contains(['.', 'e', 'E']) {
                let stable = format == InputFormat::Json
                    && !text.contains(['e', 'E'])
                    && text
                        .parse::<f64>()
                        .is_ok_and(|v| v.is_finite() && crate::values::float_wire(v) == text);
                if !stable {
                    let hint = if format == InputFormat::Yaml {
                        "legacy YAML float checksum is ambiguous; load producer-canonical v1 JSON or use v2 exact decimal strings"
                    } else {
                        "legacy JSON float is not in Ruby's canonical JSON spelling (fixed notation, shortest digits); its checksum would differ from Ruby's"
                    };
                    return Err(failure(ErrorReason::UnsupportedType, at, hint));
                }
            }
        }
        Node::Array(values) => {
            for (i, value) in values.iter().enumerate() {
                reject_legacy_floats(value, &pointer(at, &i.to_string()), format)?;
            }
        }
        Node::Object(values) => {
            for (key, value) in values {
                reject_legacy_floats(value, &pointer(at, key), format)?;
            }
        }
        _ => {}
    }
    Ok(())
}

pub fn load_bundle(bytes: &[u8], format: InputFormat) -> ContractResult<Bundle> {
    load(bytes, format)?.bundle()
}
#[derive(Clone, Copy, Debug, Default)]
pub struct Loader;
impl BundleLoader for Loader {
    fn load_bundle(&self, bytes: &[u8], format: InputFormat) -> ContractResult<Bundle> {
        load_bundle(bytes, format)
    }
}

/// Validate instance mixed-content references against actual data, not a fixed
/// sequence. Every mapped member occurrence must be represented exactly once.
pub fn validate_ordered_content(
    model: &ModelDocument,
    data: &std::collections::BTreeMap<String, NeutralValue>,
    tokens: &[ContentToken],
) -> ContractResult<()> {
    if model.ordered_content.is_none() {
        return Err(failure(
            ErrorReason::InvalidOrderedContent,
            "/__lml_content",
            "model does not declare ordered content",
        ));
    }
    let mut expected = BTreeSet::new();
    let mut kinds = std::collections::BTreeMap::new();
    if let Some(xml) = &model.mappings.xml {
        for rule in &xml.rules {
            if matches!(
                rule.kind,
                XmlRuleKind::Attribute | XmlRuleKind::AnyAttribute
            ) {
                continue;
            }
            kinds.insert(rule.to.as_str(), &rule.kind);
            if let Some(value) = data.get(&rule.to) {
                let count = match value {
                    NeutralValue::Array(v) => v.len(),
                    NeutralValue::Null => 0,
                    _ => 1,
                };
                for index in 0..count {
                    expected.insert((rule.to.as_str(), index as u64));
                }
            }
        }
    }
    let mut seen = BTreeSet::new();
    for (index, token) in tokens.iter().enumerate() {
        let at = format!("/__lml_content/{index}");
        let (attribute, slot, is_element) = match token {
            ContentToken::Element { attribute, index } => (attribute.as_str(), *index, true),
            ContentToken::Text {
                source: TextSource::Member { attribute, index },
            }
            | ContentToken::Cdata {
                source: TextSource::Member { attribute, index },
            } => (attribute.as_str(), *index, false),
            ContentToken::Text {
                source: TextSource::Literal { .. },
            }
            | ContentToken::Cdata {
                source: TextSource::Literal { .. },
            } => continue,
        };
        let member = (attribute, slot);
        if !expected.contains(&member)
            || !seen.insert(member)
            || is_element != matches!(kinds.get(attribute), Some(XmlRuleKind::Element))
        {
            let mut err = failure(
                ErrorReason::InvalidOrderedContent,
                &at,
                "duplicate, missing, out-of-range or inconsistent member token",
            );
            err.model_id = Some(format!("{}@{}", model.model, model.register));
            err.attribute = Some(attribute.into());
            return Err(err);
        }
    }
    if seen != expected {
        return Err(failure(
            ErrorReason::InvalidOrderedContent,
            "/__lml_content",
            "mapped member occurrence omitted",
        ));
    }
    Ok(())
}

pub fn validate_type_bindings(bundle: &Bundle, bindings: &TypeBindings) -> ContractResult<()> {
    for model in bundle.models.values() {
        for attr in &model.attributes {
            if let TypeRef::Value { id } = &attr.value_type {
                if !bindings.contains_key(id) {
                    let mut err = failure(
                        ErrorReason::MissingTypeBinding,
                        "/value_types",
                        format!("missing trusted binding for {id}"),
                    );
                    err.model_id = Some(format!("{}@{}", model.model, model.register));
                    err.attribute = Some(attr.name.clone());
                    return Err(err);
                }
            }
        }
    }
    Ok(())
}

/// Metadata-only validation before compile-time PARG lowering. No grammar is
/// executed and no application behavior is inferred from capture names.
pub fn validate_parg_bindings(
    bundle: &Bundle,
    bindings: &[PargClassBinding],
) -> ContractResult<()> {
    let mut rules = BTreeSet::new();
    for (index, binding) in bindings.iter().enumerate() {
        let at = format!("/parg_bindings/{index}");
        if binding.rule.is_empty() || !rules.insert(&binding.rule) {
            return Err(failure(
                ErrorReason::DuplicateId,
                &at,
                "empty or duplicate PARG rule binding",
            ));
        }
        let model = bundle
            .models
            .get(&binding.model_id)
            .ok_or_else(|| failure(ErrorReason::DanglingReference, &at, "unknown PARG model ID"))?;
        for (capture, member) in &binding.captures {
            if capture.is_empty() || !model.attributes.iter().any(|attr| &attr.name == member) {
                let mut err = failure(
                    ErrorReason::DanglingReference,
                    &pointer(&at, capture),
                    "unknown capture member",
                );
                err.model_id = Some(binding.model_id.clone());
                return Err(err);
            }
        }
    }
    Ok(())
}
