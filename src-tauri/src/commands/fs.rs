use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tauri::{Emitter, Manager, State};
use tokio_util::sync::CancellationToken;
use tracing::instrument;
use uuid::Uuid;

use crate::{
    app_state::AppState,
    error::AppError,
    fs::{self, validate_remote_path, FtpError, DOWNLOAD_CHUNK_LEN},
    tools::AuditEvent,
};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferJobDto {
    pub transfer_id: String,
    pub kind: String, // upload | download
    pub server_id: String,
    pub connection_id: String,
    pub remote_path: String,
    pub local_path: String,
    pub total_bytes: u64,
    pub transferred_bytes: u64,
    pub state: String, // queued | running | completed | failed | cancelled
    pub speed_bytes_per_sec: Option<u64>,
    pub error: Option<crate::error::AppErrorDto>,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferRequest {
    pub kind: String, // upload | download
    pub server_id: String,
    pub connection_id: String,
    pub remote_path: String,
    pub local_path: String,
    #[serde(default)]
    pub overwrite: bool,
}

fn validate_transfer_request(request: &TransferRequest) -> Result<(), AppError> {
    if request.kind != "upload" && request.kind != "download" {
        return Err(AppError::Validation(
            "kind 必须是 upload 或 download".into(),
        ));
    }
    validate_remote_path(&request.remote_path).map_err(AppError::from)?;
    if request.local_path.trim().is_empty() {
        return Err(AppError::Validation("localPath 不能为空".into()));
    }
    Ok(())
}

#[tauri::command]
#[instrument(skip(state, connection_id, path), target = "infradeck::fs")]
pub async fn fs_list(
    state: State<'_, AppState>,
    connection_id: String,
    path: String,
) -> Result<Vec<fs::FileEntry>, AppError> {
    validate_remote_path(&path).map_err(AppError::from)?;
    Ok(state.ssh.fs_list(&connection_id, &path).await?)
}

#[tauri::command]
#[instrument(skip(state, connection_id, path), target = "infradeck::fs")]
pub async fn fs_stat(
    state: State<'_, AppState>,
    connection_id: String,
    path: String,
) -> Result<fs::FileEntry, AppError> {
    validate_remote_path(&path).map_err(AppError::from)?;
    Ok(state.ssh.fs_stat(&connection_id, &path).await?)
}

#[tauri::command]
#[instrument(skip(state, connection_id, path), target = "infradeck::fs")]
pub async fn fs_mkdir(
    state: State<'_, AppState>,
    connection_id: String,
    server_id: String,
    path: String,
) -> Result<(), AppError> {
    validate_remote_path(&path).map_err(AppError::from)?;
    state.ssh.fs_mkdir(&connection_id, &path).await?;
    write_fs_audit(&state, "fs.write", &server_id, &path, "success");
    Ok(())
}

#[tauri::command]
#[instrument(skip(state, connection_id), target = "infradeck::fs")]
pub async fn fs_rename(
    state: State<'_, AppState>,
    connection_id: String,
    server_id: String,
    from: String,
    to: String,
) -> Result<(), AppError> {
    validate_remote_path(&from).map_err(AppError::from)?;
    validate_remote_path(&to).map_err(AppError::from)?;
    state.ssh.fs_rename(&connection_id, &from, &to).await?;
    write_fs_audit(&state, "fs.write", &server_id, &to, "success");
    Ok(())
}

#[tauri::command]
#[instrument(skip(state, connection_id, path), target = "infradeck::fs")]
pub async fn fs_delete(
    state: State<'_, AppState>,
    connection_id: String,
    server_id: String,
    path: String,
    recursive: bool,
) -> Result<(), AppError> {
    validate_remote_path(&path).map_err(AppError::from)?;
    state
        .ssh
        .fs_delete(&connection_id, &path, recursive)
        .await?;
    write_fs_audit(&state, "fs.delete", &server_id, &path, "success");
    Ok(())
}

fn write_fs_audit(
    state: &State<'_, AppState>,
    action: &str,
    server_id: &str,
    path: &str,
    outcome: &str,
) {
    let event = AuditEvent {
        id: Uuid::new_v4().to_string(),
        timestamp: Utc::now().to_rfc3339(),
        workspace_id: "default".into(),
        actor: "user".into(),
        server_id: Some(server_id.to_string()),
        connection_id: None,
        conversation_id: None,
        agent_run_id: None,
        action: action.into(),
        tool_name: None,
        tool_version: None,
        tool_call_id: None,
        approval_id: None,
        risk_level: None,
        policy_action: Some("allow".into()),
        outcome: outcome.into(),
        arguments_digest: None,
        sanitized_details: json!({ "path": path })
            .as_object()
            .cloned()
            .unwrap_or_default(),
    };
    if let Ok(db) = state.db.lock() {
        let _ = db.append_audit(&event);
    }
}

/// Starts a download/upload in the background and returns the queued job.
#[tauri::command]
#[instrument(skip(state, app, request), target = "infradeck::fs")]
pub async fn fs_transfer_start(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
    request: TransferRequest,
) -> Result<TransferJobDto, AppError> {
    validate_transfer_request(&request)?;
    // Overwriting an existing remote file is the one destructive default;
    // it must be explicit and is audited.
    if request.kind == "upload" && !request.overwrite {
        if let Ok(entry) = state
            .ssh
            .fs_stat(&request.connection_id, &request.remote_path)
            .await
        {
            if entry.kind == fs::FileKind::File {
                return Err(AppError::Fs {
                    code: "FS_EXISTS".into(),
                    message: "远端文件已存在，需显式 overwrite".into(),
                });
            }
        }
    }
    let total_bytes = if request.kind == "download" {
        state
            .ssh
            .fs_stat(&request.connection_id, &request.remote_path)
            .await
            .map_err(AppError::from)?
            .size
    } else {
        tokio::fs::metadata(&request.local_path)
            .await
            .map(|meta| meta.len())
            .map_err(|error| AppError::Fs {
                code: "FS_TRANSFER_FAILED".into(),
                message: error.to_string(),
            })?
    };
    let job = TransferJobDto {
        transfer_id: Uuid::new_v4().to_string(),
        kind: request.kind.clone(),
        server_id: request.server_id.clone(),
        connection_id: request.connection_id.clone(),
        remote_path: request.remote_path.clone(),
        local_path: request.local_path.clone(),
        total_bytes,
        transferred_bytes: 0,
        state: "running".into(),
        speed_bytes_per_sec: None,
        error: None,
        started_at: Some(Utc::now().to_rfc3339()),
        finished_at: None,
    };
    write_fs_audit(
        &state,
        "fs.write",
        &request.server_id,
        &request.remote_path,
        "success",
    );
    let cancel = CancellationToken::new();
    let running = job.clone();
    state
        .transfers
        .lock()
        .map_err(|_| AppError::Internal("transfer lock poisoned".into()))?
        .insert(job.transfer_id.clone(), (job.clone(), cancel.clone()));
    let queue_state = Arc::clone(&state.transfers);
    tokio::spawn(run_transfer(queue_state, app, running, request, cancel));
    Ok(job)
}

#[tauri::command]
#[instrument(skip(state, transfer_id), target = "infradeck::fs")]
pub fn fs_transfer_cancel(
    state: State<'_, AppState>,
    transfer_id: String,
) -> Result<bool, AppError> {
    let transfers = state
        .transfers
        .lock()
        .map_err(|_| AppError::Internal("transfer lock poisoned".into()))?;
    if let Some((_, cancel)) = transfers.get(&transfer_id) {
        cancel.cancel();
        Ok(true)
    } else {
        Ok(false)
    }
}

#[tauri::command]
#[instrument(skip(state), target = "infradeck::fs")]
pub fn fs_transfers_list(state: State<'_, AppState>) -> Result<Vec<TransferJobDto>, AppError> {
    let transfers = state
        .transfers
        .lock()
        .map_err(|_| AppError::Internal("transfer lock poisoned".into()))?;
    Ok(transfers.values().map(|(job, _)| job.clone()).collect())
}

type TransferMap = Mutex<HashMap<String, (TransferJobDto, CancellationToken)>>;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Runs one transfer to completion, emitting throttled progress events.
async fn run_transfer(
    transfers: Arc<TransferMap>,
    app: tauri::AppHandle,
    mut job: TransferJobDto,
    request: TransferRequest,
    cancel: CancellationToken,
) {
    let started = std::time::Instant::now();
    let mut last_emit = std::time::Instant::now();
    let result: Result<(), FtpError> = async {
        if request.kind == "download" {
            if !request.overwrite {
                // Downloads always overwrite the local side deliberately chosen by the user.
            }
            let mut offset: u64 = 0;
            let mut file = tokio::fs::File::create(&request.local_path)
                .await
                .map_err(|error| FtpError::Transfer(error.to_string()))?;
            use tokio::io::AsyncWriteExt;
            loop {
                if cancel.is_cancelled() {
                    return Err(FtpError::Transfer("cancelled".into()));
                }
                let chunk = state_chunk(&app, &request, offset, DOWNLOAD_CHUNK_LEN).await?;
                let eof = chunk.eof;
                file.write_all(&chunk.data)
                    .await
                    .map_err(|error| FtpError::Transfer(error.to_string()))?;
                offset += chunk.data.len() as u64;
                job.transferred_bytes = offset;
                if last_emit.elapsed().as_millis() >= 200 || eof {
                    last_emit = std::time::Instant::now();
                    let _ = app.emit("transfer.progress", &progress_payload(&job, started));
                }
                if eof {
                    break;
                }
            }
            file.flush()
                .await
                .map_err(|error| FtpError::Transfer(error.to_string()))?;
        } else {
            let mut file = tokio::fs::File::open(&request.local_path)
                .await
                .map_err(|error| FtpError::Transfer(error.to_string()))?;
            use tokio::io::AsyncReadExt;
            let mut offset: u64 = 0;
            let mut buffer = vec![0u8; DOWNLOAD_CHUNK_LEN as usize];
            let mut first = true;
            loop {
                if cancel.is_cancelled() {
                    return Err(FtpError::Transfer("cancelled".into()));
                }
                let read = file
                    .read(&mut buffer)
                    .await
                    .map_err(|error| FtpError::Transfer(error.to_string()))?;
                if read == 0 {
                    break;
                }
                app_state_write(&app, &request, offset, &buffer[..read], first).await?;
                first = false;
                offset += read as u64;
                job.transferred_bytes = offset;
                if last_emit.elapsed().as_millis() >= 200 || offset >= job.total_bytes {
                    last_emit = std::time::Instant::now();
                    let _ = app.emit("transfer.progress", &progress_payload(&job, started));
                }
                if offset >= job.total_bytes {
                    break;
                }
            }
        }
        Ok(())
    }
    .await;

    let elapsed = started.elapsed().as_secs_f64();
    job.speed_bytes_per_sec = Some((job.transferred_bytes as f64 / elapsed.max(0.001)) as u64);
    match result {
        Ok(()) => {
            job.state = "completed".into();
        }
        Err(FtpError::Transfer(message)) if message == "cancelled" => {
            job.state = "cancelled".into();
        }
        Err(error) => {
            job.state = "failed".into();
            let app_error = AppError::from(error);
            job.error = Some(app_error.dto());
        }
    }
    job.finished_at = Some(Utc::now().to_rfc3339());
    if let Ok(mut transfers) = transfers.lock() {
        if let Some(entry) = transfers.get_mut(&job.transfer_id) {
            entry.0 = job.clone();
        }
    }
    let _ = app.emit("transfer.finished", &job);
}

fn progress_payload(job: &TransferJobDto, started: std::time::Instant) -> serde_json::Value {
    let elapsed = started.elapsed().as_secs_f64();
    json!({
        "transferId": job.transfer_id,
        "transferredBytes": job.transferred_bytes,
        "totalBytes": job.total_bytes,
        "speedBytesPerSec": (job.transferred_bytes as f64 / elapsed.max(0.001)) as u64,
    })
}

// The transfer task cannot hold State<'_, AppState>; reach the manager through
// the AppHandle-managed state instead.
async fn state_chunk(
    app: &tauri::AppHandle,
    request: &TransferRequest,
    offset: u64,
    len: u32,
) -> Result<fs::FsChunk, FtpError> {
    let state: tauri::State<AppState> = app.state();
    state
        .ssh
        .fs_read_range(&request.connection_id, &request.remote_path, offset, len)
        .await
}

async fn app_state_write(
    app: &tauri::AppHandle,
    request: &TransferRequest,
    offset: u64,
    data: &[u8],
    truncate: bool,
) -> Result<(), FtpError> {
    let state: tauri::State<AppState> = app.state();
    state
        .ssh
        .fs_write_range(
            &request.connection_id,
            &request.remote_path,
            offset,
            data,
            truncate,
        )
        .await
}
