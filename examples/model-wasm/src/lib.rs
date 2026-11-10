//! Example wrapper over real compile-time generated model and JSON codecs.
mod contracts {
    pub use lutaml_rs::contracts::*;
}
#[path = "../../../src/wasm/mod.rs"]
mod bridge;
#[allow(dead_code)]
#[path = "../../../src/values/mod.rs"]
mod values;
include!(concat!(env!("OUT_DIR"), "/wiring.rs"));
use contracts::*;
use std::cell::RefCell;
use wasm_bindgen::prelude::*;

pub const GENERATOR_VERSION: &str = "lml-rust-wasm-v1";
pub const LML_CHECKSUM: &str = env!("LML_CHECKSUM");
thread_local! { static SESSION: RefCell<bridge::Session> = RefCell::default(); }
fn js_error(error: ContractError) -> JsValue {
    // ContractError is an acyclic typed object containing no lossy numbers.
    serde_wasm_bindgen::to_value(&error).unwrap_or_else(|_| JsValue::from_str("invalid_document"))
}
#[wasm_bindgen]
pub fn initialize(version: &str, checksum: &str) -> Result<(), JsValue> {
    SESSION
        .with(|session| {
            session
                .borrow_mut()
                .initialize(version, checksum, GENERATOR_VERSION, LML_CHECKSUM)
        })
        .map_err(|error| js_error(*error))
}
#[wasm_bindgen]
pub fn round_trip_json(input: &str) -> Result<String, JsValue> {
    round_trip(input).map_err(|error| js_error(*error))
}
fn round_trip(input: &str) -> ContractResult<String> {
    SESSION.with(|session| session.borrow().require_initialized())?;
    let model = json_codecs::read_Invoice_json(input)?;
    json_codecs::write_Invoice_json(&model)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn generated_model_exact_string_bridge_and_presence() {
        let mut s = bridge::Session::default();
        assert_eq!(
            s.require_initialized().unwrap_err().reason,
            ErrorReason::CapabilityMissing
        );
        assert_eq!(
            s.initialize("wrong", LML_CHECKSUM, GENERATOR_VERSION, LML_CHECKSUM)
                .unwrap_err()
                .reason,
            ErrorReason::UnsupportedVersion
        );
        s.initialize(
            GENERATOR_VERSION,
            LML_CHECKSUM,
            GENERATOR_VERSION,
            LML_CHECKSUM,
        )
        .unwrap();
        s.require_initialized().unwrap();
        assert_eq!(
            s.initialize(GENERATOR_VERSION, "bad", GENERATOR_VERSION, LML_CHECKSUM)
                .unwrap_err()
                .reason,
            ErrorReason::ChecksumMismatch
        );
        assert!(s.require_initialized().is_err());
        let input = r#"{"amount":"9007199254740993123456789.2300","count":"9007199254740993123456789","enabled":false,"title":null}"#;
        let model = json_codecs::read_Invoice_json(input).unwrap();
        let output = json_codecs::write_Invoice_json(&model).unwrap();
        let value: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(value["total"], "9007199254740993123456789.2300");
        assert_eq!(value["count"], "9007199254740993123456789");
        assert_eq!(value["enabled"], false);
        assert!(value["title"].is_null());
        assert!(json_codecs::read_Invoice_json(r#"{"total":123.5}"#).is_err());
    }
}
