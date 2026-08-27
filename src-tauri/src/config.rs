//! Configuration is deliberately split by lifetime and sensitivity.
//! Secrets are represented only by a credential id; the secret provider is a later phase.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub version: u32,
    pub permission_mode: PermissionMode,
    pub telemetry_enabled: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self { version: 1, permission_mode: PermissionMode::ConfirmChanges, telemetry_enabled: false }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceSettings {
    pub workspace_id: String,
    pub name: String,
    pub active_server_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PermissionMode {
    AskOnly,
    ReadOnly,
    ConfirmChanges,
    Advanced,
    Restricted,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretReference {
    pub credential_id: String,
    pub provider: SecretProvider,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SecretProvider {
    OsKeychain,
    SecretService,
    Development,
}
