use async_trait::async_trait;
use chrono::Utc;
use serde::Serialize;
use std::collections::HashMap;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::{credentials::SecretValue, models::ServerProfile};

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
}

#[derive(Debug)]
pub struct ProviderConnection {
    #[allow(dead_code)]
    pub id: String,
    #[allow(dead_code)]
    pub server_id: String,
}
#[async_trait]
pub trait SshProvider: Send + Sync {
    async fn connect(
        &self,
        profile: &ServerProfile,
        credential: Option<&SecretValue>,
        cancel: CancellationToken,
    ) -> Result<ProviderConnection, SshError>;
    #[allow(dead_code)]
    async fn disconnect(&self, connection: ProviderConnection) -> Result<(), SshError>;
}

#[derive(Default)]
pub struct MockSshProvider;

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
}

impl<P: SshProvider> SshManager<P> {
    pub fn new(provider: P) -> Self {
        Self {
            provider,
            registry: Mutex::new(HashMap::new()),
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
        let (mut dto, provider_connection) = {
            let mut registry = self.registry.lock().await;
            registry.remove(id).ok_or(SshError::ConnectionNotFound)?
        };
        transition(&mut dto, ConnectionState::Disconnecting)?;
        self.provider.disconnect(provider_connection).await?;
        transition(&mut dto, ConnectionState::Disconnected)?;
        Ok(dto)
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
        let manager = SshManager::new(MockSshProvider);
        let first = manager.connect(&profile, None).await.expect("connect");
        let second = manager.connect(&profile, None).await.expect("deduplicate");
        assert_eq!(first.id, second.id);
    }
}
