use crate::credentials::CredentialError;
use crate::ssh::SshError;
use serde::Serialize;
use serde_json::{json, Value};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("validation failed: {0}")]
    Validation(String),
    #[error("storage error: {0}")]
    Storage(#[from] rusqlite::Error),
    #[error("internal error: {0}")]
    Internal(String),
    #[error("credential provider error: {0}")]
    Credential(String),
    #[error("ssh error: {0}")]
    Ssh(String),
}

impl From<CredentialError> for AppError {
    fn from(error: CredentialError) -> Self {
        Self::Credential(error.to_string())
    }
}

impl From<SshError> for AppError {
    fn from(error: SshError) -> Self {
        Self::Ssh(error.to_string())
    }
}

#[derive(Debug, Serialize)]
pub struct AppErrorDto {
    pub code: String,
    pub message: String,
    pub retryable: bool,
    pub category: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
}

impl AppError {
    pub fn dto(&self) -> AppErrorDto {
        let (code, category, retryable) = match self {
            Self::Validation(_) => ("VALIDATION_ERROR", "validation", false),
            Self::Storage(_) => ("STORAGE_ERROR", "storage", true),
            Self::Internal(_) => ("INTERNAL_ERROR", "unknown", true),
            Self::Credential(_) => ("CREDENTIAL_PROVIDER_ERROR", "credential", false),
            Self::Ssh(_) => ("SSH_ERROR", "ssh", true),
        };
        AppErrorDto {
            code: code.into(),
            message: self.to_string(),
            retryable,
            category: category.into(),
            details: Some(json!({})),
        }
    }
}

impl Serialize for AppError {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.dto().serialize(serializer)
    }
}
