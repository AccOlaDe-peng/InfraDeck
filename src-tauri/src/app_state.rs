use crate::{
    config::{AppSettings, PermissionMode, SecretProvider, SecretReference, WorkspaceSettings},
    credentials::{CredentialProvider, PlatformCredentialProvider},
    error::AppError,
    ssh::{MockSshProvider, SshManager},
    storage::Database,
};
use std::sync::{Arc, Mutex};

pub struct AppState {
    pub db: Mutex<Database>,
    pub credentials: Arc<dyn CredentialProvider>,
    pub ssh: SshManager<MockSshProvider>,
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
        Ok(Self {
            db: Mutex::new(Database::open_default()?),
            credentials: Arc::new(PlatformCredentialProvider),
            ssh: SshManager::new(MockSshProvider),
        })
    }
}
