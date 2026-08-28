use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use tauri::{Emitter, Manager, Runtime, State};
use tokio::sync::Notify;
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
    pub kind: String, // upload | download | serverToServer
    pub server_id: String,
    pub connection_id: String,
    pub remote_path: String,
    pub local_path: String,
    pub total_bytes: u64,
    pub transferred_bytes: u64,
    pub state: String, // queued | running | paused | completed | failed | cancelled
    pub speed_bytes_per_sec: Option<u64>,
    pub error: Option<crate::error::AppErrorDto>,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    /// serverToServer only: the copy source (dest rides in server_id et al.).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_server_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_connection_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_path: Option<String>,
}

/// Per-transfer control handle shared between the paused/resumed loop and the
/// pause/resume/cancel commands. `job` is the authoritative in-map snapshot.
#[derive(Debug)]
pub struct TransferHandle {
    pub job: TransferJobDto,
    pub cancel: CancellationToken,
    pub pause: Arc<AtomicBool>,
    pub notify: Arc<Notify>,
}

type TransferMap = Mutex<HashMap<String, TransferHandle>>;

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

/// Server-to-server copy: source and dest are both remote SFTP paths on
/// different connections. UI-triggered only — the AI tool surface never
/// exposes it.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Ss2sTransferRequest {
    pub source_server_id: String,
    pub source_connection_id: String,
    pub source_path: String,
    pub dest_server_id: String,
    pub dest_connection_id: String,
    pub dest_path: String,
    #[serde(default)]
    pub overwrite: bool,
}

pub fn validate_ss2s_request(request: &Ss2sTransferRequest) -> Result<(), AppError> {
    if request.source_connection_id == request.dest_connection_id {
        return Err(AppError::Fs {
            code: "SS2S_SAME_NODE".into(),
            message: "跨节点传输的源与目标不能是同一连接".into(),
        });
    }
    validate_remote_path(&request.source_path).map_err(AppError::from)?;
    validate_remote_path(&request.dest_path).map_err(AppError::from)?;
    Ok(())
}

/// Concurrency ceiling from the V1.1 design: at most 4 active transfers
/// globally and 2 per connection (queued/running/paused all count as active).
fn enforce_transfer_limits(
    transfers: &Mutex<HashMap<String, TransferHandle>>,
    connection_id: &str,
) -> Result<(), AppError> {
    let map = transfers
        .lock()
        .map_err(|_| AppError::Internal("transfer lock poisoned".into()))?;
    let active = |filter: &dyn Fn(&TransferHandle) -> bool| {
        map.values()
            .filter(|handle| {
                matches!(handle.job.state.as_str(), "queued" | "running" | "paused")
                    && filter(handle)
            })
            .count()
    };
    if active(&|_| true) >= 4 {
        return Err(AppError::Validation(
            "并发传输已达全局上限（4），请等待现有任务完成".into(),
        ));
    }
    if active(&|handle| handle.job.connection_id == connection_id) >= 2 {
        return Err(AppError::Validation("该连接的并发传输已达上限（2）".into()));
    }
    Ok(())
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
pub async fn fs_transfer_start<R: Runtime + 'static>(
    state: State<'_, AppState>,
    app: tauri::AppHandle<R>,
    request: TransferRequest,
) -> Result<TransferJobDto, AppError> {
    validate_transfer_request(&request)?;
    enforce_transfer_limits(&state.transfers, &request.connection_id)?;
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
        source_server_id: None,
        source_connection_id: None,
        source_path: None,
    };
    write_fs_audit(
        &state,
        "fs.write",
        &request.server_id,
        &request.remote_path,
        "success",
    );
    let cancel = CancellationToken::new();
    let pause = Arc::new(AtomicBool::new(false));
    let notify = Arc::new(Notify::new());
    let running = job.clone();
    state
        .transfers
        .lock()
        .map_err(|_| AppError::Internal("transfer lock poisoned".into()))?
        .insert(
            job.transfer_id.clone(),
            TransferHandle {
                job: job.clone(),
                cancel: cancel.clone(),
                pause: Arc::clone(&pause),
                notify: Arc::clone(&notify),
            },
        );
    let queue_state = Arc::clone(&state.transfers);
    tokio::spawn(run_transfer(
        queue_state,
        app,
        running,
        request,
        cancel,
        pause,
        notify,
    ));
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
    if let Some(handle) = transfers.get(&transfer_id) {
        handle.cancel.cancel();
        // Wake a paused loop so it observes the cancellation and exits.
        handle.notify.notify_one();
        Ok(true)
    } else {
        Ok(false)
    }
}

/// Pauses a running transfer at the next chunk boundary (async effect: the
/// loop flips the job to `paused` and emits `transfer.state` itself).
#[tauri::command]
#[instrument(skip(state, transfer_id), target = "infradeck::fs")]
pub fn fs_transfer_pause(
    state: State<'_, AppState>,
    transfer_id: String,
) -> Result<bool, AppError> {
    let transfers = state
        .transfers
        .lock()
        .map_err(|_| AppError::Internal("transfer lock poisoned".into()))?;
    if let Some(handle) = transfers.get(&transfer_id) {
        if handle.job.state != "running" {
            return Ok(false);
        }
        handle.pause.store(true, Ordering::SeqCst);
        Ok(true)
    } else {
        Ok(false)
    }
}

/// Resumes a paused transfer from its current byte offset (not from zero).
#[tauri::command]
#[instrument(skip(state, transfer_id), target = "infradeck::fs")]
pub fn fs_transfer_resume(
    state: State<'_, AppState>,
    transfer_id: String,
) -> Result<bool, AppError> {
    let transfers = state
        .transfers
        .lock()
        .map_err(|_| AppError::Internal("transfer lock poisoned".into()))?;
    if let Some(handle) = transfers.get(&transfer_id) {
        if handle.job.state != "paused" {
            return Ok(false);
        }
        handle.pause.store(false, Ordering::SeqCst);
        handle.notify.notify_one();
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
    Ok(transfers
        .values()
        .map(|handle| handle.job.clone())
        .collect())
}

/// Runs one transfer to completion, emitting throttled progress events. The
/// loop keeps its local `offset` across pauses, so resume continues from the
/// exact byte where the transfer was paused (both directions support offset
/// reads/writes through the provider).
async fn run_transfer<R: Runtime + 'static>(
    transfers: Arc<TransferMap>,
    app: tauri::AppHandle<R>,
    mut job: TransferJobDto,
    request: TransferRequest,
    cancel: CancellationToken,
    pause: Arc<AtomicBool>,
    notify: Arc<Notify>,
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
                pause_point(&app, &transfers, &job, &cancel, &pause, &notify).await;
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
                pause_point(&app, &transfers, &job, &cancel, &pause, &notify).await;
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
        if let Some(handle) = transfers.get_mut(&job.transfer_id) {
            handle.job = job.clone();
        }
    }
    let _ = app.emit("transfer.finished", &job);
}

/// Chunk-boundary pause gate: when the pause flag is set, synchronize the
/// in-map job snapshot, flip it to `paused`, emit `transfer.state`, and wait
/// until resume or cancel wakes the loop.
async fn pause_point<R: Runtime + 'static>(
    app: &tauri::AppHandle<R>,
    transfers: &Arc<TransferMap>,
    job: &TransferJobDto,
    cancel: &CancellationToken,
    paused: &AtomicBool,
    notify: &Notify,
) {
    if !paused.load(Ordering::SeqCst) {
        return;
    }
    {
        let Ok(mut map) = transfers.lock() else {
            return;
        };
        let Some(handle) = map.get_mut(&job.transfer_id) else {
            return;
        };
        let mut snapshot = handle.job.clone();
        snapshot.transferred_bytes = job.transferred_bytes;
        snapshot.state = "paused".into();
        snapshot.speed_bytes_per_sec = Some(0);
        handle.job = snapshot;
    }
    let _ = app.emit(
        "transfer.state",
        &json!({ "transferId": job.transfer_id, "state": "paused" }),
    );
    while paused.load(Ordering::SeqCst) && !cancel.is_cancelled() {
        notify.notified().await;
    }
    if let Ok(mut map) = transfers.lock() {
        if let Some(handle) = map.get_mut(&job.transfer_id) {
            handle.job.state = "running".into();
        }
    }
    let _ = app.emit(
        "transfer.state",
        &json!({ "transferId": job.transfer_id, "state": "running" }),
    );
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

/// Starts a server-to-server copy in the background. The job lands in the
/// same `transfers` map, so pause/resume/cancel/list all work unchanged.
#[tauri::command]
#[instrument(skip(state, app, request), target = "infradeck::fs")]
pub async fn ss2s_transfer_start<R: Runtime + 'static>(
    state: State<'_, AppState>,
    app: tauri::AppHandle<R>,
    request: Ss2sTransferRequest,
) -> Result<TransferJobDto, AppError> {
    validate_ss2s_request(&request)?;
    enforce_transfer_limits(&state.transfers, &request.dest_connection_id)?;
    let source = state
        .ssh
        .fs_stat(&request.source_connection_id, &request.source_path)
        .await
        .map_err(AppError::from)?;
    // Overwriting the remote dest is destructive: it must be explicit and is
    // audited together with the source path.
    if !request.overwrite {
        if let Ok(entry) = state
            .ssh
            .fs_stat(&request.dest_connection_id, &request.dest_path)
            .await
        {
            if entry.kind == fs::FileKind::File {
                return Err(AppError::Fs {
                    code: "SS2S_DEST_EXISTS".into(),
                    message: "目标文件已存在，需显式 overwrite".into(),
                });
            }
        }
    }
    let job = TransferJobDto {
        transfer_id: Uuid::new_v4().to_string(),
        kind: "serverToServer".into(),
        server_id: request.dest_server_id.clone(),
        connection_id: request.dest_connection_id.clone(),
        remote_path: request.dest_path.clone(),
        local_path: String::new(),
        total_bytes: source.size,
        transferred_bytes: 0,
        state: "running".into(),
        speed_bytes_per_sec: None,
        error: None,
        started_at: Some(Utc::now().to_rfc3339()),
        finished_at: None,
        source_server_id: Some(request.source_server_id.clone()),
        source_connection_id: Some(request.source_connection_id.clone()),
        source_path: Some(request.source_path.clone()),
    };
    let cancel = CancellationToken::new();
    let pause = Arc::new(AtomicBool::new(false));
    let notify = Arc::new(Notify::new());
    let running = job.clone();
    state
        .transfers
        .lock()
        .map_err(|_| AppError::Internal("transfer lock poisoned".into()))?
        .insert(
            job.transfer_id.clone(),
            TransferHandle {
                job: job.clone(),
                cancel: cancel.clone(),
                pause: Arc::clone(&pause),
                notify: Arc::clone(&notify),
            },
        );
    let queue_state = Arc::clone(&state.transfers);
    tokio::spawn(run_ss2s_transfer(
        queue_state,
        app,
        running,
        request,
        cancel,
        pause,
        notify,
    ));
    Ok(job)
}

/// Chunk-bridges source → dest: each read chunk is written at the same offset
/// on the destination (first chunk truncates). Pause/resume reuse the M12
/// gate; offset resume holds on both ends.
async fn run_ss2s_transfer<R: Runtime + 'static>(
    transfers: Arc<TransferMap>,
    app: tauri::AppHandle<R>,
    mut job: TransferJobDto,
    request: Ss2sTransferRequest,
    cancel: CancellationToken,
    pause: Arc<AtomicBool>,
    notify: Arc<Notify>,
) {
    let started = std::time::Instant::now();
    let mut last_emit = std::time::Instant::now();
    let result: Result<(), FtpError> = async {
        let mut offset: u64 = 0;
        let mut first = true;
        loop {
            if cancel.is_cancelled() {
                return Err(FtpError::Transfer("cancelled".into()));
            }
            let chunk = ss2s_read(&app, &request, offset, DOWNLOAD_CHUNK_LEN).await?;
            let eof = chunk.eof;
            ss2s_write(&app, &request, offset, &chunk.data, first).await?;
            first = false;
            offset += chunk.data.len() as u64;
            job.transferred_bytes = offset;
            pause_point(&app, &transfers, &job, &cancel, &pause, &notify).await;
            if last_emit.elapsed().as_millis() >= 200 || eof {
                last_emit = std::time::Instant::now();
                let _ = app.emit("transfer.progress", &progress_payload(&job, started));
            }
            if eof {
                break;
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
            job.error = Some(match error {
                FtpError::Transfer(detail) => AppError::Fs {
                    code: "SS2S_TRANSFER_FAILED".into(),
                    message: format!("跨节点传输失败：{detail}"),
                }
                .dto(),
                other => AppError::from(other).dto(),
            });
        }
    }
    job.finished_at = Some(Utc::now().to_rfc3339());
    // Audit before publishing the terminal state so observers of
    // `transfer.finished` always see the audit record already written.
    let outcome = if job.state == "completed" {
        "success"
    } else {
        "failed"
    };
    write_ss2s_audit(&app.state::<AppState>(), &request, outcome);
    if let Ok(mut transfers) = transfers.lock() {
        if let Some(handle) = transfers.get_mut(&job.transfer_id) {
            handle.job = job.clone();
        }
    }
    let _ = app.emit("transfer.finished", &job);
}

async fn ss2s_read<R: Runtime + 'static>(
    app: &tauri::AppHandle<R>,
    request: &Ss2sTransferRequest,
    offset: u64,
    len: u32,
) -> Result<fs::FsChunk, FtpError> {
    let state: tauri::State<AppState> = app.state();
    state
        .ssh
        .fs_read_range(
            &request.source_connection_id,
            &request.source_path,
            offset,
            len,
        )
        .await
}

async fn ss2s_write<R: Runtime + 'static>(
    app: &tauri::AppHandle<R>,
    request: &Ss2sTransferRequest,
    offset: u64,
    data: &[u8],
    truncate: bool,
) -> Result<(), FtpError> {
    let state: tauri::State<AppState> = app.state();
    state
        .ssh
        .fs_write_range(
            &request.dest_connection_id,
            &request.dest_path,
            offset,
            data,
            truncate,
        )
        .await
}

/// Audits the source→dest path pair for the ss2s transfer lifecycle.
fn write_ss2s_audit(state: &State<'_, AppState>, request: &Ss2sTransferRequest, outcome: &str) {
    let event = AuditEvent {
        id: Uuid::new_v4().to_string(),
        timestamp: Utc::now().to_rfc3339(),
        workspace_id: "default".into(),
        actor: "user".into(),
        server_id: Some(request.dest_server_id.clone()),
        connection_id: None,
        conversation_id: None,
        agent_run_id: None,
        action: "ss2s.transfer".into(),
        tool_name: None,
        tool_version: None,
        tool_call_id: None,
        approval_id: None,
        risk_level: None,
        policy_action: Some("allow".into()),
        outcome: outcome.into(),
        arguments_digest: None,
        sanitized_details: json!({
            "sourcePath": request.source_path,
            "destPath": request.dest_path,
        })
        .as_object()
        .cloned()
        .unwrap_or_default(),
    };
    if let Ok(db) = state.db.lock() {
        let _ = db.append_audit(&event);
    }
}

// The transfer task cannot hold State<'_, AppState>; reach the manager through
// the AppHandle-managed state instead.
async fn state_chunk<R: Runtime + 'static>(
    app: &tauri::AppHandle<R>,
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

async fn app_state_write<R: Runtime + 'static>(
    app: &tauri::AppHandle<R>,
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
