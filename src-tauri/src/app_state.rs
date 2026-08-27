use crate::{
    config::{AppSettings, PermissionMode, SecretProvider, SecretReference, WorkspaceSettings},
    credentials::{CredentialProvider, PlatformCredentialProvider},
    error::AppError,
    ssh::{
        real::{HostKeyTrustStore, RusshProvider},
        SshManager, SshProvider,
    },
    storage::Database,
    tools::ToolCall,
};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

pub struct AppState {
    pub db: Mutex<Database>,
    pub credentials: Arc<dyn CredentialProvider>,
    pub ssh: SshManager<Box<dyn SshProvider>>,
    pub host_keys: Arc<HostKeyTrustStore>,
    pub pending_tool_calls: Mutex<HashMap<String, ToolCall>>,
}

impl AppState {
    pub fn new() -> Result<Self, AppError> {
        let _workspace_defaults = WorkspaceSettings {
            workspace_id: "default".into(),
            name: "Default workspace".into(),
            active_server_id: None,
        };
        let _app_settings = AppSettings::default();
        let _secret_reference = SecretReference {
            credential_id: "00000000-0000-4000-8000-000000000000".into(),
            provider: SecretProvider::OsKeychain,
        };
        let _permission_mode = PermissionMode::ConfirmChanges;
        let database = Database::open_default()?;
        database.expire_stale_approvals()?;
        let host_keys = Arc::new(HostKeyTrustStore::default());
        host_keys.load(database.list_known_host_fingerprints()?);
        let provider: Box<dyn SshProvider> = Box::new(RusshProvider::new(Arc::clone(&host_keys)));
        Ok(Self {
            db: Mutex::new(database),
            credentials: Arc::new(PlatformCredentialProvider),
            ssh: SshManager::new(provider),
            host_keys,
            pending_tool_calls: Mutex::new(HashMap::new()),
        })
    }
}
