use super::{failure, pointer};
use crate::contracts::{ContractResult, ErrorReason, InputFormat};
use serde::Serialize;
use std::collections::HashSet;

/// An ordered neutral syntax tree: JSON object order is retained until explicit
/// canonicalization, and duplicate keys are rejected before a map can erase them.
#[derive(Clone, Debug, PartialEq)]
pub enum Node {
    Null,
    Bool(bool),
    Number(serde_json::Number),
    String(String),
    Array(Vec<Node>),
    Object(Vec<(String, Node)>),
}
impl Node {
    pub fn get(&self, key: &str) -> Option<&Self> {
        match self {
            Self::Object(entries) => entries
                .iter()
                .find(|(name, _)| name == key)
                .map(|(_, value)| value),
            _ => None,
        }
    }
    pub fn string(&self) -> Option<&str> {
        if let Self::String(s) = self {
            Some(s)
        } else {
            None
        }
    }
    pub fn array(&self) -> Option<&[Self]> {
        if let Self::Array(a) = self {
            Some(a)
        } else {
            None
        }
    }
    pub fn object(&self) -> Option<&[(String, Self)]> {
        if let Self::Object(m) = self {
            Some(m)
        } else {
            None
        }
    }
    pub fn integer(&self) -> Option<u64> {
        if let Self::Number(n) = self {
            n.as_u64()
        } else {
            None
        }
    }
    pub fn sorted(&self) -> Self {
        match self {
            Self::Object(entries) => {
                let mut entries: Vec<_> = entries
                    .iter()
                    .map(|(k, v)| (k.clone(), v.sorted()))
                    .collect();
                entries.sort_by(|a, b| a.0.cmp(&b.0));
                Self::Object(entries)
            }
            Self::Array(entries) => Self::Array(entries.iter().map(Self::sorted).collect()),
            other => other.clone(),
        }
    }
}
impl Serialize for Node {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::{SerializeMap, SerializeSeq};
        match self {
            Self::Null => s.serialize_unit(),
            Self::Bool(v) => s.serialize_bool(*v),
            Self::Number(v) => v.serialize(s),
            Self::String(v) => s.serialize_str(v),
            Self::Array(v) => {
                let mut seq = s.serialize_seq(Some(v.len()))?;
                for v in v {
                    seq.serialize_element(v)?;
                }
                seq.end()
            }
            Self::Object(v) => {
                let mut map = s.serialize_map(Some(v.len()))?;
                for (k, v) in v {
                    map.serialize_entry(k, v)?;
                }
                map.end()
            }
        }
    }
}

pub fn parse(bytes: &[u8], format: InputFormat) -> ContractResult<Node> {
    let text = std::str::from_utf8(bytes)
        .map_err(|e| failure(ErrorReason::InvalidDocument, "", e.to_string()))?;
    if text.starts_with('\u{feff}') {
        return Err(failure(
            ErrorReason::InvalidDocument,
            "",
            "BOM is not allowed",
        ));
    }
    match format {
        InputFormat::Json => {
            let mut parser = Json {
                bytes: text.as_bytes(),
                pos: 0,
            };
            let node = parser.value("", 0)?;
            parser.space();
            if parser.pos != parser.bytes.len() {
                return Err(failure(
                    ErrorReason::InvalidDocument,
                    "",
                    "trailing JSON data",
                ));
            }
            Ok(node)
        }
        InputFormat::Yaml => super::yaml::parse(text.as_bytes()),
    }
}

/// Containers nested deeper than this are refused (Ruby DocumentValidator::MAX_DEPTH).
pub(crate) const MAX_DEPTH: usize = 100;
pub(crate) fn nesting(at: &str, depth: usize) -> ContractResult<()> {
    if depth >= MAX_DEPTH {
        return Err(failure(
            ErrorReason::InvalidDocument,
            at,
            format!("nesting deeper than {MAX_DEPTH} levels"),
        ));
    }
    Ok(())
}

struct Json<'a> {
    bytes: &'a [u8],
    pos: usize,
}
impl Json<'_> {
    fn space(&mut self) {
        while self
            .bytes
            .get(self.pos)
            .is_some_and(|b| matches!(b, b' ' | b'\t' | b'\r' | b'\n'))
        {
            self.pos += 1;
        }
    }
    fn string(&mut self, at: &str) -> ContractResult<String> {
        let start = self.pos;
        self.pos += 1;
        let mut escaped = false;
        while let Some(&byte) = self.bytes.get(self.pos) {
            self.pos += 1;
            if byte == b'"' && !escaped {
                return serde_json::from_slice(&self.bytes[start..self.pos])
                    .map_err(|e| failure(ErrorReason::InvalidDocument, at, e.to_string()));
            }
            if byte == b'\\' {
                escaped = !escaped;
            } else {
                escaped = false;
            }
        }
        Err(failure(
            ErrorReason::InvalidDocument,
            at,
            "unterminated JSON string",
        ))
    }
    fn value(&mut self, at: &str, depth: usize) -> ContractResult<Node> {
        self.space();
        match self.bytes.get(self.pos) {
            Some(b'"') => Ok(Node::String(self.string(at)?)),
            Some(b'{') => {
                nesting(at, depth)?;
                self.pos += 1;
                self.space();
                let mut entries = Vec::new();
                let mut names = HashSet::new();
                if self.bytes.get(self.pos) == Some(&b'}') {
                    self.pos += 1;
                    return Ok(Node::Object(entries));
                }
                loop {
                    self.space();
                    if self.bytes.get(self.pos) != Some(&b'"') {
                        return Err(failure(
                            ErrorReason::InvalidDocument,
                            at,
                            "object key must be a string",
                        ));
                    }
                    let key = self.string(at)?;
                    let child = pointer(at, &key);
                    if !names.insert(key.clone()) {
                        return Err(failure(
                            ErrorReason::InvalidDocument,
                            &child,
                            "duplicate object key",
                        ));
                    }
                    self.space();
                    if self.bytes.get(self.pos) != Some(&b':') {
                        return Err(failure(
                            ErrorReason::InvalidDocument,
                            &child,
                            "expected colon",
                        ));
                    }
                    self.pos += 1;
                    entries.push((key, self.value(&child, depth + 1)?));
                    self.space();
                    match self.bytes.get(self.pos) {
                        Some(b'}') => {
                            self.pos += 1;
                            break;
                        }
                        Some(b',') => self.pos += 1,
                        _ => {
                            return Err(failure(
                                ErrorReason::InvalidDocument,
                                at,
                                "expected comma or closing brace",
                            ))
                        }
                    }
                }
                Ok(Node::Object(entries))
            }
            Some(b'[') => {
                nesting(at, depth)?;
                self.pos += 1;
                self.space();
                let mut entries = Vec::new();
                if self.bytes.get(self.pos) == Some(&b']') {
                    self.pos += 1;
                    return Ok(Node::Array(entries));
                }
                loop {
                    entries.push(self.value(&pointer(at, &entries.len().to_string()), depth + 1)?);
                    self.space();
                    match self.bytes.get(self.pos) {
                        Some(b']') => {
                            self.pos += 1;
                            break;
                        }
                        Some(b',') => self.pos += 1,
                        _ => {
                            return Err(failure(
                                ErrorReason::InvalidDocument,
                                at,
                                "expected comma or closing bracket",
                            ))
                        }
                    }
                }
                Ok(Node::Array(entries))
            }
            Some(_) => {
                let start = self.pos;
                while self
                    .bytes
                    .get(self.pos)
                    .is_some_and(|b| !b.is_ascii_whitespace() && !b",]}".contains(b))
                {
                    self.pos += 1;
                }
                let token = std::str::from_utf8(&self.bytes[start..self.pos]).unwrap();
                match token {
                    "null" => Ok(Node::Null),
                    "true" => Ok(Node::Bool(true)),
                    "false" => Ok(Node::Bool(false)),
                    // Ruby parses "-0" as the integer 0, so its identity is "0".
                    "-0" => Ok(Node::Number(0.into())),
                    _ => token
                        .parse::<serde_json::Number>()
                        .map(Node::Number)
                        .map_err(|e| failure(ErrorReason::InvalidDocument, at, e.to_string())),
                }
            }
            None => Err(failure(
                ErrorReason::InvalidDocument,
                at,
                "unexpected end of JSON",
            )),
        }
    }
}
