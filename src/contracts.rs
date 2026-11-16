//! Wire types shared by loaders, emitters, generated runtimes and wasm adapters.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;

pub type ModelId = String;
pub type ValueTypeId = String;
pub type SourceMap = BTreeMap<String, String>;
pub type ContractResult<T> = Result<T, Box<ContractError>>;
pub type ExactInteger = num_bigint::BigInt;
pub type ExactRational = num_rational::BigRational;

/// Neutral data deliberately has no floating-point variant. Exact values cross
/// the portable boundary as strings; these integers are structural metadata.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(untagged)]
pub enum NeutralValue {
    Null,
    Boolean(bool),
    Signed(i64),
    Unsigned(u64),
    String(String),
    Array(Vec<NeutralValue>),
    Object(BTreeMap<String, NeutralValue>),
}

// Decode through the arbitrary-precision JSON number representation. Serde's
// untagged fallback could otherwise mistake its private number map for an
// ordinary object and admit fractional structural metadata.
impl<'de> Deserialize<'de> for NeutralValue {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        fn convert(value: serde_json::Value) -> Result<NeutralValue, &'static str> {
            use serde_json::Value;
            match value {
                Value::Null => Ok(NeutralValue::Null),
                Value::Bool(v) => Ok(NeutralValue::Boolean(v)),
                Value::String(v) => Ok(NeutralValue::String(v)),
                Value::Number(v) => {
                    if let Some(v) = v.as_i64() {
                        Ok(NeutralValue::Signed(v))
                    } else if let Some(v) = v.as_u64() {
                        Ok(NeutralValue::Unsigned(v))
                    } else {
                        Err("structural numbers must be bounded integers; exact values require strings")
                    }
                }
                Value::Array(v) => v
                    .into_iter()
                    .map(convert)
                    .collect::<Result<Vec<_>, _>>()
                    .map(NeutralValue::Array),
                Value::Object(v) => v
                    .into_iter()
                    .map(|(key, value)| convert(value).map(|value| (key, value)))
                    .collect::<Result<BTreeMap<_, _>, _>>()
                    .map(NeutralValue::Object),
            }
        }
        convert(serde_json::Value::deserialize(d)?).map_err(serde::de::Error::custom)
    }
}

/// Instance fields distinguish absent, explicit null, and a present value.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Presence<T> {
    #[default]
    Absent,
    Null,
    Value(T),
}

/// Defaults use the portable boolean tag, including an explicit null value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DefaultValue {
    Absent,
    Present(NeutralValue),
}
impl Serialize for DefaultValue {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let mut map =
            serializer.serialize_map(Some(if matches!(self, Self::Absent) { 1 } else { 2 }))?;
        map.serialize_entry("present", &matches!(self, Self::Present(_)))?;
        if let Self::Present(value) = self {
            map.serialize_entry("value", value)?;
        }
        map.end()
    }
}
impl<'de> Deserialize<'de> for DefaultValue {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let mut map = BTreeMap::<String, NeutralValue>::deserialize(deserializer)?;
        let present = map.remove("present");
        let value = map.remove("value");
        if !map.is_empty() {
            return Err(serde::de::Error::custom("unknown default member"));
        }
        match (present, value) {
            (Some(NeutralValue::Boolean(false)), None) => Ok(Self::Absent),
            (Some(NeutralValue::Boolean(true)), Some(value)) => Ok(Self::Present(value)),
            _ => Err(serde::de::Error::custom(
                "default requires present:false or present:true with value",
            )),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TypeRef {
    Builtin { name: BuiltinType },
    Model { id: ModelId },
    Value { id: ValueTypeId },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BuiltinType {
    String,
    Token,
    Uri,
    Email,
    Hostname,
    Ip,
    Boolean,
    Integer,
    Decimal,
    Rational,
    Float,
    Date,
    Time,
    DateTime,
    TimeWithoutDate,
    Duration,
    Qname,
    Base64Binary,
    HexBinary,
    Symbol,
    Hash,
    EmailAddress,
    IpV4Address,
    IpV6Address,
    Uuid,
    Reference,
    UriReference,
    DateWithTimezone,
    DateTimeWithTimezone,
    DayTimeDuration,
    YearMonthDuration,
    PositiveInteger,
    NonNegativeInteger,
}
impl BuiltinType {
    /// Integer-like builtins (the contract defines no bounds for the signed variants).
    pub fn is_integer(&self) -> bool {
        matches!(
            self,
            Self::Integer | Self::PositiveInteger | Self::NonNegativeInteger
        )
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Collection {
    Scalar(ScalarCollection),
    Bounded {
        min: u64,
        max: Option<u64>,
        #[serde(rename = "class", skip_serializing_if = "Option::is_none")]
        class_binding: Option<String>,
    },
}
/// Scalar collection metadata is exactly false, never true.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScalarCollection;
impl Default for Collection {
    fn default() -> Self {
        Self::Scalar(ScalarCollection)
    }
}
impl Default for DefaultValue {
    fn default() -> Self {
        Self::Absent
    }
}
impl Serialize for ScalarCollection {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_bool(false)
    }
}
impl<'de> Deserialize<'de> for ScalarCollection {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        if bool::deserialize(d)? {
            Err(serde::de::Error::custom("scalar collection must be false"))
        } else {
            Ok(Self)
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Discriminator {
    pub attribute: String,
    pub cases: BTreeMap<String, ModelId>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attribute {
    pub name: String,
    #[serde(rename = "type")]
    pub value_type: TypeRef,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub collection: Collection,
    #[serde(default)]
    pub default: DefaultValue,
    #[serde(default)]
    pub initialize_empty: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discriminator: Option<Discriminator>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reference: Option<ModelId>,
    #[serde(rename = "values", skip_serializing_if = "Option::is_none")]
    pub enumeration: Option<Vec<NeutralValue>>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValueType {
    pub base: BuiltinType,
    pub facets: BTreeMap<String, NeutralValue>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrderedContent {
    pub member: String,
    pub version: u32,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TextSource {
    Member { attribute: String, index: u64 },
    Literal { literal: String },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ContentToken {
    Element {
        attribute: String,
        index: u64,
    },
    Text {
        #[serde(flatten)]
        source: TextSource,
    },
    Cdata {
        #[serde(flatten)]
        source: TextSource,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum Namespace {
    Inherit,
    Unqualified,
    Uri {
        uri: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        prefix: Option<String>,
    },
}
/// Ruby accepts boolean and string policies. Unknown strings remain data and
/// must be refused by a target that cannot implement their declared behavior.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum RenderAction {
    #[default]
    Absent,
    Omit,
    Nil,
    Blank,
    AsNil,
    AsBlank,
    Boolean(bool),
    Other(String),
}
impl RenderAction {
    pub fn is_absent(&self) -> bool {
        matches!(self, Self::Absent)
    }
}
impl From<bool> for RenderAction {
    fn from(value: bool) -> Self {
        Self::Boolean(value)
    }
}
impl Serialize for RenderAction {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Absent => Err(serde::ser::Error::custom("absent policy must be omitted")),
            Self::Boolean(v) => s.serialize_bool(*v),
            Self::Other(v) => s.serialize_str(v),
            v => s.serialize_str(match v {
                Self::Omit => "omit",
                Self::Nil => "nil",
                Self::Blank => "blank",
                Self::AsNil => "as_nil",
                Self::AsBlank => "as_blank",
                _ => unreachable!(),
            }),
        }
    }
}
impl<'de> Deserialize<'de> for RenderAction {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Policy {
            Boolean(bool),
            Text(String),
        }
        Ok(match Policy::deserialize(d)? {
            Policy::Boolean(v) => Self::Boolean(v),
            Policy::Text(v) => match v.as_str() {
                "omit" => Self::Omit,
                "nil" => Self::Nil,
                "blank" => Self::Blank,
                "as_nil" => Self::AsNil,
                "as_blank" => Self::AsBlank,
                _ => Self::Other(v),
            },
        })
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderPolicy {
    #[serde(default, skip_serializing_if = "RenderAction::is_absent")]
    pub nil: RenderAction,
    #[serde(default, skip_serializing_if = "RenderAction::is_absent")]
    pub empty: RenderAction,
    #[serde(default, skip_serializing_if = "RenderAction::is_absent")]
    pub default: RenderAction,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum XmlRuleKind {
    #[default]
    #[serde(skip)]
    Unspecified,
    Attribute,
    Element,
    Content,
    Sequence,
    Choice,
    /// Ruby `map_all_attributes`: collects every attribute no other rule claims.
    AnyAttribute,
}
impl XmlRuleKind {
    fn is_unspecified(&self) -> bool {
        matches!(self, Self::Unspecified)
    }
}
/// Shared declarative rule shape: XML groups recursively carry full rules,
/// while JSON/YAML rules carry ordered key aliases. Defaults retain old callers.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct XmlRule {
    #[serde(default, skip_serializing_if = "XmlRuleKind::is_unspecified")]
    pub kind: XmlRuleKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub key: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub to: String,
    #[serde(default)]
    pub namespace: Namespace,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prefix: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cdata: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub render: Option<RenderPolicy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub discriminator: Option<Discriminator>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub members: Option<Vec<XmlRule>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<NeutralValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<NeutralValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub child_mappings: Option<BTreeMap<String, NeutralValue>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root_mappings: Option<BTreeMap<String, NeutralValue>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub polymorphic_map: Option<BTreeMap<String, String>>,
}
impl Default for Namespace {
    fn default() -> Self {
        Self::Inherit
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct XmlRoot {
    pub element: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mixed: Option<bool>,
    #[serde(flatten)]
    pub metadata: BTreeMap<String, NeutralValue>,
}
/// Legacy read compatibility. Canonical XML sequences contain full XmlRules.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Compositor {
    Sequence {
        members: Vec<CompositorMember>,
        min: u64,
        max: Option<u64>,
    },
    Choice {
        members: Vec<CompositorMember>,
        min: u64,
        max: Option<u64>,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CompositorMember {
    Attribute(String),
    Group(Box<Compositor>),
}
impl From<Compositor> for XmlRule {
    fn from(group: Compositor) -> Self {
        let (kind, members, min, max) = match group {
            Compositor::Sequence { members, min, max } => {
                (XmlRuleKind::Sequence, members, min, max)
            }
            Compositor::Choice { members, min, max } => (XmlRuleKind::Choice, members, min, max),
        };
        Self {
            kind,
            members: Some(
                members
                    .into_iter()
                    .map(|member| match member {
                        CompositorMember::Attribute(to) => Self {
                            kind: XmlRuleKind::Element,
                            to,
                            ..Default::default()
                        },
                        CompositorMember::Group(group) => (*group).into(),
                    })
                    .collect(),
            ),
            min: Some(NeutralValue::Unsigned(min)),
            max: Some(
                max.map(NeutralValue::Unsigned)
                    .unwrap_or(NeutralValue::Null),
            ),
            ..Default::default()
        }
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct XmlMapping {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub root: Option<XmlRoot>,
    pub namespace: Namespace,
    pub rules: Vec<XmlRule>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sequence: Option<Vec<XmlRule>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ordered: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mixed_content: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub root_mappings: Option<BTreeMap<String, NeutralValue>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub child_mappings: Option<BTreeMap<String, NeutralValue>>,
}
impl<'de> Deserialize<'de> for XmlMapping {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Sequence {
            Canonical(Vec<XmlRule>),
            Legacy(Compositor),
        }
        #[derive(Deserialize, Default)]
        struct Wire {
            root: Option<XmlRoot>,
            #[serde(default)]
            namespace: Namespace,
            #[serde(default)]
            rules: Vec<XmlRule>,
            sequence: Option<Sequence>,
            choice: Option<Compositor>,
            ordered: Option<bool>,
            mixed_content: Option<bool>,
            root_mappings: Option<BTreeMap<String, NeutralValue>>,
            child_mappings: Option<BTreeMap<String, NeutralValue>>,
        }
        let w = Wire::deserialize(d)?;
        let mut sequence = w.sequence.map(|s| match s {
            Sequence::Canonical(v) => v,
            Sequence::Legacy(v) => vec![v.into()],
        });
        if let Some(choice) = w.choice {
            sequence.get_or_insert_with(Vec::new).push(choice.into());
        }
        Ok(Self {
            root: w.root,
            namespace: w.namespace,
            rules: w.rules,
            sequence,
            ordered: w.ordered,
            mixed_content: w.mixed_content,
            root_mappings: w.root_mappings,
            child_mappings: w.child_mappings,
        })
    }
}
pub type KeyRule = XmlRule;
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyMapping {
    #[serde(default)]
    pub rules: Vec<KeyRule>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root_mappings: Option<BTreeMap<String, NeutralValue>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub child_mappings: Option<BTreeMap<String, NeutralValue>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ordered: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mixed_content: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root: Option<NeutralValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namespace: Option<Namespace>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mappings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub xml: Option<XmlMapping>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub json: Option<KeyMapping>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub yaml: Option<KeyMapping>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelDocument {
    pub lml: u32,
    pub model: String,
    pub register: String,
    pub attributes: Vec<Attribute>,
    #[serde(default)]
    pub mappings: Mappings,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ordered_content: Option<OrderedContent>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bundle {
    pub lml_bundle: u32,
    pub roots: Vec<ModelId>,
    pub models: BTreeMap<ModelId, ModelDocument>,
    pub value_types: BTreeMap<ValueTypeId, ValueType>,
    pub capabilities: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provenance: Option<BTreeMap<String, NeutralValue>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorReason {
    UnsupportedVersion,
    InvalidDocument,
    DuplicateId,
    DanglingReference,
    UnknownType,
    UnsupportedType,
    MissingTypeBinding,
    UnsupportedFormat,
    UnsupportedMapping,
    NonSerializableDefault,
    LambdaDefault,
    CustomInitialize,
    ImperativeElementOrder,
    InvalidOrderedContent,
    ReservedMember,
    NonterminatingRational,
    UnknownDiscriminator,
    RoundTripMismatch,
    ChecksumMismatch,
    CapabilityMissing,
    NotSerializable,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractError {
    pub reason: ErrorReason,
    pub model_id: Option<ModelId>,
    pub attribute: Option<String>,
    pub format: Option<String>,
    pub rule: Option<String>,
    pub location: String,
    pub klass: Option<String>,
    pub detail: Option<String>,
}
impl fmt::Display for ContractError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?} at {}", self.reason, self.location)?;
        if let Some(detail) = &self.detail {
            write!(f, ": {detail}")?;
        }
        Ok(())
    }
}
impl std::error::Error for ContractError {}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Target {
    Ruby,
    Ts,
    Rust,
    Wasm,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CorpusPin {
    pub generation: String,
    pub sha256: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FilePin {
    pub path: String,
    pub sha256: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DependencyPin {
    pub name: String,
    pub revision: String,
    pub sha256: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    pub manifest: u32,
    pub lml_checksum: String,
    pub schema_version: u32,
    pub generator_version: String,
    pub target: Target,
    pub capabilities: Vec<String>,
    pub corpora: Vec<CorpusPin>,
    pub files: Vec<FilePin>,
    pub dependencies: Vec<DependencyPin>,
}
/// Trusted target bindings are supplied by the caller, never by schema code.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeBinding {
    pub rust_type: String,
    pub codec: Option<String>,
}
pub type TypeBindings = BTreeMap<ValueTypeId, TypeBinding>;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputFormat {
    Json,
    Yaml,
}
pub trait BundleLoader {
    fn load_bundle(&self, bytes: &[u8], format: InputFormat) -> ContractResult<Bundle>;
}
pub trait ModelEmitter {
    fn generate_models(
        &self,
        bundle: &Bundle,
        bindings: &TypeBindings,
    ) -> ContractResult<SourceMap>;
}
pub trait JsonCodecEmitter {
    fn generate_json_codecs(
        &self,
        bundle: &Bundle,
        bindings: &TypeBindings,
    ) -> ContractResult<SourceMap>;
}
pub trait XmlCodecEmitter {
    fn generate_xml_codecs(
        &self,
        bundle: &Bundle,
        bindings: &TypeBindings,
    ) -> ContractResult<SourceMap>;
}
/// Compile-time PARG lowering hook. Bind stable grammar rule/capture names to
/// LML class IDs and members before emitting Rust; this carries no runtime
/// grammar or executable behavior. Application/typed behavior stays external.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PargClassBinding {
    pub rule: String,
    pub model_id: ModelId,
    pub captures: BTreeMap<String, String>,
}
pub trait PargBindingEmitter {
    fn generate_parg_bindings(
        &self,
        bundle: &Bundle,
        bindings: &[PargClassBinding],
    ) -> ContractResult<SourceMap>;
}
