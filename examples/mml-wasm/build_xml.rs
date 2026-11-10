//! Ungated view of the 09a/09c emitter modules for the build script.
#[path = "../../src/xml/contract.rs"]
pub mod contract;
#[path = "../../src/xml/emitter.rs"]
mod emitter;
#[path = "../../src/xml/projection.rs"]
pub mod projection;
#[path = "../../src/xml/reader_emitter.rs"]
pub mod reader_emitter;
#[path = "../../src/xml/writer_emitter.rs"]
pub mod writer_emitter;
pub use emitter::RustXmlEmitter;
