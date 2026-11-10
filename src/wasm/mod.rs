//! Initialization gate shared by target-local generated wasm wrappers.
//! Exact values cross JS as JSON strings; no JS number conversion is involved.
use crate::contracts::{ContractError, ContractResult, ErrorReason};

#[derive(Default)]
pub struct Session {
    initialized: bool,
}
fn error(reason: ErrorReason, location: &str, detail: &str) -> Box<ContractError> {
    Box::new(ContractError {
        reason,
        model_id: None,
        attribute: None,
        format: Some("json".into()),
        rule: None,
        location: location.into(),
        klass: None,
        detail: Some(detail.into()),
    })
}
impl Session {
    /// A failed reinitialization closes the gate, including after earlier success.
    pub fn initialize(
        &mut self,
        version: &str,
        checksum: &str,
        expected_version: &str,
        expected_checksum: &str,
    ) -> ContractResult<()> {
        self.initialized = false;
        if version != expected_version {
            return Err(error(
                ErrorReason::UnsupportedVersion,
                "/generator_version",
                version,
            ));
        }
        if checksum != expected_checksum {
            return Err(error(
                ErrorReason::ChecksumMismatch,
                "/lml_checksum",
                checksum,
            ));
        }
        self.initialized = true;
        Ok(())
    }
    pub fn require_initialized(&self) -> ContractResult<()> {
        if !self.initialized {
            return Err(error(
                ErrorReason::CapabilityMissing,
                "/initialization",
                "wasm codec is not initialized",
            ));
        }
        Ok(())
    }
}
