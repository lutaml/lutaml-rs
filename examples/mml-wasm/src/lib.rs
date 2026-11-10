//! Generated mml/oracle XML codecs (over moxml-rs) behind a string/JSON boundary.
//! XML text and JSON text cross JS as strings only, so exact values never pass
//! through a JavaScript number.
use lutaml_rs::contracts::*;
use std::cell::Cell;
use wasm_bindgen::prelude::*;

pub const GENERATOR_VERSION: &str = "lml-rust-wasm-xml-v1";

include!(concat!(env!("OUT_DIR"), "/wiring.rs"));

fn error(reason: ErrorReason, location: &str, detail: impl Into<String>) -> Box<ContractError> {
    Box::new(ContractError {
        reason,
        model_id: None,
        attribute: None,
        format: Some("xml".into()),
        rule: None,
        location: location.into(),
        klass: None,
        detail: Some(detail.into()),
    })
}
fn to_json<T: serde::Serialize>(model: &T) -> ContractResult<String> {
    serde_json::to_string(model).map_err(|e| error(ErrorReason::NotSerializable, "", e.to_string()))
}
fn from_json<T: serde::de::DeserializeOwned>(data: &str) -> ContractResult<T> {
    serde_json::from_str(data)
        .map_err(|e| error(ErrorReason::InvalidDocument, "/data", e.to_string()))
}
fn unavailable(set: &str, model: &str) -> Box<ContractError> {
    let mut e = error(
        ErrorReason::CapabilityMissing,
        "/model",
        format!("no XML codec for {set}:{model}"),
    );
    e.model_id = Some(model.into());
    e
}

/// Read XML into model JSON through the generated codec of `set`/`model`.
pub fn read_xml_json(set: &str, model: &str, xml: &str) -> ContractResult<String> {
    dispatch_read(set, model, xml.as_bytes()).unwrap_or_else(|| Err(unavailable(set, model)))
}
/// Write model JSON as XML through the generated codec of `set`/`model`.
pub fn write_xml_json(set: &str, model: &str, data: &str) -> ContractResult<String> {
    dispatch_write(set, model, data).unwrap_or_else(|| Err(unavailable(set, model)))
}
/// Initialization gate; a failed (re)initialization closes it.
pub fn check_identity(version: &str, checksum: &str) -> ContractResult<()> {
    if version != GENERATOR_VERSION {
        return Err(error(
            ErrorReason::UnsupportedVersion,
            "/generator_version",
            version,
        ));
    }
    if checksum != LML_CHECKSUM {
        return Err(error(
            ErrorReason::ChecksumMismatch,
            "/lml_checksum",
            checksum,
        ));
    }
    Ok(())
}

thread_local! { static READY: Cell<bool> = const { Cell::new(false) }; }
fn require_ready() -> ContractResult<()> {
    if READY.with(Cell::get) {
        Ok(())
    } else {
        Err(error(
            ErrorReason::CapabilityMissing,
            "/initialization",
            "wasm codec is not initialized",
        ))
    }
}
fn js_error(error: ContractError) -> JsValue {
    serde_wasm_bindgen::to_value(&error).unwrap_or_else(|_| JsValue::from_str("invalid_document"))
}

#[wasm_bindgen]
pub fn initialize(version: &str, checksum: &str) -> Result<(), JsValue> {
    READY.with(|r| r.set(false));
    check_identity(version, checksum).map_err(|e| js_error(*e))?;
    READY.with(|r| r.set(true));
    Ok(())
}
/// JSON object `{set: lml_checksum}` of the compiled-in bundles.
#[wasm_bindgen]
pub fn set_checksums() -> String {
    SET_CHECKSUMS.into()
}
#[wasm_bindgen]
pub fn read_xml(set: &str, model: &str, xml: &str) -> Result<String, JsValue> {
    require_ready()
        .and_then(|_| read_xml_json(set, model, xml))
        .map_err(|e| js_error(*e))
}
#[wasm_bindgen]
pub fn write_xml(set: &str, model: &str, data: &str) -> Result<String, JsValue> {
    require_ready()
        .and_then(|_| write_xml_json(set, model, data))
        .map_err(|e| js_error(*e))
}
