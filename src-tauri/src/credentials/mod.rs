use std::fmt;

use thiserror::Error;
use zeroize::Zeroize;

pub const KEYRING_SERVICE: &str = "com.infradeck.desktop";

#[derive(Error, Debug)]
pub enum CredentialError {
    #[error("credential id is invalid")]
    InvalidId,
    #[error("credential not found")]
    NotFound,
    #[error("credential provider failed: {0}")]
    Provider(String),
}

fn is_missing_entry(message: &str) -> bool {
    let normalized = message.to_ascii_lowercase();
    normalized.contains("no matching entry")
        || normalized.contains("not found")
        || normalized.contains("could not be found")
}

/// Secret bytes are intentionally not serializable, cloneable or displayable.
pub struct SecretValue(String);

impl SecretValue {
    pub fn new(value: String) -> Result<Self, CredentialError> {
        if value.is_empty() {
            return Err(CredentialError::Provider("secret cannot be empty".into()));
        }
        Ok(Self(value))
    }

    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for SecretValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretValue([REDACTED])")
    }
}

impl Drop for SecretValue {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

pub trait CredentialProvider: Send + Sync {
    fn set(&self, id: &str, secret: SecretValue) -> Result<(), CredentialError>;
    #[allow(dead_code)]
    fn get(&self, id: &str) -> Result<SecretValue, CredentialError>;
    fn delete(&self, id: &str) -> Result<(), CredentialError>;
    #[allow(dead_code)]
    fn exists(&self, id: &str) -> Result<bool, CredentialError>;
}

#[derive(Debug, Default)]
pub struct PlatformCredentialProvider;

impl PlatformCredentialProvider {
    fn entry(id: &str) -> Result<keyring::Entry, CredentialError> {
        if uuid::Uuid::parse_str(id).is_err() {
            return Err(CredentialError::InvalidId);
        }
        keyring::Entry::new(KEYRING_SERVICE, id)
            .map_err(|error| CredentialError::Provider(error.to_string()))
    }
}

impl CredentialProvider for PlatformCredentialProvider {
    fn set(&self, id: &str, secret: SecretValue) -> Result<(), CredentialError> {
        let entry = Self::entry(id)?;
        entry
            .set_password(secret.expose())
            .map_err(|error| CredentialError::Provider(error.to_string()))
    }

    fn get(&self, id: &str) -> Result<SecretValue, CredentialError> {
        let entry = Self::entry(id)?;
        let value = entry.get_password().map_err(|error| {
            let message = error.to_string();
            if is_missing_entry(&message) {
                CredentialError::NotFound
            } else {
                CredentialError::Provider(message)
            }
        })?;
        SecretValue::new(value)
    }

    fn delete(&self, id: &str) -> Result<(), CredentialError> {
        let entry = Self::entry(id)?;
        entry
            .delete_credential()
            .map_err(|error| CredentialError::Provider(error.to_string()))
    }

    fn exists(&self, id: &str) -> Result<bool, CredentialError> {
        let entry = Self::entry(id)?;
        match entry.get_password() {
            Ok(mut value) => {
                value.zeroize();
                Ok(true)
            }
            Err(error) if is_missing_entry(&error.to_string()) => Ok(false),
            Err(error) => Err(CredentialError::Provider(error.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_uuid_credential_ids() {
        let provider = PlatformCredentialProvider;
        let error = provider.exists("not-a-uuid").expect_err("invalid id");
        assert!(matches!(error, CredentialError::InvalidId));
    }

    #[test]
    fn secret_debug_is_redacted() {
        let secret = SecretValue::new("super-secret".into()).expect("secret");
        assert_eq!(format!("{secret:?}"), "SecretValue([REDACTED])");
    }

    #[test]
    fn recognizes_platform_missing_entry_messages() {
        assert!(is_missing_entry(
            "No matching entry found in secure storage"
        ));
        assert!(is_missing_entry("The item could not be found"));
        assert!(!is_missing_entry("User denied access to secure storage"));
    }
}
