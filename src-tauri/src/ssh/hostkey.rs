use base64::{engine::general_purpose::STANDARD_NO_PAD, Engine as _};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HostKeyStatus {
    Unknown,
    Changed,
    Matched,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HostKeyCheckDto {
    pub host: String,
    pub port: u16,
    pub algorithm: String,
    pub fingerprint_sha256: String,
    pub status: HostKeyStatus,
    pub previous_fingerprint_sha256: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostKeyDecision {
    pub host: String,
    pub port: u16,
    pub algorithm: String,
    pub fingerprint_sha256: String,
    pub decision: HostKeyDecisionKind,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HostKeyDecisionKind {
    TrustOnce,
    TrustAndSave,
    Reject,
}

#[allow(dead_code)]
pub fn fingerprint_sha256(raw_public_key: &[u8]) -> String {
    let digest = Sha256::digest(raw_public_key);
    format!("SHA256:{}", STANDARD_NO_PAD.encode(digest))
}

pub fn evaluate(
    host: &str,
    port: u16,
    algorithm: &str,
    fingerprint: &str,
    previous: Option<&str>,
) -> HostKeyCheckDto {
    let status = match previous {
        None => HostKeyStatus::Unknown,
        Some(saved) if saved == fingerprint => HostKeyStatus::Matched,
        Some(_) => HostKeyStatus::Changed,
    };
    HostKeyCheckDto {
        host: host.into(),
        port,
        algorithm: algorithm.into(),
        fingerprint_sha256: fingerprint.into(),
        status,
        previous_fingerprint_sha256: previous.map(str::to_owned),
    }
}

pub fn decision_allowed(status: HostKeyStatus, decision: HostKeyDecisionKind) -> bool {
    match status {
        HostKeyStatus::Unknown => matches!(
            decision,
            HostKeyDecisionKind::TrustOnce
                | HostKeyDecisionKind::TrustAndSave
                | HostKeyDecisionKind::Reject
        ),
        HostKeyStatus::Changed | HostKeyStatus::Matched => matches!(
            decision,
            HostKeyDecisionKind::Reject | HostKeyDecisionKind::TrustOnce
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprint_uses_sha256_without_padding() {
        assert_eq!(
            fingerprint_sha256(b"key"),
            "SHA256:LHDhK3oGRvkiefQnx7OOczTY5Tic/xZ6HcMOc/gmtoM"
        );
    }

    #[test]
    fn changed_key_cannot_be_trusted_and_saved() {
        let check = evaluate(
            "example.com",
            22,
            "ssh-ed25519",
            "SHA256:new",
            Some("SHA256:old"),
        );
        assert_eq!(check.status, HostKeyStatus::Changed);
        assert!(!decision_allowed(
            check.status,
            HostKeyDecisionKind::TrustAndSave
        ));
        assert!(decision_allowed(check.status, HostKeyDecisionKind::Reject));
    }
}
