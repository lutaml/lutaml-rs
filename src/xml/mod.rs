//! XML projection lowering (09a), the moxml-rs binding and its generators (09c).
pub mod contract;
pub mod projection;

#[cfg(feature = "xml")]
pub mod moxml_binding;
#[cfg(all(feature = "xml", feature = "generator"))]
pub mod reader_emitter;
#[cfg(all(feature = "xml", feature = "generator"))]
pub mod writer_emitter;

#[cfg(all(feature = "xml", feature = "generator"))]
mod emitter;
#[cfg(all(feature = "xml", feature = "generator"))]
pub use emitter::RustXmlEmitter;
