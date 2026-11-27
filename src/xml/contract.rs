//! Target-local XML adapter boundary. No upstream DOM or parser API is assumed.
use crate::contracts::{
    Attribute, Compositor, ContentToken, ContractResult, CustomSerialization, Discriminator,
    Namespace, RenderPolicy, TypeRef, XmlRuleKind,
};

/// Static field information consumed by generated, target-specific codecs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Field {
    pub member: String,
    /// Preserves collection bounds, defaults, requiredness and exact type metadata.
    pub attribute: Attribute,
    pub value_type: TypeRef,
    pub kind: XmlRuleKind,
    /// Readers accept every alias; writers use the first alias.
    pub aliases: Vec<String>,
    pub namespace: Namespace,
    pub prefix: Option<String>,
    pub cdata: bool,
    /// None retains the Ruby default; adapters must resolve it explicitly.
    pub render: Option<RenderPolicy>,
    pub discriminator: Option<Discriminator>,
    /// Names of handwritten hooks. A rule with `custom` may have no declared attribute (hook-only:
    /// `member` is empty and `attribute` a placeholder).
    pub custom: Option<CustomSerialization>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Operation {
    Root {
        element: Option<String>,
        namespace: Namespace,
    },
    Field(Box<Field>),
    Sequence(Compositor),
    Choice(Compositor),
    /// Instance tokens, never schema member traversal, determine mixed order.
    InstanceOrder {
        member: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Projection {
    pub model_id: String,
    pub operations: Vec<Operation>,
}

/// Static dispatch allows native and wasm implementations to use different
/// handles and batch APIs without exposing them to schema compilation.
/// Implementors must enforce policies/compositors or return a typed error;
/// successful partial execution is forbidden.
pub trait XmlReader {
    type Input;
    type Model;
    fn read_batch(
        &mut self,
        projection: &Projection,
        input: Self::Input,
    ) -> ContractResult<Self::Model>;
}

pub trait XmlWriter {
    type Model;
    type Output;
    fn write_batch(
        &mut self,
        projection: &Projection,
        model: &Self::Model,
        order: Option<&[ContentToken]>,
    ) -> ContractResult<Self::Output>;
}
