use chrono::Utc;
use tauri::State;
use tracing::{info, instrument};

use crate::{app_state::AppState, error::AppError, models::{HealthCheckDto, ServerProfile}};

#[tauri::command]
#[instrument(skip(state), target = "infradeck::app")]
pub fn health_check(state: State<'_, AppState>) -> Result<HealthCheckDto, AppError> {
    let _db = state.db.lock().map_err(|_| AppError::Internal("database lock poisoned".into()))?;
    info!(target: "infradeck::app", "health check");
    Ok(HealthCheckDto { status: "ok", app_version: env!("CARGO_PKG_VERSION"), storage: "ready", timestamp: Utc::now().to_rfc3339() })
}

#[tauri::command]
#[instrument(skip(state), target = "infradeck::storage")]
pub fn server_profiles_list(state: State<'_, AppState>) -> Result<Vec<ServerProfile>, AppError> {
    let db = state.db.lock().map_err(|_| AppError::Internal("database lock poisoned".into()))?;
    db.list_server_profiles()
}

#[tauri::command]
#[instrument(skip(state, profile), target = "infradeck::storage")]
pub fn server_profile_save(state: State<'_, AppState>, profile: ServerProfile) -> Result<ServerProfile, AppError> {
    validate_profile(&profile)?;
    let db = state.db.lock().map_err(|_| AppError::Internal("database lock poisoned".into()))?;
    db.upsert_server_profile(&profile)?;
    Ok(profile)
}

fn validate_profile(profile: &ServerProfile) -> Result<(), AppError> {
    if profile.id.trim().is_empty() { return Err(AppError::Validation("server profile id is required".into())); }
    if profile.name.trim().is_empty() { return Err(AppError::Validation("服务器名称不能为空".into())); }
    if profile.host.trim().is_empty() { return Err(AppError::Validation("主机地址不能为空".into())); }
    if profile.username.trim().is_empty() { return Err(AppError::Validation("用户名不能为空".into())); }
    if profile.port == 0 { return Err(AppError::Validation("端口必须在 1-65535 范围内".into())); }
    Ok(())
}
