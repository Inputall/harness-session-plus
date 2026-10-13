use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Default, Serialize)]
pub struct Capabilities {
    pub list: bool,
    pub archive: bool,
    pub restore: bool,
    pub permanently_delete: bool,
    pub activity_state: bool,
}
#[derive(Debug, Serialize)]
pub struct Report {
    pub schema_version: u32,
    pub browser: String,
    pub protocol_version: Option<String>,
    pub structural_evidence: Value,
    pub capabilities: Capabilities,
    pub instance_identity_verified: bool,
    pub mutations_enabled: bool,
    pub reason: &'static str,
}
impl Report {
    pub fn new(version: crate::discovery::Version, structural_evidence: Value) -> Self {
        Self { schema_version: 1, browser: version.browser, protocol_version: version.protocol_version, structural_evidence, capabilities: Capabilities::default(), instance_identity_verified: false, mutations_enabled: false,
            reason: "Structural evidence is not proof of session identity, activity, archive or restore. All mutations are disabled; permanent deletion is not implemented." }
    }
}
