use chrono::Utc;
use tauri::State;
use tracing::{info, instrument};

use crate::{
    app_state::AppState,
    credentials::SecretValue,
    error::AppError,
    models::{HealthCheckDto, ServerProfile, ServerProfileInput},
    ssh::ConnectionDto,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[tauri::command]
#[instrument(skip(state), target = "infradeck::app")]
pub fn health_check(state: State<'_, AppState>) -> Result<HealthCheckDto, AppError> {
    let _db = state
        .db
        .lock()
        .map_err(|_| AppError::Internal("database lock poisoned".into()))?;
    info!(target: "infradeck::app", "health check");
    Ok(HealthCheckDto {
        schema_version: 1,
        status: "ok",
        app_version: env!("CARGO_PKG_VERSION"),
        storage: "ready",
        timestamp: Utc::now().to_rfc3339(),
    })
}

#[tauri::command]
#[instrument(skip(state), target = "infradeck::storage")]
pub fn server_profiles_list(state: State<'_, AppState>) -> Result<Vec<ServerProfile>, AppError> {
    let db = state
        .db
        .lock()
        .map_err(|_| AppError::Internal("database lock poisoned".into()))?;
    db.list_server_profiles()
}

#[tauri::command]
#[instrument(skip(state), target = "infradeck::ssh")]
pub async fn server_connect(
    state: State<'_, AppState>,
    server_id: String,
) -> Result<ConnectionDto, AppError> {
    let profile = {
        let db = state
            .db
            .lock()
            .map_err(|_| AppError::Internal("database lock poisoned".into()))?;
        db.list_server_profiles()?
            .into_iter()
            .find(|item| item.id == server_id)
            .ok_or_else(|| AppError::Validation("服务器配置不存在".into()))?
    };
    state
        .ssh
        .connect(&profile, None)
        .await
        .map_err(AppError::from)
}

#[tauri::command]
#[instrument(skip(state), target = "infradeck::ssh")]
pub async fn connection_disconnect(
    state: State<'_, AppState>,
    connection_id: String,
) -> Result<ConnectionDto, AppError> {
    state
        .ssh
        .disconnect(&connection_id)
        .await
        .map_err(AppError::from)
}

#[tauri::command]
#[instrument(skip(state, input), target = "infradeck::storage")]
pub fn server_profile_save(
    state: State<'_, AppState>,
    input: ServerProfileInput,
) -> Result<ServerProfile, AppError> {
    let now = Utc::now().to_rfc3339();
    let profile = ServerProfile {
        id: input.id.unwrap_or_else(|| Uuid::new_v4().to_string()),
        name: input.name,
        host: input.host,
        port: input.port.unwrap_or(22),
        username: input.username,
        auth: input.auth,
        environment: input
            .environment
            .unwrap_or(crate::models::Environment::Unknown),
        tags: input.tags.unwrap_or_default(),
        connect_timeout_ms: input.connect_timeout_ms.unwrap_or(15_000),
        keep_alive_interval_sec: input.keep_alive_interval_sec.unwrap_or(30),
        created_at: now.clone(),
        updated_at: now,
    };
    validate_profile(&profile)?;
    let db = state
        .db
        .lock()
        .map_err(|_| AppError::Internal("database lock poisoned".into()))?;
    db.upsert_server_profile(&profile)?;
    Ok(profile)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialSetInput {
    pub credential_id: Option<String>,
    pub secret: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialRefDto {
    pub credential_id: String,
    pub exists: bool,
}

#[tauri::command]
#[instrument(skip(state, input), target = "infradeck::credential")]
pub fn credential_set(
    state: State<'_, AppState>,
    input: CredentialSetInput,
) -> Result<CredentialRefDto, AppError> {
    let id = input
        .credential_id
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let secret = SecretValue::new(input.secret).map_err(AppError::from)?;
    state.credentials.set(&id, secret).map_err(AppError::from)?;
    Ok(CredentialRefDto {
        credential_id: id,
        exists: true,
    })
}

#[tauri::command]
#[instrument(skip(state), target = "infradeck::credential")]
pub fn credential_delete(
    state: State<'_, AppState>,
    credential_id: String,
) -> Result<(), AppError> {
    state
        .credentials
        .delete(&credential_id)
        .map_err(AppError::from)
}

#[tauri::command]
#[instrument(skip(state), target = "infradeck::credential")]
pub fn credential_exists(
    state: State<'_, AppState>,
    credential_id: String,
) -> Result<bool, AppError> {
    state
        .credentials
        .exists(&credential_id)
        .map_err(AppError::from)
}

fn validate_profile(profile: &ServerProfile) -> Result<(), AppError> {
    if profile.id.trim().is_empty() {
        return Err(AppError::Validation("server profile id is required".into()));
    }
    if profile.name.trim().is_empty() {
        return Err(AppError::Validation("服务器名称不能为空".into()));
    }
    if profile.host.trim().is_empty() {
        return Err(AppError::Validation("主机地址不能为空".into()));
    }
    if profile.username.trim().is_empty() {
        return Err(AppError::Validation("用户名不能为空".into()));
    }
    if profile.port == 0 {
        return Err(AppError::Validation("端口必须在 1-65535 范围内".into()));
    }
    Ok(())
}
