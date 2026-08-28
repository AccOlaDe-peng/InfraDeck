use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{collections::HashMap, time::Instant};
use tokio_util::sync::CancellationToken;

use crate::ssh::{ExecRequest, SshManager, SshProvider};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolMetadata {
    pub mutation: bool,
    pub risk_hint: String,
    pub requires_privilege: bool,
    pub timeout_ms: u64,
    pub supports_batch: bool,
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolDefinition {
    pub name: String,
    pub version: String,
    pub title: String,
    pub description: String,
    pub input_schema: Value,
    pub output_schema: Value,
    pub metadata: ToolMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub version: String,
    pub input: Value,
    pub target: ResourceTarget,
    pub requested_at: String,
    pub conversation_id: Option<String>,
    pub agent_run_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ResourceTarget {
    Server { server_id: String },
    Service { server_id: String, service: String },
    Process { server_id: String, pid: i64 },
}

impl ResourceTarget {
    pub fn server_id(&self) -> &str {
        match self {
            Self::Server { server_id }
            | Self::Service { server_id, .. }
            | Self::Process { server_id, .. } => server_id,
        }
    }
    pub fn label(&self) -> String {
        match self {
            Self::Server { server_id } => server_id.clone(),
            Self::Service { server_id, service } => format!("{server_id}/{service}"),
            Self::Process { server_id, pid } => format!("{server_id}/pid:{pid}"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolResult {
    pub call_id: String,
    pub status: String,
    pub data: Option<Value>,
    pub summary: String,
    pub evidence: Vec<EvidenceRef>,
    pub changed_resources: Vec<ResourceTarget>,
    pub warnings: Vec<String>,
    pub error: Option<crate::error::AppErrorDto>,
    pub meta: ToolResultMeta,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceRef {
    pub kind: String,
    pub label: String,
    pub digest_sha256: Option<String>,
    pub sanitized_excerpt: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolResultMeta {
    pub duration_ms: u64,
    pub truncated: bool,
    pub started_at: String,
    pub finished_at: String,
    pub audit_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditEvent {
    pub id: String,
    pub timestamp: String,
    pub workspace_id: String,
    pub actor: String,
    pub server_id: Option<String>,
    pub connection_id: Option<String>,
    pub conversation_id: Option<String>,
    pub agent_run_id: Option<String>,
    pub action: String,
    pub tool_name: Option<String>,
    pub tool_version: Option<String>,
    pub tool_call_id: Option<String>,
    pub approval_id: Option<String>,
    pub risk_level: Option<String>,
    pub policy_action: Option<String>,
    pub outcome: String,
    pub arguments_digest: Option<String>,
    pub sanitized_details: serde_json::Map<String, Value>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ToolExecutionResponse {
    Result {
        result: ToolResult,
    },
    ApprovalRequired {
        approval: crate::policy::ApprovalRequest,
    },
}

pub fn definitions() -> Vec<ToolDefinition> {
    vec![
        definition(
            "server.info",
            "Server information",
            false,
            "safe",
            10_000,
            empty_schema(),
        ),
        definition(
            "system.memory",
            "Memory usage",
            false,
            "safe",
            10_000,
            empty_schema(),
        ),
        definition(
            "system.disk",
            "Disk usage",
            false,
            "safe",
            10_000,
            object_schema(
                serde_json::json!({"path":{"type":"string","minLength":1,"maxLength":4096}}),
                &[],
            ),
        ),
        definition(
            "process.list",
            "Process list",
            false,
            "safe",
            10_000,
            object_schema(
                serde_json::json!({"sort":{"type":"string","enum":["memory","cpu","pid"]},"order":{"type":"string","enum":["asc","desc"]},"limit":{"type":"integer","minimum":1,"maximum":200}}),
                &[],
            ),
        ),
        definition(
            "process.inspect",
            "Process details",
            false,
            "safe",
            10_000,
            object_schema(
                serde_json::json!({"pid":{"type":"integer","minimum":1,"maximum":2147483647}}),
                &["pid"],
            ),
        ),
        definition(
            "network.ports",
            "Listening ports",
            false,
            "safe",
            10_000,
            object_schema(
                serde_json::json!({"protocol":{"type":"string","enum":["tcp","udp","all"]}}),
                &[],
            ),
        ),
        definition(
            "service.status",
            "Service status",
            false,
            "safe",
            10_000,
            service_schema(),
        ),
        definition(
            "service.logs",
            "Service logs",
            false,
            "safe",
            15_000,
            object_schema(
                serde_json::json!({"service":{"type":"string","minLength":1,"maxLength":128},"lines":{"type":"integer","minimum":1,"maximum":1000},"sinceMinutes":{"type":"integer","minimum":1,"maximum":10080}}),
                &["service"],
            ),
        ),
        definition(
            "service.restart",
            "Restart service",
            true,
            "caution",
            30_000,
            service_schema(),
        ),
        definition(
            "shell.execute",
            "Execute fallback shell command",
            true,
            "high",
            300_000,
            object_schema(
                serde_json::json!({"command":{"type":"string","minLength":1,"maxLength":32768},"cwd":{"type":"string","minLength":1,"maxLength":4096},"timeoutMs":{"type":"integer","minimum":1000,"maximum":300000},"purpose":{"type":"string","minLength":1,"maxLength":500}}),
                &["command", "timeoutMs", "purpose"],
            ),
        ),
    ]
}

fn definition(
    name: &str,
    title: &str,
    mutation: bool,
    risk: &str,
    timeout: u64,
    input_schema: Value,
) -> ToolDefinition {
    ToolDefinition {
        name: name.into(),
        version: "1.0.0".into(),
        title: title.into(),
        description: title.into(),
        input_schema,
        output_schema: serde_json::json!({"type":"object","properties":{},"additionalProperties":false}),
        metadata: ToolMetadata {
            mutation,
            risk_hint: risk.into(),
            requires_privilege: name == "service.restart",
            timeout_ms: timeout,
            supports_batch: false,
            capabilities: vec!["ssh.exec".into()],
        },
    }
}
fn empty_schema() -> Value {
    object_schema(serde_json::json!({}), &[])
}
fn service_schema() -> Value {
    object_schema(
        serde_json::json!({"service":{"type":"string","minLength":1,"maxLength":128}}),
        &["service"],
    )
}
fn object_schema(properties: Value, required: &[&str]) -> Value {
    serde_json::json!({"type":"object","properties":properties,"required":required,"additionalProperties":false})
}

pub fn resolve(name: &str, version: &str) -> Option<ToolDefinition> {
    definitions()
        .into_iter()
        .find(|d| d.name == name && d.version == version)
}

pub fn validate_call(call: &ToolCall, definition: &ToolDefinition) -> Result<(), String> {
    uuid::Uuid::parse_str(&call.id).map_err(|_| "invalid tool call id")?;
    let requested = DateTime::parse_from_rfc3339(&call.requested_at)
        .map_err(|_| "invalid requestedAt")?
        .with_timezone(&Utc);
    if (Utc::now() - requested).num_seconds().abs() > 300 {
        return Err("requestedAt outside allowed window".into());
    }
    let input = call.input.as_object().ok_or("input must be an object")?;
    let props = definition.input_schema["properties"]
        .as_object()
        .ok_or("invalid tool schema")?;
    if input.keys().any(|key| !props.contains_key(key)) {
        return Err("input contains unknown property".into());
    }
    for required in definition.input_schema["required"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
    {
        if !input.contains_key(required) {
            return Err(format!("missing required property: {required}"));
        }
    }
    validate_specific(call)?;
    if call.target.server_id().is_empty() {
        return Err("target serverId is required".into());
    }
    Ok(())
}

fn validate_specific(call: &ToolCall) -> Result<(), String> {
    let input = call.input.as_object().expect("validated object");
    match call.name.as_str() {
        "system.disk" => {
            if let Some(path) = input.get("path").and_then(Value::as_str) {
                if !path.starts_with('/') || path.len() > 4096 {
                    return Err("path must be absolute".into());
                }
            }
        }
        "process.list" => {
            if let Some(limit) = input.get("limit").and_then(Value::as_i64) {
                if !(1..=200).contains(&limit) {
                    return Err("limit out of range".into());
                }
            }
        }
        "process.inspect" => {
            let pid = input
                .get("pid")
                .and_then(Value::as_i64)
                .ok_or("pid is required")?;
            if !(1..=2147483647).contains(&pid) {
                return Err("pid out of range".into());
            }
            if !matches!(call.target,ResourceTarget::Process{pid:target,..} if target==pid) {
                return Err("target pid mismatch".into());
            }
        }
        "service.status" | "service.logs" | "service.restart" => {
            let service = input
                .get("service")
                .and_then(Value::as_str)
                .ok_or("service is required")?;
            if !valid_service(service) {
                return Err("invalid service name".into());
            }
            if !matches!(&call.target,ResourceTarget::Service{service:target,..} if target==service)
            {
                return Err("target service mismatch".into());
            }
        }
        "shell.execute" => {
            let timeout = input
                .get("timeoutMs")
                .and_then(Value::as_u64)
                .ok_or("timeoutMs is required")?;
            if !(1000..=300000).contains(&timeout) {
                return Err("timeout out of range".into());
            }
        }
        _ => {}
    }
    Ok(())
}
fn valid_service(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"@_.:-".contains(&b))
}
pub fn arguments_digest(call: &ToolCall) -> String {
    format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&call.input).unwrap_or_default())
    )
}

pub async fn execute<P: SshProvider>(
    manager: &SshManager<P>,
    connection_id: &str,
    call: &ToolCall,
    audit_id: String,
) -> ToolResult {
    let started_at = Utc::now();
    let timer = Instant::now();
    let definition = match resolve(&call.name, &call.version) {
        Some(value) => value,
        None => {
            return failed(
                call,
                audit_id,
                started_at,
                timer,
                "TOOL_NOT_FOUND",
                "工具不存在",
            )
        }
    };
    let request = match build_request(call, definition.metadata.timeout_ms) {
        Ok(value) => value,
        Err(message) => {
            return failed(
                call,
                audit_id,
                started_at,
                timer,
                "TOOL_SCHEMA_INVALID",
                &message,
            )
        }
    };
    let exec = match manager
        .exec(connection_id, request, CancellationToken::new())
        .await
    {
        Ok(value) => value,
        Err(error) => {
            return failed(
                call,
                audit_id,
                started_at,
                timer,
                "TOOL_EXEC_FAILED",
                &error.to_string(),
            )
        }
    };
    if exec.exit_code.unwrap_or(1) != 0 {
        return failed(
            call,
            audit_id,
            started_at,
            timer,
            "TOOL_EXEC_FAILED",
            "远程命令执行失败",
        );
    }
    let parsed = match parse_output(call, &exec.stdout) {
        Ok(value) => value,
        Err(message) => {
            return failed(
                call,
                audit_id,
                started_at,
                timer,
                "TOOL_SCHEMA_INVALID",
                &message,
            )
        }
    };
    let changed_resources = if definition.metadata.mutation {
        vec![call.target.clone()]
    } else {
        Vec::new()
    };
    let restart_verified = call.name != "service.restart"
        || (parsed["activeState"] == "active"
            && matches!(parsed["subState"].as_str(), Some("running" | "exited")));
    let status = if restart_verified {
        "success"
    } else {
        "partial"
    };
    let warnings = if restart_verified {
        Vec::new()
    } else {
        vec!["服务重启命令成功，但运行状态验证未通过".into()]
    };
    ToolResult {
        call_id: call.id.clone(),
        status: status.into(),
        summary: summary(call, &parsed),
        data: Some(parsed),
        evidence: vec![EvidenceRef {
            kind: "command".into(),
            label: format!("{} remote execution", call.name),
            digest_sha256: Some(format!("{:x}", Sha256::digest(exec.stdout.as_bytes()))),
            sanitized_excerpt: None,
        }],
        changed_resources,
        warnings,
        error: None,
        meta: ToolResultMeta {
            duration_ms: timer.elapsed().as_millis() as u64,
            truncated: exec.truncated,
            started_at: started_at.to_rfc3339(),
            finished_at: Utc::now().to_rfc3339(),
            audit_id,
        },
    }
}

fn build_request(call: &ToolCall, default_timeout: u64) -> Result<ExecRequest, String> {
    let value = |key: &str| call.input.get(key);
    let service = || {
        value("service")
            .and_then(Value::as_str)
            .ok_or_else(|| "service is required".to_string())
    };
    let command = match call.name.as_str() {
        "server.info" => "hostname; uname -s; uname -r; uname -m; awk -F= '$1==\"NAME\"||$1==\"VERSION_ID\"{gsub(/\"/,\"\",$2);print $1\"=\"$2}' /etc/os-release 2>/dev/null; awk '{print \"UPTIME=\"int($1)}' /proc/uptime 2>/dev/null".into(),
        "system.memory" => "cat /proc/meminfo".into(),
        "system.disk" => format!("LC_ALL=C df -B1 -P -- {}", shell_escape(value("path").and_then(Value::as_str).unwrap_or("/"))),
        "process.list" => "LC_ALL=C ps -eo pid=,ppid=,user=,pcpu=,pmem=,rss=,etimes=,comm=".into(),
        "process.inspect" => format!("LC_ALL=C ps -p {} -o pid=,ppid=,user=,pcpu=,pmem=,rss=,etimes=,comm=", value("pid").and_then(Value::as_i64).ok_or("pid is required")?),
        "network.ports" => "if command -v ss >/dev/null 2>&1; then ss -lntupH; else netstat -lntup 2>/dev/null; fi".into(),
        "service.status" => format!("systemctl show --no-page --property=LoadState,ActiveState,SubState,MainPID,UnitFileState,Description -- {}", shell_escape(&unit_name(service()?))),
        "service.logs" => { let lines=value("lines").and_then(Value::as_u64).unwrap_or(100); let since=value("sinceMinutes").and_then(Value::as_u64).map(|v|format!(" --since '-{v} minutes'")).unwrap_or_default(); format!("journalctl --no-pager -o short-iso-precise -n {lines} -u {}{since}",shell_escape(&unit_name(service()?))) },
        "service.restart" => format!("systemctl show --no-page --property=LoadState,ActiveState,SubState,MainPID,UnitFileState,Description -- {0}; printf '\n__INFRADECK_RESTART__\n'; if [ \"$(id -u)\" -eq 0 ]; then systemctl restart -- {0}; else sudo -n systemctl restart -- {0}; fi && sleep 0.5 && systemctl show --no-page --property=LoadState,ActiveState,SubState,MainPID,UnitFileState,Description -- {0}", shell_escape(&unit_name(service()?))),
        "shell.execute" => value("command").and_then(Value::as_str).ok_or("command is required")?.to_string(),
        _ => return Err("unknown tool".into()),
    };
    Ok(ExecRequest {
        command,
        timeout_ms: value("timeoutMs")
            .and_then(Value::as_u64)
            .unwrap_or(default_timeout),
        cwd: value("cwd").and_then(Value::as_str).map(str::to_owned),
        env: HashMap::new(),
        max_output_bytes: 1_048_576,
    })
}

fn shell_escape(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}
fn unit_name(value: &str) -> String {
    if value.contains('.') {
        value.into()
    } else {
        format!("{value}.service")
    }
}

fn parse_output(call: &ToolCall, stdout: &str) -> Result<Value, String> {
    match call.name.as_str() {
        "server.info" => parse_server_info(stdout),
        "system.memory" => parse_memory(stdout),
        "system.disk" => parse_disk(stdout),
        "process.list" => parse_processes(stdout, call),
        "process.inspect" => parse_process(stdout),
        "network.ports" => parse_ports(stdout),
        "service.status" | "service.restart" => parse_service(stdout, call),
        "service.logs" => Ok(
            serde_json::json!({"service":call.input["service"],"entries":stdout.lines().take(1000).map(|line|serde_json::json!({"message":redact(line)})).collect::<Vec<_>>()}),
        ),
        "shell.execute" => Ok(
            serde_json::json!({"completed":true,"outputDigest":format!("{:x}",Sha256::digest(stdout.as_bytes()))}),
        ),
        _ => Err("unsupported parser".into()),
    }
}

fn parse_server_info(s: &str) -> Result<Value, String> {
    let lines: Vec<_> = s.lines().collect();
    if lines.len() < 4 {
        return Err("parse_failed".into());
    }
    let mut os_name = None;
    let mut os_version = None;
    let mut uptime = None;
    for line in &lines[4..] {
        if let Some(v) = line.strip_prefix("NAME=") {
            os_name = Some(v)
        } else if let Some(v) = line.strip_prefix("VERSION_ID=") {
            os_version = Some(v)
        } else if let Some(v) = line.strip_prefix("UPTIME=") {
            uptime = v.parse::<u64>().ok()
        }
    }
    Ok(
        serde_json::json!({"hostname":lines[0],"osName":os_name.unwrap_or(lines[1]),"osVersion":os_version,"kernel":format!("{} {}",lines[1],lines[2]),"architecture":lines[3],"uptimeSec":uptime}),
    )
}
fn parse_memory(s: &str) -> Result<Value, String> {
    let mut m = HashMap::new();
    for line in s.lines() {
        let p: Vec<_> = line.split_whitespace().collect();
        if p.len() >= 3 && p[2] == "kB" {
            if let Ok(v) = p[1].parse::<u64>() {
                m.insert(p[0].trim_end_matches(':'), v * 1024);
            }
        }
    }
    let total = *m.get("MemTotal").ok_or("parse_failed")?;
    let available = *m.get("MemAvailable").ok_or("parse_failed")?;
    if total == 0 {
        return Err("parse_failed".into());
    }
    let used = total.saturating_sub(available);
    Ok(
        serde_json::json!({"totalBytes":total,"availableBytes":available,"usedBytes":used,"freeBytes":m.get("MemFree").copied().unwrap_or(0),"buffersBytes":m.get("Buffers").copied().unwrap_or(0),"cachedBytes":m.get("Cached").copied().unwrap_or(0),"swapTotalBytes":m.get("SwapTotal").copied().unwrap_or(0),"swapUsedBytes":m.get("SwapTotal").copied().unwrap_or(0).saturating_sub(m.get("SwapFree").copied().unwrap_or(0)),"usedPercent":((used as f64/total as f64)*10000.0).round()/100.0}),
    )
}
fn parse_disk(s: &str) -> Result<Value, String> {
    let mut mounts = Vec::new();
    for line in s.lines().skip(1) {
        let p: Vec<_> = line.split_whitespace().collect();
        if p.len() < 6 {
            continue;
        }
        mounts.push(serde_json::json!({"filesystem":p[0],"totalBytes":p[1].parse::<u64>().unwrap_or(0),"usedBytes":p[2].parse::<u64>().unwrap_or(0),"availableBytes":p[3].parse::<u64>().unwrap_or(0),"usedPercent":p[4].trim_end_matches('%').parse::<f64>().unwrap_or(0.0),"mountPoint":p[5]}));
    }
    if mounts.is_empty() {
        Err("parse_failed".into())
    } else {
        Ok(serde_json::json!({"mounts":mounts}))
    }
}
fn process_value(line: &str) -> Option<Value> {
    let p: Vec<_> = line.split_whitespace().collect();
    if p.len() < 8 {
        return None;
    }
    Some(
        serde_json::json!({"pid":p[0].parse::<i64>().ok()?,"ppid":p[1].parse::<i64>().ok()?,"user":p[2],"cpuPercent":p[3].parse::<f64>().ok()?,"memoryPercent":p[4].parse::<f64>().ok()?,"rssBytes":p[5].parse::<u64>().ok()?*1024,"elapsedSec":p[6].parse::<u64>().ok()?,"command":redact(&p[7..].join(" "))}),
    )
}
fn parse_processes(s: &str, call: &ToolCall) -> Result<Value, String> {
    let mut rows: Vec<_> = s.lines().filter_map(process_value).collect();
    let sort = call
        .input
        .get("sort")
        .and_then(Value::as_str)
        .unwrap_or("memory");
    rows.sort_by(|a, b| {
        let key = |v: &Value| match sort {
            "cpu" => v["cpuPercent"].as_f64().unwrap_or(0.0),
            "pid" => v["pid"].as_f64().unwrap_or(0.0),
            _ => v["memoryPercent"].as_f64().unwrap_or(0.0),
        };
        key(a)
            .partial_cmp(&key(b))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    if call
        .input
        .get("order")
        .and_then(Value::as_str)
        .unwrap_or("desc")
        == "desc"
    {
        rows.reverse()
    }
    rows.truncate(
        call.input
            .get("limit")
            .and_then(Value::as_u64)
            .unwrap_or(20) as usize,
    );
    Ok(serde_json::json!({"processes":rows}))
}
fn parse_process(s: &str) -> Result<Value, String> {
    s.lines()
        .find_map(process_value)
        .ok_or_else(|| "process_not_found".into())
}
fn parse_ports(s: &str) -> Result<Value, String> {
    let ports = s
        .lines()
        .filter_map(|line| {
            let proto = if line.starts_with("tcp") {
                "tcp"
            } else if line.starts_with("udp") {
                "udp"
            } else {
                return None;
            };
            let token = line.split_whitespace().find(|p| {
                p.rsplit(':')
                    .next()
                    .is_some_and(|v| v.parse::<u16>().is_ok())
            })?;
            let port = token.rsplit(':').next()?.parse::<u16>().ok()?;
            Some(serde_json::json!({"protocol":proto,"localAddress":token,"port":port}))
        })
        .collect::<Vec<_>>();
    Ok(serde_json::json!({"ports":ports}))
}
fn parse_service(s: &str, call: &ToolCall) -> Result<Value, String> {
    let mut m = HashMap::new();
    for line in s.lines() {
        if let Some((k, v)) = line.split_once('=') {
            m.insert(k, v);
        }
    }
    if !m.contains_key("ActiveState") {
        return Err("systemd_unavailable".into());
    }
    Ok(
        serde_json::json!({"service":call.input["service"],"loadState":m.get("LoadState").copied().unwrap_or("unknown"),"activeState":m.get("ActiveState").copied().unwrap_or("unknown"),"subState":m.get("SubState").copied().unwrap_or("unknown"),"mainPid":m.get("MainPID").and_then(|v|v.parse::<u64>().ok()),"unitFileState":m.get("UnitFileState"),"description":m.get("Description")}),
    )
}
pub(crate) fn redact(value: &str) -> String {
    let lowered = value.to_ascii_lowercase();
    if ["password=", "token=", "secret=", "authorization:"]
        .iter()
        .any(|p| lowered.contains(p))
    {
        "[REDACTED]".into()
    } else {
        value.chars().take(4096).collect()
    }
}
fn summary(call: &ToolCall, data: &Value) -> String {
    match call.name.as_str() {
        "system.memory" => format!("内存使用率 {}%", data["usedPercent"]),
        "system.disk" => format!(
            "返回 {} 个挂载点",
            data["mounts"].as_array().map_or(0, Vec::len)
        ),
        "process.list" => format!(
            "返回 {} 个进程",
            data["processes"].as_array().map_or(0, Vec::len)
        ),
        "service.status" => format!("服务状态：{}", data["activeState"]),
        "service.restart" => format!("服务已重启并验证：{}", data["activeState"]),
        _ => format!("{} 执行成功", call.name),
    }
}
fn failed(
    call: &ToolCall,
    audit_id: String,
    started: chrono::DateTime<Utc>,
    timer: Instant,
    code: &str,
    message: &str,
) -> ToolResult {
    let error = crate::error::AppErrorDto {
        code: code.into(),
        message: message.into(),
        retryable: code == "TOOL_EXEC_FAILED",
        category: "tool".into(),
        details: Some(serde_json::json!({"reason":message})),
    };
    ToolResult {
        call_id: call.id.clone(),
        status: "failed".into(),
        data: None,
        summary: message.chars().take(2000).collect(),
        evidence: Vec::new(),
        changed_resources: Vec::new(),
        warnings: Vec::new(),
        error: Some(error),
        meta: ToolResultMeta {
            duration_ms: timer.elapsed().as_millis() as u64,
            truncated: false,
            started_at: started.to_rfc3339(),
            finished_at: Utc::now().to_rfc3339(),
            audit_id,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn registry_has_unique_names() {
        let defs = definitions();
        let mut names = std::collections::HashSet::new();
        for d in defs {
            assert!(names.insert((d.name, d.version)));
        }
    }
    #[test]
    fn registry_rejects_unknown_name_and_version() {
        assert!(resolve("unknown.tool", "1.0.0").is_none());
        assert!(resolve("system.memory", "2.0.0").is_none());
    }
    #[test]
    fn rejects_extra_property() {
        let d = resolve("server.info", "1.0.0").unwrap();
        let c = ToolCall {
            id: uuid::Uuid::new_v4().to_string(),
            name: d.name.clone(),
            version: d.version.clone(),
            input: serde_json::json!({"extra":true}),
            target: ResourceTarget::Server {
                server_id: "s".into(),
            },
            requested_at: Utc::now().to_rfc3339(),
            conversation_id: None,
            agent_run_id: None,
        };
        assert!(validate_call(&c, &d).is_err());
    }

    #[test]
    fn resource_target_uses_camel_case_contract() {
        let target = ResourceTarget::Service {
            server_id: "server".into(),
            service: "nginx".into(),
        };
        assert_eq!(
            serde_json::to_value(target).unwrap(),
            serde_json::json!({"kind":"service","serverId":"server","service":"nginx"})
        );
    }

    #[test]
    fn parses_linux_memory_fixture() {
        let value = parse_memory("MemTotal:       1000 kB\nMemFree:         100 kB\nMemAvailable:    400 kB\nBuffers:          10 kB\nCached:          200 kB\nSwapTotal:       500 kB\nSwapFree:        300 kB\n").unwrap();
        assert_eq!(value["totalBytes"], 1_024_000);
        assert_eq!(value["usedPercent"], 60.0);
        assert_eq!(value["swapUsedBytes"], 204_800);
    }

    #[test]
    fn parses_disk_fixture_and_rejects_garbage() {
        let value = parse_disk("Filesystem 1024-blocks Used Available Capacity Mounted on\n/dev/sda1 1000 600 400 60% /\n").unwrap();
        assert_eq!(value["mounts"][0]["mountPoint"], "/");
        assert!(parse_disk("localized garbage").is_err());
    }

    #[test]
    fn redacts_suspected_secrets_from_logs() {
        assert_eq!(redact("token=top-secret"), "[REDACTED]");
        assert_eq!(redact("service ready"), "service ready");
    }

    #[test]
    fn parses_process_service_and_port_fixtures() {
        let process = process_value("42 1 root 2.5 1.0 128 60 nginx").unwrap();
        assert_eq!(process["pid"], 42);
        assert_eq!(process["rssBytes"], 131_072);
        let call = ToolCall {
            id: uuid::Uuid::new_v4().to_string(),
            name: "service.status".into(),
            version: "1.0.0".into(),
            input: serde_json::json!({"service":"nginx"}),
            target: ResourceTarget::Service {
                server_id: "s".into(),
                service: "nginx".into(),
            },
            requested_at: Utc::now().to_rfc3339(),
            conversation_id: None,
            agent_run_id: None,
        };
        let service = parse_service(
            "LoadState=loaded\nActiveState=active\nSubState=running\nMainPID=42\n",
            &call,
        )
        .unwrap();
        assert_eq!(service["activeState"], "active");
        let ports = parse_ports("tcp LISTEN 0 128 0.0.0.0:22 0.0.0.0:* users:((\"sshd\",pid=1,fd=3))\nudp UNCONN 0 0 0.0.0.0:53 0.0.0.0:*").unwrap();
        assert_eq!(ports["ports"].as_array().unwrap().len(), 2);
    }
}
