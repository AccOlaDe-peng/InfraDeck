//! File management over SFTP (V0.2 Epic L). Paths are untrusted input:
//! every entry point validates them before touching the remote side.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Errors map 1:1 onto the FS_* error codes in the V0.2 spec.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum FtpError {
    #[error("path is invalid")]
    PathInvalid,
    #[error("path not found")]
    NotFound,
    #[error("sftp unsupported by server")]
    Unsupported,
    #[error("transfer failed: {0}")]
    Transfer(String),
}

impl From<FtpError> for crate::error::AppError {
    fn from(error: FtpError) -> Self {
        let (code, message) = match &error {
            FtpError::PathInvalid => ("FS_PATH_INVALID", error.to_string()),
            FtpError::NotFound => ("FS_NOT_FOUND", error.to_string()),
            FtpError::Unsupported => ("FS_SFTP_UNSUPPORTED", error.to_string()),
            FtpError::Transfer(detail) => ("FS_TRANSFER_FAILED", format!("传输失败：{detail}")),
        };
        Self::Fs {
            code: code.into(),
            message,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FileEntry {
    pub name: String,
    pub path: String,
    pub kind: FileKind,
    pub size: u64,
    pub mode: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner_id: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_id: Option<u32>,
    pub modified_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symlink_target: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum FileKind {
    File,
    Directory,
    Symlink,
    Other,
}

/// One chunk read from a remote file; `eof` terminates download loops.
#[derive(Debug, Clone)]
pub struct FsChunk {
    pub data: Vec<u8>,
    pub eof: bool,
}

/// Remote paths must be absolute and cannot escape anywhere via `..`.
pub fn validate_remote_path(path: &str) -> Result<(), FtpError> {
    if !path.starts_with('/') {
        return Err(FtpError::PathInvalid);
    }
    if path.contains('\0') {
        return Err(FtpError::PathInvalid);
    }
    if path.len() > 4096 {
        return Err(FtpError::PathInvalid);
    }
    for segment in path.split('/') {
        if segment == ".." {
            return Err(FtpError::PathInvalid);
        }
    }
    Ok(())
}

pub fn join_path(dir: &str, name: &str) -> String {
    let dir = dir.trim_end_matches('/');
    if dir.is_empty() {
        format!("/{name}")
    } else {
        format!("{dir}/{name}")
    }
}

pub fn mode_string(permissions: Option<u32>) -> String {
    match permissions {
        Some(bits) => format!("{:04o}", bits & 0o7777),
        None => "0000".into(),
    }
}

/// The filesystem surface InfraDeck needs; implemented over SFTP for SSH
/// connections and by an in-memory tree in tests. Kept as the V1 seam for
/// non-SSH filesystem providers; the SshProvider trait embeds the same
/// methods today because connections are owned by SshManager.
#[async_trait]
#[allow(dead_code)]
pub trait FsProvider: Send + Sync {
    async fn fs_list(&self, connection_id: &str, path: &str) -> Result<Vec<FileEntry>, FtpError>;
    async fn fs_stat(&self, connection_id: &str, path: &str) -> Result<FileEntry, FtpError>;
    async fn fs_mkdir(&self, connection_id: &str, path: &str) -> Result<(), FtpError>;
    async fn fs_rename(&self, connection_id: &str, from: &str, to: &str) -> Result<(), FtpError>;
    async fn fs_delete(
        &self,
        connection_id: &str,
        path: &str,
        recursive: bool,
    ) -> Result<(), FtpError>;
    async fn fs_read_range(
        &self,
        connection_id: &str,
        path: &str,
        offset: u64,
        len: u32,
    ) -> Result<FsChunk, FtpError>;
    /// Writes `data` at `offset`; `truncate` only applies when offset is 0.
    async fn fs_write_range(
        &self,
        connection_id: &str,
        path: &str,
        offset: u64,
        data: &[u8],
        truncate: bool,
    ) -> Result<(), FtpError>;
}

pub const DOWNLOAD_CHUNK_LEN: u32 = 256 * 1024;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_traversal_and_malformed_paths() {
        assert!(validate_remote_path("/var/log").is_ok());
        assert!(validate_remote_path("/").is_ok());
        assert!(
            validate_remote_path("/a//b").is_ok(),
            "double slashes fold, not fail"
        );
        assert_eq!(validate_remote_path("var/log"), Err(FtpError::PathInvalid));
        assert_eq!(
            validate_remote_path("/a/../etc"),
            Err(FtpError::PathInvalid)
        );
        assert_eq!(validate_remote_path("/a\0b"), Err(FtpError::PathInvalid));
        assert_eq!(
            validate_remote_path(&format!("/{}", "x".repeat(5000))),
            Err(FtpError::PathInvalid)
        );
    }

    #[test]
    fn join_and_mode_helpers() {
        assert_eq!(join_path("/", "etc"), "/etc");
        assert_eq!(join_path("/var", "log"), "/var/log");
        assert_eq!(mode_string(Some(0o100_644)), "0644");
        assert_eq!(mode_string(Some(0o40755)), "0755");
        assert_eq!(mode_string(None), "0000");
    }
}
