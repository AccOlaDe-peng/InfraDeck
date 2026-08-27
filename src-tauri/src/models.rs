use serde::{Deserialize, Serialize};

pub type ServerId = String;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Environment {
    Dev,
    Staging,
    Production,
    Unknown,
}

impl Environment {
    pub fn as_str(&self) -> &'static str {
        match self { Self::Dev => "dev", Self::Staging => "staging", Self::Production => "production", Self::Unknown => "unknown" }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum AuthRef {
    #[serde(rename = "password")]
    Password { #[serde(rename = "credentialId")] credential_id: String },
    #[serde(rename = "privateKey")]
    PrivateKey { #[serde(rename = "keyPath")] key_path: String, #[serde(rename = "passphraseCredentialId", skip_serializing_if = "Option::is_none")] passphrase_credential_id: Option<String> },
    #[serde(rename = "agent")]
    Agent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerProfile {
    pub id: ServerId,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth: AuthRef,
    pub environment: Environment,
    pub tags: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthCheckDto {
    pub status: &'static str,
    pub app_version: &'static str,
    pub storage: &'static str,
    pub timestamp: String,
}
