use async_trait::async_trait;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::{credentials::SecretValue, models::ServerProfile};

pub mod hostkey;
pub mod real;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ConnectionState {
    Connecting,
    #[allow(dead_code)]
    WaitingHostKey,
    Authenticating,
    Connected,
    Disconnecting,
    Disconnected,
    #[allow(dead_code)]
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionDto {
    pub id: String,
    pub server_id: String,
    pub state: ConnectionState,
    pub remote_address: Option<String>,
    pub server_version: Option<String>,
    pub authenticated_by: Option<String>,
    pub connected_at: Option<String>,
    pub disconnected_at: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum SshError {
    #[error("invalid connection state transition: {from:?} -> {to:?}")]
    InvalidTransition {
        from: ConnectionState,
        to: ConnectionState,
    },
    #[error("connection not found")]
    ConnectionNotFound,
    #[error("provider error: {0}")]
    Provider(String),
    #[error("channel limit reached")]
    ChannelLimit,
    #[error("host key verification required")]
    HostKeyRequired {
        host: String,
        port: u16,
        algorithm: String,
        fingerprint_sha256: String,
    },
}

#[derive(Debug)]
pub struct ProviderConnection {
    #[allow(dead_code)]
    pub id: String,
    #[allow(dead_code)]
    pub server_id: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PtyOptions {
    pub terminal_type: String,
    pub cols: u16,
    pub rows: u16,
    pub cwd: Option<String>,
    pub env: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalSessionDto {
    pub session_id: String,
    pub terminal_id: String,
    pub connection_id: String,
    pub state: String,
    pub cols: u16,
    pub rows: u16,
    pub opened_at: Option<String>,
    pub closed_at: Option<String>,
    pub exit_code: Option<i32>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecRequest {
    pub command: String,
    pub timeout_ms: u64,
    pub cwd: Option<String>,
    pub env: HashMap<String, String>,
    pub max_output_bytes: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecResult {
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub duration_ms: u64,
    pub truncated: bool,
    pub stdout_bytes: usize,
    pub stderr_bytes: usize,
    pub signal: Option<String>,
}

#[derive(Debug)]
pub struct ProviderPty {
    pub id: String,
}

/// Drained PTY output chunk; `closed` signals the remote side hung up.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PtyChunk {
    pub data: Vec<u8>,
    pub closed: bool,
}

#[async_trait]
pub trait SshProvider: Send + Sync {
    async fn connect(
        &self,
        profile: &ServerProfile,
        credential: Option<&SecretValue>,
        cancel: CancellationToken,
    ) -> Result<ProviderConnection, SshError>;
    async fn open_pty(
        &self,
        connection: &ProviderConnection,
        options: PtyOptions,
        cancel: CancellationToken,
    ) -> Result<ProviderPty, SshError>;
    async fn exec(
        &self,
        connection: &ProviderConnection,
        request: ExecRequest,
        cancel: CancellationToken,
    ) -> Result<ExecResult, SshError>;
    #[allow(dead_code)]
    async fn disconnect(&self, connection: ProviderConnection) -> Result<(), SshError>;
    async fn pty_write(&self, pty_id: &str, data: &[u8]) -> Result<(), SshError>;
    async fn pty_resize(&self, pty_id: &str, cols: u16, rows: u16) -> Result<(), SshError>;
    /// Drains buffered PTY output accumulated since the last call.
    async fn pty_take_output(&self, pty_id: &str) -> Result<PtyChunk, SshError>;
    async fn pty_close(&self, pty_id: &str) -> Result<(), SshError>;
}

#[async_trait]
impl<T: SshProvider + ?Sized> SshProvider for Box<T> {
    async fn connect(
        &self,
        profile: &ServerProfile,
        credential: Option<&SecretValue>,
        cancel: CancellationToken,
    ) -> Result<ProviderConnection, SshError> {
        self.as_ref().connect(profile, credential, cancel).await
    }
    async fn open_pty(
        &self,
        connection: &ProviderConnection,
        options: PtyOptions,
        cancel: CancellationToken,
    ) -> Result<ProviderPty, SshError> {
        self.as_ref().open_pty(connection, options, cancel).await
    }
    async fn exec(
        &self,
        connection: &ProviderConnection,
        request: ExecRequest,
        cancel: CancellationToken,
    ) -> Result<ExecResult, SshError> {
        self.as_ref().exec(connection, request, cancel).await
    }
    async fn disconnect(&self, connection: ProviderConnection) -> Result<(), SshError> {
        self.as_ref().disconnect(connection).await
    }
    async fn pty_write(&self, pty_id: &str, data: &[u8]) -> Result<(), SshError> {
        self.as_ref().pty_write(pty_id, data).await
    }
    async fn pty_resize(&self, pty_id: &str, cols: u16, rows: u16) -> Result<(), SshError> {
        self.as_ref().pty_resize(pty_id, cols, rows).await
    }
    async fn pty_take_output(&self, pty_id: &str) -> Result<PtyChunk, SshError> {
        self.as_ref().pty_take_output(pty_id).await
    }
    async fn pty_close(&self, pty_id: &str) -> Result<(), SshError> {
        self.as_ref().pty_close(pty_id).await
    }
}

#[derive(Default)]
pub struct MockSshProvider {
    ptys: Mutex<HashMap<String, MockPty>>,
}

#[derive(Default)]
struct MockPty {
    echoed: Vec<u8>,
    pending: Vec<u8>,
    closed: bool,
}

#[async_trait]
impl SshProvider for MockSshProvider {
    async fn connect(
        &self,
        profile: &ServerProfile,
        _credential: Option<&SecretValue>,
        cancel: CancellationToken,
    ) -> Result<ProviderConnection, SshError> {
        if cancel.is_cancelled() {
            return Err(SshError::Provider("cancelled".into()));
        }
        Ok(ProviderConnection {
            id: Uuid::new_v4().to_string(),
            server_id: profile.id.clone(),
        })
    }
    async fn disconnect(&self, _connection: ProviderConnection) -> Result<(), SshError> {
        Ok(())
    }
    async fn open_pty(
        &self,
        connection: &ProviderConnection,
        options: PtyOptions,
        cancel: CancellationToken,
    ) -> Result<ProviderPty, SshError> {
        if cancel.is_cancelled() {
            return Err(SshError::Provider("cancelled".into()));
        }
        if options.terminal_type != "xterm-256color"
            || options.cwd.as_deref().is_some_and(|cwd| cwd.contains('\0'))
            || options.env.len() > 64
        {
            return Err(SshError::Provider("invalid PTY options".into()));
        }
        if options.cols < 20 || options.cols > 500 || options.rows < 5 || options.rows > 300 {
            return Err(SshError::Provider("invalid PTY dimensions".into()));
        }
        let id = format!("pty:{}", connection.id);
        self.ptys.lock().await.entry(id.clone()).or_default();
        Ok(ProviderPty { id })
    }
    async fn exec(
        &self,
        _connection: &ProviderConnection,
        request: ExecRequest,
        cancel: CancellationToken,
    ) -> Result<ExecResult, SshError> {
        if cancel.is_cancelled() {
            return Err(SshError::Provider("cancelled".into()));
        }
        if request.command.is_empty() || request.command.len() > 32_768 {
            return Err(SshError::Provider("invalid command length".into()));
        }
        if !(1_000..=300_000).contains(&request.timeout_ms) {
            return Err(SshError::Provider("invalid timeout".into()));
        }
        if !(4_096..=1_048_576).contains(&request.max_output_bytes) {
            return Err(SshError::Provider("invalid output limit".into()));
        }
        if request.cwd.as_deref().is_some_and(|cwd| cwd.contains('\0')) || request.env.len() > 64 {
            return Err(SshError::Provider("invalid exec environment".into()));
        }
        let stdout = request.command.clone();
        let stdout_bytes = stdout.len();
        Ok(ExecResult {
            exit_code: Some(0),
            stdout,
            stderr: String::new(),
            duration_ms: 0,
            truncated: false,
            stdout_bytes,
            stderr_bytes: 0,
            signal: None,
        })
    }

    async fn pty_write(&self, pty_id: &str, data: &[u8]) -> Result<(), SshError> {
        let mut ptys = self.ptys.lock().await;
        let pty = ptys.get_mut(pty_id).ok_or(SshError::ConnectionNotFound)?;
        pty.echoed.extend_from_slice(data);
        pty.pending.extend_from_slice(data);
        Ok(())
    }

    async fn pty_resize(&self, pty_id: &str, cols: u16, rows: u16) -> Result<(), SshError> {
        let ptys = self.ptys.lock().await;
        if !ptys.contains_key(pty_id) {
            return Err(SshError::ConnectionNotFound);
        }
        if cols == 0 || rows == 0 {
            return Err(SshError::Provider("invalid PTY dimensions".into()));
        }
        Ok(())
    }

    async fn pty_take_output(&self, pty_id: &str) -> Result<PtyChunk, SshError> {
        let mut ptys = self.ptys.lock().await;
        let pty = ptys.get_mut(pty_id).ok_or(SshError::ConnectionNotFound)?;
        Ok(PtyChunk {
            data: std::mem::take(&mut pty.pending),
            closed: pty.closed,
        })
    }

    async fn pty_close(&self, pty_id: &str) -> Result<(), SshError> {
        let mut ptys = self.ptys.lock().await;
        let pty = ptys.get_mut(pty_id).ok_or(SshError::ConnectionNotFound)?;
        pty.closed = true;
        Ok(())
    }
}

pub fn can_transition(from: ConnectionState, to: ConnectionState) -> bool {
    matches!(
        (from, to),
        (
            ConnectionState::Connecting,
            ConnectionState::WaitingHostKey
                | ConnectionState::Authenticating
                | ConnectionState::Failed
        ) | (
            ConnectionState::WaitingHostKey,
            ConnectionState::Authenticating | ConnectionState::Failed
        ) | (
            ConnectionState::Authenticating,
            ConnectionState::Connected | ConnectionState::Failed
        ) | (
            ConnectionState::Connected,
            ConnectionState::Disconnecting | ConnectionState::Failed
        ) | (
            ConnectionState::Disconnecting,
            ConnectionState::Disconnected
        )
    )
}

pub fn transition(dto: &mut ConnectionDto, next: ConnectionState) -> Result<(), SshError> {
    if !can_transition(dto.state, next) {
        return Err(SshError::InvalidTransition {
            from: dto.state,
            to: next,
        });
    }
    dto.state = next;
    if next == ConnectionState::Connected {
        dto.connected_at = Some(Utc::now().to_rfc3339());
    }
    if next == ConnectionState::Disconnected {
        dto.disconnected_at = Some(Utc::now().to_rfc3339());
    }
    Ok(())
}

pub struct SshManager<P> {
    provider: P,
    registry: Mutex<HashMap<String, (ConnectionDto, ProviderConnection)>>,
    /// session_id → (pty id, owning connection id)
    terminal_sessions: Mutex<HashMap<String, (String, String)>>,
    active_channels: Mutex<usize>,
}

impl<P: SshProvider> SshManager<P> {
    pub fn new(provider: P) -> Self {
        Self {
            provider,
            registry: Mutex::new(HashMap::new()),
            terminal_sessions: Mutex::new(HashMap::new()),
            active_channels: Mutex::new(0),
        }
    }

    fn resolve_pty<'a>(
        sessions: &'a HashMap<String, (String, String)>,
        session_id: &str,
    ) -> Result<&'a str, SshError> {
        sessions
            .get(session_id)
            .map(|(pty_id, _)| pty_id.as_str())
            .ok_or(SshError::ConnectionNotFound)
    }

    pub async fn terminal_write(&self, session_id: &str, data: &[u8]) -> Result<(), SshError> {
        let sessions = self.terminal_sessions.lock().await;
        let pty_id = Self::resolve_pty(&sessions, session_id)?;
        self.provider.pty_write(pty_id, data).await
    }

    pub async fn terminal_resize(
        &self,
        session_id: &str,
        cols: u16,
        rows: u16,
    ) -> Result<(), SshError> {
        let sessions = self.terminal_sessions.lock().await;
        let pty_id = Self::resolve_pty(&sessions, session_id)?;
        self.provider.pty_resize(pty_id, cols, rows).await
    }

    pub async fn terminal_read(&self, session_id: &str) -> Result<PtyChunk, SshError> {
        let sessions = self.terminal_sessions.lock().await;
        let pty_id = Self::resolve_pty(&sessions, session_id)?;
        self.provider.pty_take_output(pty_id).await
    }

    pub async fn terminal_close(&self, session_id: &str) -> Result<(), SshError> {
        let pty_id = {
            let mut sessions = self.terminal_sessions.lock().await;
            sessions
                .remove(session_id)
                .map(|(pty_id, _)| pty_id)
                .ok_or(SshError::ConnectionNotFound)?
        };
        self.provider.pty_close(&pty_id).await
    }

    /// Closes and forgets every terminal session owned by `connection_id`.
    /// Called on disconnect so no PTY outlives its connection.
    pub async fn close_terminal_sessions_of(&self, connection_id: &str) {
        let ptys: Vec<String> = {
            let mut sessions = self.terminal_sessions.lock().await;
            let owned: Vec<(String, String)> = sessions
                .iter()
                .filter(|(_, (_, owner))| owner == connection_id)
                .map(|(session_id, (pty_id, _))| (session_id.clone(), pty_id.clone()))
                .collect();
            for (session_id, _) in &owned {
                sessions.remove(session_id);
            }
            owned.into_iter().map(|(_, pty_id)| pty_id).collect()
        };
        for pty_id in ptys {
            let _ = self.provider.pty_close(&pty_id).await;
        }
    }

    pub async fn connect(
        &self,
        profile: &ServerProfile,
        credential: Option<&SecretValue>,
    ) -> Result<ConnectionDto, SshError> {
        let mut registry = self.registry.lock().await;
        if let Some((existing, _)) = registry.values().find(|(item, _)| {
            item.server_id == profile.id && item.state == ConnectionState::Connected
        }) {
            return Ok(existing.clone());
        }
        let id = Uuid::new_v4().to_string();
        let mut dto = ConnectionDto {
            id: id.clone(),
            server_id: profile.id.clone(),
            state: ConnectionState::Connecting,
            remote_address: None,
            server_version: None,
            authenticated_by: None,
            connected_at: None,
            disconnected_at: None,
        };
        transition(&mut dto, ConnectionState::Authenticating)?;
        let provider_connection = self
            .provider
            .connect(profile, credential, CancellationToken::new())
            .await?;
        transition(&mut dto, ConnectionState::Connected)?;
        registry.insert(id, (dto.clone(), provider_connection));
        Ok(dto)
    }

    pub async fn disconnect(&self, id: &str) -> Result<ConnectionDto, SshError> {
        self.close_terminal_sessions_of(id).await;
        let (mut dto, provider_connection) = {
            let mut registry = self.registry.lock().await;
            registry.remove(id).ok_or(SshError::ConnectionNotFound)?
        };
        transition(&mut dto, ConnectionState::Disconnecting)?;
        self.provider.disconnect(provider_connection).await?;
        transition(&mut dto, ConnectionState::Disconnected)?;
        Ok(dto)
    }

    pub async fn reconnect(
        &self,
        profile: &ServerProfile,
        credential: Option<&SecretValue>,
    ) -> Result<ConnectionDto, SshError> {
        let active_id = {
            let registry = self.registry.lock().await;
            registry
                .iter()
                .find(|(_, (item, _))| {
                    item.server_id == profile.id && item.state == ConnectionState::Connected
                })
                .map(|(id, _)| id.clone())
        };
        if let Some(id) = active_id {
            self.disconnect(&id).await?;
        }
        self.connect(profile, credential).await
    }

    pub async fn active_connection_id(&self, server_id: &str) -> Option<String> {
        let registry = self.registry.lock().await;
        registry.iter().find_map(|(id, (connection, _))| {
            (connection.server_id == server_id && connection.state == ConnectionState::Connected)
                .then(|| id.clone())
        })
    }

    pub async fn open_pty(
        &self,
        connection_id: &str,
        options: PtyOptions,
        cancel: CancellationToken,
    ) -> Result<TerminalSessionDto, SshError> {
        self.acquire_channel().await?;
        let registry = self.registry.lock().await;
        let (connection, provider_connection) = match registry.get(connection_id) {
            Some(value) => value,
            None => {
                drop(registry);
                self.release_channel().await;
                return Err(SshError::ConnectionNotFound);
            }
        };
        if connection.state != ConnectionState::Connected {
            self.release_channel().await;
            return Err(SshError::Provider("connection is not active".into()));
        }
        let result = self
            .provider
            .open_pty(provider_connection, options.clone(), cancel)
            .await;
        drop(registry);
        self.release_channel().await;
        let pty = result?;
        let session_id = Uuid::new_v4().to_string();
        self.terminal_sessions.lock().await.insert(
            session_id.clone(),
            (pty.id.clone(), connection_id.to_string()),
        );
        Ok(TerminalSessionDto {
            session_id,
            terminal_id: pty.id,
            connection_id: connection_id.into(),
            state: "open".into(),
            cols: options.cols,
            rows: options.rows,
            opened_at: Some(Utc::now().to_rfc3339()),
            closed_at: None,
            exit_code: None,
        })
    }

    pub async fn exec(
        &self,
        connection_id: &str,
        request: ExecRequest,
        cancel: CancellationToken,
    ) -> Result<ExecResult, SshError> {
        self.acquire_channel().await?;
        let registry = self.registry.lock().await;
        let (connection, provider_connection) = match registry.get(connection_id) {
            Some(value) => value,
            None => {
                drop(registry);
                self.release_channel().await;
                return Err(SshError::ConnectionNotFound);
            }
        };
        if connection.state != ConnectionState::Connected {
            self.release_channel().await;
            return Err(SshError::Provider("connection is not active".into()));
        }
        let result = self
            .provider
            .exec(provider_connection, request, cancel)
            .await;
        drop(registry);
        self.release_channel().await;
        result
    }

    async fn acquire_channel(&self) -> Result<(), SshError> {
        let mut active = self.active_channels.lock().await;
        if *active >= 8 {
            return Err(SshError::ChannelLimit);
        }
        *active += 1;
        Ok(())
    }

    async fn release_channel(&self) {
        let mut active = self.active_channels.lock().await;
        *active = active.saturating_sub(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_terminal_state_reentry() {
        let mut dto = ConnectionDto {
            id: "c".into(),
            server_id: "s".into(),
            state: ConnectionState::Disconnected,
            remote_address: None,
            server_version: None,
            authenticated_by: None,
            connected_at: None,
            disconnected_at: None,
        };
        assert!(transition(&mut dto, ConnectionState::Connected).is_err());
    }

    #[test]
    fn transition_guard_matches_contract() {
        let valid = [
            (ConnectionState::Connecting, ConnectionState::Authenticating),
            (ConnectionState::Connecting, ConnectionState::WaitingHostKey),
            (
                ConnectionState::WaitingHostKey,
                ConnectionState::Authenticating,
            ),
            (ConnectionState::Authenticating, ConnectionState::Connected),
            (ConnectionState::Connected, ConnectionState::Disconnecting),
            (
                ConnectionState::Disconnecting,
                ConnectionState::Disconnected,
            ),
            (ConnectionState::Connected, ConnectionState::Failed),
        ];
        for (from, to) in valid {
            assert!(can_transition(from, to), "expected {from:?}->{to:?}");
        }
        let invalid = [
            (ConnectionState::Disconnected, ConnectionState::Connected),
            (ConnectionState::Failed, ConnectionState::Connecting),
            (ConnectionState::Connecting, ConnectionState::Connected),
            (ConnectionState::Disconnecting, ConnectionState::Connected),
        ];
        for (from, to) in invalid {
            assert!(!can_transition(from, to), "unexpected {from:?}->{to:?}");
        }
    }

    #[tokio::test]
    async fn manager_deduplicates_active_connections() {
        let profile = ServerProfile {
            id: "server".into(),
            name: "test".into(),
            host: "localhost".into(),
            port: 22,
            username: "dev".into(),
            auth: crate::models::AuthRef::Agent,
            environment: crate::models::Environment::Dev,
            tags: vec![],
            connect_timeout_ms: 15_000,
            keep_alive_interval_sec: 30,
            created_at: String::new(),
            updated_at: String::new(),
        };
        let manager = SshManager::new(MockSshProvider::default());
        let first = manager.connect(&profile, None).await.expect("connect");
        let second = manager.connect(&profile, None).await.expect("deduplicate");
        assert_eq!(first.id, second.id);
    }

    #[tokio::test]
    async fn explicit_reconnect_creates_a_new_connection() {
        let profile = ServerProfile {
            id: "server-reconnect".into(),
            name: "test".into(),
            host: "localhost".into(),
            port: 22,
            username: "dev".into(),
            auth: crate::models::AuthRef::Agent,
            environment: crate::models::Environment::Dev,
            tags: vec![],
            connect_timeout_ms: 15_000,
            keep_alive_interval_sec: 30,
            created_at: String::new(),
            updated_at: String::new(),
        };
        let manager = SshManager::new(MockSshProvider::default());
        let first = manager.connect(&profile, None).await.expect("connect");
        let second = manager.reconnect(&profile, None).await.expect("reconnect");
        assert_ne!(first.id, second.id);
        assert_eq!(second.state, ConnectionState::Connected);
    }

    #[tokio::test]
    async fn mock_supports_pty_and_exec_contracts() {
        let profile = ServerProfile {
            id: "server-pty".into(),
            name: "test".into(),
            host: "localhost".into(),
            port: 22,
            username: "dev".into(),
            auth: crate::models::AuthRef::Agent,
            environment: crate::models::Environment::Dev,
            tags: vec![],
            connect_timeout_ms: 15_000,
            keep_alive_interval_sec: 30,
            created_at: String::new(),
            updated_at: String::new(),
        };
        let manager = SshManager::new(MockSshProvider::default());
        let connection = manager.connect(&profile, None).await.expect("connect");
        let pty = manager
            .open_pty(
                &connection.id,
                PtyOptions {
                    terminal_type: "xterm-256color".into(),
                    cols: 120,
                    rows: 36,
                    cwd: None,
                    env: HashMap::new(),
                },
                CancellationToken::new(),
            )
            .await
            .expect("pty");
        assert_eq!(pty.state, "open");
        let result = manager
            .exec(
                &connection.id,
                ExecRequest {
                    command: "printf ok".into(),
                    timeout_ms: 30_000,
                    cwd: None,
                    env: HashMap::new(),
                    max_output_bytes: 262_144,
                },
                CancellationToken::new(),
            )
            .await
            .expect("exec");
        assert_eq!(result.stdout, "printf ok");
    }

    #[tokio::test]
    async fn invalid_exec_limits_are_rejected() {
        let profile = ServerProfile {
            id: "server-limit".into(),
            name: "test".into(),
            host: "localhost".into(),
            port: 22,
            username: "dev".into(),
            auth: crate::models::AuthRef::Agent,
            environment: crate::models::Environment::Dev,
            tags: vec![],
            connect_timeout_ms: 15_000,
            keep_alive_interval_sec: 30,
            created_at: String::new(),
            updated_at: String::new(),
        };
        let manager = SshManager::new(MockSshProvider::default());
        let connection = manager.connect(&profile, None).await.expect("connect");
        let result = manager
            .exec(
                &connection.id,
                ExecRequest {
                    command: "true".into(),
                    timeout_ms: 1,
                    cwd: None,
                    env: HashMap::new(),
                    max_output_bytes: 262_144,
                },
                CancellationToken::new(),
            )
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn terminal_io_round_trip_then_close() {
        let profile = ServerProfile {
            id: "server-term".into(),
            name: "test".into(),
            host: "localhost".into(),
            port: 22,
            username: "dev".into(),
            auth: crate::models::AuthRef::Agent,
            environment: crate::models::Environment::Dev,
            tags: vec![],
            connect_timeout_ms: 15_000,
            keep_alive_interval_sec: 30,
            created_at: String::new(),
            updated_at: String::new(),
        };
        let manager = SshManager::new(MockSshProvider::default());
        let connection = manager.connect(&profile, None).await.expect("connect");
        let session = manager
            .open_pty(
                &connection.id,
                PtyOptions {
                    terminal_type: "xterm-256color".into(),
                    cols: 80,
                    rows: 24,
                    cwd: None,
                    env: HashMap::new(),
                },
                CancellationToken::new(),
            )
            .await
            .expect("pty");
        manager
            .terminal_write(&session.session_id, b"echo hi\n")
            .await
            .expect("write");
        let chunk = manager
            .terminal_read(&session.session_id)
            .await
            .expect("read");
        assert_eq!(chunk.data, b"echo hi\n");
        assert!(!chunk.closed);
        manager
            .terminal_resize(&session.session_id, 120, 40)
            .await
            .expect("resize");
        manager
            .terminal_close(&session.session_id)
            .await
            .expect("close");
        assert!(manager.terminal_read(&session.session_id).await.is_err());
    }

    #[tokio::test]
    async fn disconnect_closes_terminal_sessions_of_the_connection() {
        let profile = ServerProfile {
            id: "server-term-drop".into(),
            name: "test".into(),
            host: "localhost".into(),
            port: 22,
            username: "dev".into(),
            auth: crate::models::AuthRef::Agent,
            environment: crate::models::Environment::Dev,
            tags: vec![],
            connect_timeout_ms: 15_000,
            keep_alive_interval_sec: 30,
            created_at: String::new(),
            updated_at: String::new(),
        };
        let manager = SshManager::new(MockSshProvider::default());
        let connection = manager.connect(&profile, None).await.expect("connect");
        let session = manager
            .open_pty(
                &connection.id,
                PtyOptions {
                    terminal_type: "xterm-256color".into(),
                    cols: 80,
                    rows: 24,
                    cwd: None,
                    env: HashMap::new(),
                },
                CancellationToken::new(),
            )
            .await
            .expect("pty");
        manager
            .disconnect(&connection.id)
            .await
            .expect("disconnect");
        assert!(manager.terminal_read(&session.session_id).await.is_err());
    }

    #[tokio::test]
    async fn provider_honors_cancellation() {
        let profile = ServerProfile {
            id: "cancel".into(),
            name: "test".into(),
            host: "localhost".into(),
            port: 22,
            username: "dev".into(),
            auth: crate::models::AuthRef::Agent,
            environment: crate::models::Environment::Dev,
            tags: vec![],
            connect_timeout_ms: 15_000,
            keep_alive_interval_sec: 30,
            created_at: String::new(),
            updated_at: String::new(),
        };
        let cancel = CancellationToken::new();
        cancel.cancel();
        let result = MockSshProvider::default()
            .connect(&profile, None, cancel)
            .await;
        assert!(matches!(result, Err(SshError::Provider(message)) if message == "cancelled"));
    }
}
