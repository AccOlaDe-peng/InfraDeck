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
        match self {
            Self::Dev => "dev",
            Self::Staging => "staging",
            Self::Production => "production",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum AuthRef {
    #[serde(rename = "password")]
    Password {
        #[serde(rename = "credentialId")]
        credential_id: String,
    },
    #[serde(rename = "privateKey")]
    PrivateKey {
        #[serde(rename = "keyPath")]
        key_path: String,
        #[serde(
            rename = "passphraseCredentialId",
            skip_serializing_if = "Option::is_none"
        )]
        passphrase_credential_id: Option<String>,
    },
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
    pub connect_timeout_ms: u32,
    pub keep_alive_interval_sec: u32,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerProfileInput {
    pub id: Option<ServerId>,
    pub name: String,
    pub host: String,
    pub port: Option<u16>,
    pub username: String,
    pub auth: AuthRef,
    pub environment: Option<Environment>,
    pub tags: Option<Vec<String>>,
    pub connect_timeout_ms: Option<u32>,
    pub keep_alive_interval_sec: Option<u32>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthCheckDto {
    pub schema_version: u32,
    pub status: &'static str,
    pub app_version: &'static str,
    pub storage: &'static str,
    pub timestamp: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_profile_fixture_matches_wire_contract() {
        let fixture = include_str!("../../tests/contracts/server_profile.json");
        let profile: ServerProfile =
            serde_json::from_str(fixture).expect("valid server profile fixture");
        assert_eq!(profile.connect_timeout_ms, 15_000);
        assert_eq!(profile.keep_alive_interval_sec, 30);
        assert_eq!(
            serde_json::to_value(profile).expect("serialize")["createdAt"],
            "2026-08-27T00:00:00Z"
        );
    }
}
