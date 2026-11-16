//! Shared portable-v2 data and code generation contracts.
//!
//! Validation/loading and target emitters are separate modules, integrated by
//! their owners. Deserializing these types alone does not validate graph closure.
pub mod contracts;
pub use contracts::*;

#[cfg(feature = "generator")]
pub mod artifacts;
#[cfg(feature = "generator")]
pub mod codecs;
#[cfg(feature = "generator")]
pub mod generator;
#[cfg(feature = "generator")]
pub mod lml;
pub mod values;
pub mod xml;
