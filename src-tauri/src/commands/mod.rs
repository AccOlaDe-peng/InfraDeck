pub mod ai;

use base64::Engine as _;
use chrono::Utc;
use tauri::State;
use tracing::{info, instrument};

use crate::{
    app_state::AppState,
    credentials::SecretValue,
    error::AppError,
    models::{HealthCheckDto, ServerProfile, ServerProfileInput},
    policy::{
        self, ApprovalDecision, ApprovalGrant, ApprovalStatus, PolicyDecision, RequiredConfirmation,
    },
    ssh::{
        hostkey::{
            decision_allowed, evaluate, HostKeyCheckDto, HostKeyDecision, HostKeyDecisionKind,
        },
        ConnectionDto, ExecRequest, ExecResult, PtyOptions, TerminalSessionDto,
    },
    tools::{
        self, AuditEvent, ToolCall, ToolDefinition, ToolExecutionResponse, ToolResult,
        ToolResultMeta,
    },
};
use chrono::DateTime;
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;
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
    let credential = match &profile.auth {
        crate::models::AuthRef::Password { credential_id } => {
            Some(state.credentials.get(credential_id)?)
        }
        crate::models::AuthRef::PrivateKey {
            passphrase_credential_id: Some(credential_id),
            ..
        } => Some(state.credentials.get(credential_id)?),
        _ => None,
    };
    state
        .ssh
        .connect(&profile, credential.as_ref())
        .await
        .map_err(AppError::from)
}

#[tauri::command]
#[instrument(skip(state), target = "infradeck::ssh")]
pub async fn server_reconnect(
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
    let credential = match &profile.auth {
        crate::models::AuthRef::Password { credential_id } => {
            Some(state.credentials.get(credential_id)?)
        }
        crate::models::AuthRef::PrivateKey {
            passphrase_credential_id: Some(credential_id),
            ..
        } => Some(state.credentials.get(credential_id)?),
        _ => None,
    };
    state
        .ssh
        .reconnect(&profile, credential.as_ref())
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
#[instrument(skip(state, options), target = "infradeck::ssh")]
pub async fn terminal_open(
    state: State<'_, AppState>,
    connection_id: String,
    options: PtyOptions,
) -> Result<TerminalSessionDto, AppError> {
    state
        .ssh
        .open_pty(&connection_id, options, CancellationToken::new())
        .await
        .map_err(AppError::from)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalReadDto {
    pub data_base64: String,
    pub closed: bool,
}

#[tauri::command]
#[instrument(skip(state), target = "infradeck::ssh")]
pub async fn terminal_write(
    state: State<'_, AppState>,
    session_id: String,
    data: String,
) -> Result<(), AppError> {
    state
        .ssh
        .terminal_write(&session_id, data.as_bytes())
        .await
        .map_err(AppError::from)
}

#[tauri::command]
#[instrument(skip(state), target = "infradeck::ssh")]
pub async fn terminal_read(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<TerminalReadDto, AppError> {
    let chunk = state
        .ssh
        .terminal_read(&session_id)
        .await
        .map_err(AppError::from)?;
    Ok(TerminalReadDto {
        data_base64: base64::engine::general_purpose::STANDARD.encode(&chunk.data),
        closed: chunk.closed,
    })
}

#[tauri::command]
#[instrument(skip(state), target = "infradeck::ssh")]
pub async fn terminal_resize(
    state: State<'_, AppState>,
    session_id: String,
    cols: u16,
    rows: u16,
) -> Result<(), AppError> {
    state
        .ssh
        .terminal_resize(&session_id, cols, rows)
        .await
        .map_err(AppError::from)
}

#[tauri::command]
#[instrument(skip(state), target = "infradeck::ssh")]
pub async fn terminal_close(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<(), AppError> {
    state
        .ssh
        .terminal_close(&session_id)
        .await
        .map_err(AppError::from)
}

#[tauri::command]
#[instrument(skip(state, request), target = "infradeck::ssh")]
pub async fn connection_exec(
    state: State<'_, AppState>,
    connection_id: String,
    request: ExecRequest,
) -> Result<ExecResult, AppError> {
    state
        .ssh
        .exec(&connection_id, request, CancellationToken::new())
        .await
        .map_err(AppError::from)
}

#[tauri::command]
pub fn tool_definitions_list(_server_id: Option<String>) -> Vec<ToolDefinition> {
    tools::definitions()
}

#[tauri::command]
#[instrument(skip(state, call), target = "infradeck::tool")]
pub async fn tool_execute(
    state: tauri::State<'_, AppState>,
    call: ToolCall,
) -> Result<ToolExecutionResponse, AppError> {
    execute_tool(state.inner(), call, "user").await
}

/// Shared Tool → Policy → Executor path used by both the UI command and the AI agent loop.
pub(crate) async fn execute_tool(
    state: &AppState,
    call: ToolCall,
    actor: &str,
) -> Result<ToolExecutionResponse, AppError> {
    let definition = match tools::resolve(&call.name, &call.version) {
        Some(value) => value,
        None => {
            return Ok(ToolExecutionResponse::Result {
                result: rejected_result(&call, "TOOL_NOT_FOUND", "工具或版本不存在", "failed"),
            })
        }
    };
    if let Err(message) = tools::validate_call(&call, &definition) {
        let result = rejected_result(&call, "TOOL_SCHEMA_INVALID", &message, "failed");
        append_tool_audit(state, &call, None, "deny", &result, actor)?;
        return Ok(ToolExecutionResponse::Result { result });
    }
    let profile = profile_for(state, call.target.server_id())?;
    let privilege = if profile.username == "root" {
        "root"
    } else if definition.metadata.requires_privilege {
        "sudo"
    } else {
        "user"
    };
    let mode = permission_mode(state)?;
    match policy::evaluate(&definition, &call, profile.environment, privilege, mode) {
        PolicyDecision::Deny(risk, reason) => {
            let mut result = rejected_result(&call, "POLICY_DENIED", &reason, "denied");
            result.meta.audit_id = uuid::Uuid::new_v4().to_string();
            append_tool_audit(state, &call, Some(&risk), "deny", &result, actor)?;
            Ok(ToolExecutionResponse::Result { result })
        }
        PolicyDecision::Confirm(risk) => {
            let approval = policy::approval_request(&definition, &call, risk.clone());
            {
                let db = state
                    .db
                    .lock()
                    .map_err(|_| AppError::Internal("database lock poisoned".into()))?;
                db.create_approval(&policy::record(&approval))?;
            }
            state
                .pending_tool_calls
                .lock()
                .map_err(|_| AppError::Internal("pending call lock poisoned".into()))?
                .insert(approval.approval_id.clone(), call.clone());
            let event = audit_for(
                &call,
                Some(&risk),
                actor,
                "confirm",
                "success",
                Some(approval.approval_id.clone()),
                serde_json::json!({"approvalCreated":true}),
            );
            state
                .db
                .lock()
                .map_err(|_| AppError::Internal("database lock poisoned".into()))?
                .append_audit(&event)?;
            Ok(ToolExecutionResponse::ApprovalRequired { approval })
        }
        PolicyDecision::Allow(risk) => {
            execute_allowed(state, call, risk, "allow", None, actor).await
        }
    }
}

#[tauri::command]
#[instrument(skip(state, grant), target = "infradeck::policy")]
pub async fn approval_resolve(
    state: State<'_, AppState>,
    grant: ApprovalGrant,
) -> Result<ToolExecutionResponse, AppError> {
    resolve_approval(state.inner(), grant).await
}

/// Shared approval resolution path: the UI command and QA integration tests
/// both go through this to exercise the full Tool → Policy → Approval chain.
pub(crate) async fn resolve_approval(
    state: &AppState,
    grant: ApprovalGrant,
) -> Result<ToolExecutionResponse, AppError> {
    let record = state
        .db
        .lock()
        .map_err(|_| AppError::Internal("database lock poisoned".into()))?
        .approval(&grant.approval_id)?
        .ok_or_else(|| AppError::Validation("approval 不存在".into()))?;
    let pending = state
        .pending_tool_calls
        .lock()
        .map_err(|_| AppError::Internal("pending call lock poisoned".into()))?
        .get(&grant.approval_id)
        .cloned();
    if record.status != ApprovalStatus::Pending || pending.is_none() {
        return Ok(ToolExecutionResponse::Result {
            result: replay_denied(record.tool_call_id, "approval 已过期、消费或无法恢复"),
        });
    }
    let call = pending.expect("checked pending");
    if grant.request_hash != record.request_hash || grant.request_hash.is_empty() {
        return denied_approval(
            state,
            call.id.clone(),
            Some(grant.approval_id.clone()),
            "approval hash 不匹配",
        );
    }
    let expires = DateTime::parse_from_rfc3339(&record.expires_at)
        .map_err(|error| AppError::Internal(error.to_string()))?
        .with_timezone(&Utc);
    if expires <= Utc::now() {
        state
            .db
            .lock()
            .map_err(|_| AppError::Internal("database lock poisoned".into()))?
            .resolve_approval(
                &grant.approval_id,
                ApprovalStatus::Pending,
                ApprovalStatus::Expired,
                "user",
            )?;
        state
            .pending_tool_calls
            .lock()
            .map_err(|_| AppError::Internal("pending call lock poisoned".into()))?
            .remove(&grant.approval_id);
        return denied_approval(
            state,
            call.id,
            Some(grant.approval_id.clone()),
            "approval 已过期",
        );
    }
    if grant.decision == ApprovalDecision::Reject {
        state
            .db
            .lock()
            .map_err(|_| AppError::Internal("database lock poisoned".into()))?
            .resolve_approval(
                &grant.approval_id,
                ApprovalStatus::Pending,
                ApprovalStatus::Rejected,
                "user",
            )?;
        state
            .pending_tool_calls
            .lock()
            .map_err(|_| AppError::Internal("pending call lock poisoned".into()))?
            .remove(&grant.approval_id);
        return denied_approval(
            state,
            call.id,
            Some(grant.approval_id.clone()),
            "用户拒绝执行",
        );
    }
    let definition = tools::resolve(&call.name, &call.version)
        .ok_or_else(|| AppError::Validation("工具不存在".into()))?;
    let profile = profile_for(state, call.target.server_id())?;
    let privilege = if profile.username == "root" {
        "root"
    } else {
        "sudo"
    };
    let risk = match policy::evaluate(
        &definition,
        &call,
        profile.environment,
        privilege,
        permission_mode(state)?,
    ) {
        PolicyDecision::Confirm(value) => value,
        _ => {
            return denied_approval(
                state,
                call.id,
                Some(grant.approval_id.clone()),
                "policy 已变化",
            )
        }
    };
    if let Err(reason) = policy::validate_approval(
        &record,
        &grant,
        &policy::request_hash(&definition, &call, &risk),
        &call.target.label(),
        Utc::now(),
    ) {
        return denied_approval(state, call.id, Some(grant.approval_id.clone()), reason);
    }
    if policy::request_hash(&definition, &call, &risk) != grant.request_hash {
        return denied_approval(
            state,
            call.id,
            Some(grant.approval_id.clone()),
            "approval 参数已变化",
        );
    }
    let expected = call.target.label();
    let required = if risk.level == policy::RiskLevel::High {
        RequiredConfirmation::TypeTarget
    } else {
        RequiredConfirmation::Button
    };
    if required == RequiredConfirmation::TypeTarget
        && grant.typed_confirmation.as_deref().map(str::trim) != Some(expected.as_str())
    {
        return denied_approval(
            state,
            call.id,
            Some(grant.approval_id.clone()),
            "高风险确认文本不匹配",
        );
    }
    {
        let mut db = state
            .db
            .lock()
            .map_err(|_| AppError::Internal("database lock poisoned".into()))?;
        if !db.resolve_approval(
            &grant.approval_id,
            ApprovalStatus::Pending,
            ApprovalStatus::Approved,
            "user",
        )? || !db.resolve_approval(
            &grant.approval_id,
            ApprovalStatus::Approved,
            ApprovalStatus::Consumed,
            "user",
        )? {
            return denied_approval(
                state,
                call.id,
                Some(grant.approval_id.clone()),
                "approval replay 被阻断",
            );
        }
    }
    state
        .pending_tool_calls
        .lock()
        .map_err(|_| AppError::Internal("pending call lock poisoned".into()))?
        .remove(&grant.approval_id);
    execute_allowed(
        state,
        call,
        risk,
        "confirm",
        Some(grant.approval_id),
        "user",
    )
    .await
}

#[tauri::command]
#[instrument(skip(state), target = "infradeck::storage")]
pub fn app_settings_get(
    state: State<'_, AppState>,
) -> Result<crate::config::AppSettings, AppError> {
    state
        .db
        .lock()
        .map_err(|_| AppError::Internal("database lock poisoned".into()))?
        .app_settings()
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettingsInput {
    pub permission_mode: crate::config::PermissionMode,
    pub conversation_persistence: bool,
}

#[tauri::command]
#[instrument(skip(state, input), target = "infradeck::storage")]
pub fn app_settings_save(
    state: State<'_, AppState>,
    input: AppSettingsInput,
) -> Result<crate::config::AppSettings, AppError> {
    let settings = crate::config::AppSettings {
        version: 1,
        permission_mode: input.permission_mode,
        telemetry_enabled: false,
        conversation_persistence: input.conversation_persistence,
    };
    let db = state
        .db
        .lock()
        .map_err(|_| AppError::Internal("database lock poisoned".into()))?;
    db.save_app_settings(&settings, input.conversation_persistence)?;
    Ok(settings)
}

#[tauri::command]
pub fn audit_events_list(
    state: State<'_, AppState>,
    limit: Option<usize>,
) -> Result<Vec<AuditEvent>, AppError> {
    state
        .db
        .lock()
        .map_err(|_| AppError::Internal("database lock poisoned".into()))?
        .list_audit(limit.unwrap_or(100))
}

async fn execute_allowed(
    state: &AppState,
    call: ToolCall,
    risk: policy::RiskAssessment,
    policy_action: &str,
    approval_id: Option<String>,
    actor: &str,
) -> Result<ToolExecutionResponse, AppError> {
    let connection_id = match state
        .ssh
        .active_connection_id(call.target.server_id())
        .await
    {
        Some(value) => value,
        None => {
            let result = rejected_result(
                &call,
                "SSH_CONNECTION_NOT_FOUND",
                "目标服务器未连接",
                "failed",
            );
            append_tool_audit(state, &call, Some(&risk), policy_action, &result, actor)?;
            return Ok(ToolExecutionResponse::Result { result });
        }
    };
    let audit_id = uuid::Uuid::new_v4().to_string();
    let result = tools::execute(&state.ssh, &connection_id, &call, audit_id).await;
    let mut event = audit_for(
        &call,
        Some(&risk),
        actor,
        policy_action,
        &result.status,
        approval_id,
        serde_json::json!({"summary":result.summary,"durationMs":result.meta.duration_ms}),
    );
    event.connection_id = Some(connection_id);
    state
        .db
        .lock()
        .map_err(|_| AppError::Internal("database lock poisoned".into()))?
        .append_audit(&event)?;
    Ok(ToolExecutionResponse::Result { result })
}

fn permission_mode(state: &AppState) -> Result<crate::config::PermissionMode, AppError> {
    Ok(state
        .db
        .lock()
        .map_err(|_| AppError::Internal("database lock poisoned".into()))?
        .app_settings()?
        .permission_mode)
}

fn profile_for(state: &AppState, server_id: &str) -> Result<ServerProfile, AppError> {
    state
        .db
        .lock()
        .map_err(|_| AppError::Internal("database lock poisoned".into()))?
        .list_server_profiles()?
        .into_iter()
        .find(|profile| profile.id == server_id)
        .ok_or_else(|| AppError::Validation("服务器配置不存在".into()))
}
fn rejected_result(call: &ToolCall, code: &str, message: &str, status: &str) -> ToolResult {
    let now = Utc::now().to_rfc3339();
    ToolResult {
        call_id: call.id.clone(),
        status: status.into(),
        data: None,
        summary: message.into(),
        evidence: Vec::new(),
        changed_resources: Vec::new(),
        warnings: Vec::new(),
        error: Some(crate::error::AppErrorDto {
            code: code.into(),
            message: message.into(),
            retryable: false,
            category: if code.starts_with("POLICY") {
                "policy".into()
            } else {
                "tool".into()
            },
            details: Some(serde_json::json!({"reason":message})),
        }),
        meta: ToolResultMeta {
            duration_ms: 0,
            truncated: false,
            started_at: now.clone(),
            finished_at: now,
            audit_id: uuid::Uuid::new_v4().to_string(),
        },
    }
}
/// A denied approval resolution always returns a denied result AND writes an
/// audit event, so every confirmation decision (reject/expiry/replay) is traceable.
fn denied_approval(
    state: &AppState,
    call_id: String,
    approval_id: Option<String>,
    message: &str,
) -> Result<ToolExecutionResponse, AppError> {
    let result = replay_denied(call_id, message);
    let details = serde_json::json!({"reason": message});
    let event = AuditEvent {
        id: uuid::Uuid::new_v4().to_string(),
        timestamp: Utc::now().to_rfc3339(),
        workspace_id: "default".into(),
        actor: "user".into(),
        server_id: None,
        connection_id: None,
        conversation_id: None,
        agent_run_id: None,
        action: "approval.resolve".into(),
        tool_name: None,
        tool_version: None,
        tool_call_id: Some(result.call_id.clone()),
        approval_id,
        risk_level: None,
        policy_action: Some("deny".into()),
        outcome: "denied".into(),
        arguments_digest: None,
        sanitized_details: details.as_object().cloned().unwrap_or_default(),
    };
    state
        .db
        .lock()
        .map_err(|_| AppError::Internal("database lock poisoned".into()))?
        .append_audit(&event)?;
    Ok(ToolExecutionResponse::Result { result })
}
fn replay_denied(call_id: String, message: &str) -> ToolResult {
    let call = ToolCall {
        id: call_id,
        name: "approval.resolve".into(),
        version: "1.0.0".into(),
        input: serde_json::json!({}),
        target: tools::ResourceTarget::Server {
            server_id: "unknown".into(),
        },
        requested_at: Utc::now().to_rfc3339(),
        conversation_id: None,
        agent_run_id: None,
    };
    rejected_result(&call, "POLICY_DENIED", message, "denied")
}
fn append_tool_audit(
    state: &AppState,
    call: &ToolCall,
    risk: Option<&policy::RiskAssessment>,
    action: &str,
    result: &ToolResult,
    actor: &str,
) -> Result<(), AppError> {
    let event = audit_for(
        call,
        risk,
        actor,
        action,
        &result.status,
        None,
        serde_json::json!({"summary":result.summary}),
    );
    state
        .db
        .lock()
        .map_err(|_| AppError::Internal("database lock poisoned".into()))?
        .append_audit(&event)
}
fn audit_for(
    call: &ToolCall,
    risk: Option<&policy::RiskAssessment>,
    actor: &str,
    action: &str,
    outcome: &str,
    approval_id: Option<String>,
    details: serde_json::Value,
) -> AuditEvent {
    AuditEvent {
        id: uuid::Uuid::new_v4().to_string(),
        timestamp: Utc::now().to_rfc3339(),
        workspace_id: "default".into(),
        actor: actor.into(),
        server_id: Some(call.target.server_id().into()),
        connection_id: None,
        conversation_id: call.conversation_id.clone(),
        agent_run_id: call.agent_run_id.clone(),
        action: "tool.execute".into(),
        tool_name: Some(call.name.clone()),
        tool_version: Some(call.version.clone()),
        tool_call_id: Some(call.id.clone()),
        approval_id,
        risk_level: risk.map(|v| v.level.as_str().into()),
        policy_action: Some(action.into()),
        outcome: outcome.into(),
        arguments_digest: Some(tools::arguments_digest(call)),
        sanitized_details: details.as_object().cloned().unwrap_or_default(),
    }
}

#[tauri::command]
#[instrument(skip(state), target = "infradeck::ssh")]
pub fn host_key_check(
    state: State<'_, AppState>,
    host: String,
    port: u16,
    algorithm: String,
    fingerprint_sha256: String,
) -> Result<HostKeyCheckDto, AppError> {
    let db = state
        .db
        .lock()
        .map_err(|_| AppError::Internal("database lock poisoned".into()))?;
    let previous = db.known_host_fingerprint(&host, port, &algorithm)?;
    Ok(evaluate(
        &host,
        port,
        &algorithm,
        &fingerprint_sha256,
        previous.as_deref(),
    ))
}

#[tauri::command]
#[instrument(skip(state, decision), target = "infradeck::ssh")]
pub fn host_key_resolve(
    state: State<'_, AppState>,
    decision: HostKeyDecision,
) -> Result<(), AppError> {
    let db = state
        .db
        .lock()
        .map_err(|_| AppError::Internal("database lock poisoned".into()))?;
    let previous = db.known_host_fingerprint(&decision.host, decision.port, &decision.algorithm)?;
    let check = evaluate(
        &decision.host,
        decision.port,
        &decision.algorithm,
        &decision.fingerprint_sha256,
        previous.as_deref(),
    );
    if !decision_allowed(check.status, decision.decision) {
        return Err(AppError::Validation(
            "当前 Host Key 状态不允许该决策".into(),
        ));
    }
    if matches!(
        decision.decision,
        HostKeyDecisionKind::TrustOnce | HostKeyDecisionKind::TrustAndSave
    ) {
        state
            .host_keys
            .add(&decision.host, decision.port, &decision.fingerprint_sha256);
    }
    if matches!(decision.decision, HostKeyDecisionKind::TrustAndSave) {
        db.save_known_host(
            &decision.host,
            decision.port,
            &decision.algorithm,
            &decision.fingerprint_sha256,
        )?;
    }
    Ok(())
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
