use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{
    config::PermissionMode,
    models::Environment,
    tools::{ToolCall, ToolDefinition},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalStatus {
    Pending,
    Approved,
    Rejected,
    Expired,
    Consumed,
}

impl ApprovalStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Approved => "approved",
            Self::Rejected => "rejected",
            Self::Expired => "expired",
            Self::Consumed => "consumed",
        }
    }
    pub fn parse(value: &str) -> Self {
        match value {
            "approved" => Self::Approved,
            "rejected" => Self::Rejected,
            "expired" => Self::Expired,
            "consumed" => Self::Consumed,
            _ => Self::Pending,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ApprovalRecord {
    pub id: String,
    pub tool_call_id: String,
    pub request_hash: String,
    pub risk_level: String,
    pub summary: String,
    pub impact: Vec<String>,
    pub status: ApprovalStatus,
    pub created_at: String,
    pub expires_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RiskAssessment {
    pub level: RiskLevel,
    pub score: u8,
    pub reasons: Vec<String>,
    pub matched_rules: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum RiskLevel {
    Safe,
    Caution,
    High,
    Blocked,
}

impl RiskLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Safe => "safe",
            Self::Caution => "caution",
            Self::High => "high",
            Self::Blocked => "blocked",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalRequest {
    pub approval_id: String,
    pub tool_call_id: String,
    pub request_hash: String,
    pub risk: RiskAssessment,
    pub summary: String,
    pub target_label: String,
    pub impact: Vec<String>,
    pub proposed_change: Option<ProposedChange>,
    pub expires_at: String,
    pub required_confirmation: RequiredConfirmation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalGrant {
    pub approval_id: String,
    pub request_hash: String,
    pub decision: ApprovalDecision,
    pub typed_confirmation: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ApprovalDecision {
    Approve,
    Reject,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum RequiredConfirmation {
    Button,
    TypeTarget,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProposedChange {
    pub kind: String,
    pub summary: String,
    pub before: Option<String>,
    pub after: Option<String>,
    pub verification_steps: Vec<String>,
}

#[derive(Debug, Clone)]
pub enum PolicyDecision {
    Allow(RiskAssessment),
    Confirm(RiskAssessment),
    Deny(RiskAssessment, String),
}

/// `mode` is the workspace permission mode selected in Settings; it can only
/// tighten decisions (fail-closed), never loosen a Deny or skip a hard block.
pub fn evaluate(
    definition: &ToolDefinition,
    call: &ToolCall,
    environment: Environment,
    privilege: &str,
    mode: PermissionMode,
) -> PolicyDecision {
    if let Some(rule) = hard_block(call) {
        let risk = RiskAssessment {
            level: RiskLevel::Blocked,
            score: 100,
            reasons: vec!["命中不可覆盖的高危规则".into()],
            matched_rules: vec![rule.into()],
        };
        return PolicyDecision::Deny(risk, "操作被安全策略永久阻断".into());
    }
    let mut score = match definition.metadata.risk_hint.as_str() {
        "caution" => 40,
        "high" => 70,
        _ => 0,
    };
    let mut reasons = Vec::new();
    if definition.metadata.mutation {
        score += 10;
        reasons.push("工具会修改远程资源".into());
    }
    score += match environment {
        Environment::Production => 20,
        Environment::Staging | Environment::Unknown => 10,
        Environment::Dev => 0,
    };
    score += match privilege {
        "root" => 15,
        "sudo" | "unknown" => 10,
        _ => 0,
    };
    let score = score.min(100);
    let mut level = if score < 30 {
        RiskLevel::Safe
    } else if score < 60 {
        RiskLevel::Caution
    } else {
        RiskLevel::High
    };
    if definition.metadata.mutation && level == RiskLevel::Safe {
        level = RiskLevel::Caution;
    }
    if definition.metadata.mutation && environment == Environment::Production {
        level = RiskLevel::High;
        reasons.push("生产环境变更最低为高风险".into());
    }
    let risk = RiskAssessment {
        level,
        score: score as u8,
        reasons,
        matched_rules: Vec::new(),
    };
    if definition.metadata.mutation {
        return PolicyDecision::Confirm(risk);
    }
    match mode {
        PermissionMode::ReadOnly => PolicyDecision::Deny(
            RiskAssessment {
                level: RiskLevel::High,
                score: score.max(60) as u8,
                reasons: vec!["只读权限模式下禁止工具执行".into()],
                matched_rules: vec!["MODE-READ-ONLY".into()],
            },
            "当前为只读权限模式，操作被拒绝".into(),
        ),
        PermissionMode::AskOnly => PolicyDecision::Confirm(risk),
        PermissionMode::ConfirmChanges | PermissionMode::Advanced => PolicyDecision::Allow(risk),
        PermissionMode::Restricted => {
            if call.name == "shell.execute" {
                PolicyDecision::Deny(
                    RiskAssessment {
                        level: RiskLevel::High,
                        score: score.max(60) as u8,
                        reasons: vec!["受限权限模式下禁用 shell fallback".into()],
                        matched_rules: vec!["MODE-RESTRICTED".into()],
                    },
                    "受限权限模式下 shell.execute 被禁用".into(),
                )
            } else {
                PolicyDecision::Allow(risk)
            }
        }
    }
}

fn hard_block(call: &ToolCall) -> Option<&'static str> {
    if call.name != "shell.execute" {
        return None;
    }
    let command = call.input.get("command")?.as_str()?.to_ascii_lowercase();
    if command.contains("mkfs") {
        return Some("HB-001");
    }
    if (command.contains("dd ") || command.contains('>'))
        && ["/dev/sd", "/dev/nvme", "/dev/disk"]
            .iter()
            .any(|p| command.contains(p))
    {
        return Some("HB-002");
    }
    if command.contains("rm -rf /") || command.contains("rm -fr /") {
        return Some("HB-003");
    }
    if (command.contains("curl") || command.contains("wget"))
        && command.contains('|')
        && [" sh", " bash", " python", " perl"]
            .iter()
            .any(|p| command.contains(p))
    {
        return Some("HB-004");
    }
    None
}

pub fn approval_request(
    definition: &ToolDefinition,
    call: &ToolCall,
    risk: RiskAssessment,
) -> ApprovalRequest {
    let target_label = call.target.label();
    let request_hash = request_hash(definition, call, &risk);
    ApprovalRequest {
        approval_id: Uuid::new_v4().to_string(),
        tool_call_id: call.id.clone(),
        request_hash,
        risk: risk.clone(),
        summary: format!("执行 {}", definition.title),
        target_label: target_label.clone(),
        impact: vec![format!("将修改 {target_label}")],
        proposed_change: Some(ProposedChange {
            kind: "action".into(),
            summary: format!("执行 {}", definition.name),
            before: None,
            after: None,
            verification_steps: vec!["执行后重新读取资源状态".into()],
        }),
        expires_at: (Utc::now() + Duration::minutes(5)).to_rfc3339(),
        required_confirmation: if risk.level == RiskLevel::High {
            RequiredConfirmation::TypeTarget
        } else {
            RequiredConfirmation::Button
        },
    }
}

pub fn request_hash(definition: &ToolDefinition, call: &ToolCall, risk: &RiskAssessment) -> String {
    let value = serde_json::json!({"toolCallId":call.id,"toolName":definition.name,"toolVersion":definition.version,"target":call.target,"input":call.input,"riskLevel":risk.level.as_str(),"serverId":call.target.server_id()});
    let canonical = canonicalize(&value);
    format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&canonical).expect("canonical json"))
    )
}

fn canonicalize(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut keys: Vec<_> = map.keys().collect();
            keys.sort();
            let mut out = Map::new();
            for key in keys {
                out.insert(key.clone(), canonicalize(&map[key]));
            }
            Value::Object(out)
        }
        Value::Array(items) => Value::Array(items.iter().map(canonicalize).collect()),
        other => other.clone(),
    }
}

pub fn record(request: &ApprovalRequest) -> ApprovalRecord {
    ApprovalRecord {
        id: request.approval_id.clone(),
        tool_call_id: request.tool_call_id.clone(),
        request_hash: request.request_hash.clone(),
        risk_level: request.risk.level.as_str().into(),
        summary: request.summary.clone(),
        impact: request.impact.clone(),
        status: ApprovalStatus::Pending,
        created_at: Utc::now().to_rfc3339(),
        expires_at: request.expires_at.clone(),
    }
}

pub fn validate_approval(
    record: &ApprovalRecord,
    grant: &ApprovalGrant,
    expected_hash: &str,
    target_label: &str,
    now: chrono::DateTime<Utc>,
) -> Result<(), &'static str> {
    if record.status != ApprovalStatus::Pending {
        return Err("HB-008: approval replay");
    }
    if record.request_hash != grant.request_hash
        || expected_hash != grant.request_hash
        || grant.request_hash.is_empty()
    {
        return Err("HB-007: approval hash mismatch");
    }
    let expires = chrono::DateTime::parse_from_rfc3339(&record.expires_at)
        .map_err(|_| "approval expiry invalid")?
        .with_timezone(&Utc);
    if expires <= now {
        return Err("HB-008: approval expired");
    }
    if record.risk_level == "high"
        && grant.decision == ApprovalDecision::Approve
        && grant.typed_confirmation.as_deref().map(str::trim) != Some(target_label)
    {
        return Err("high risk confirmation mismatch");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::{ResourceTarget, ToolMetadata};
    fn definition(mutation: bool) -> ToolDefinition {
        ToolDefinition {
            name: "service.restart".into(),
            version: "1.0.0".into(),
            title: "Restart".into(),
            description: "Restart".into(),
            input_schema: Value::Null,
            output_schema: Value::Null,
            metadata: ToolMetadata {
                mutation,
                risk_hint: "caution".into(),
                requires_privilege: true,
                timeout_ms: 30_000,
                supports_batch: false,
                capabilities: vec![],
            },
        }
    }
    fn call(name: &str, input: Value) -> ToolCall {
        ToolCall {
            id: Uuid::new_v4().to_string(),
            name: name.into(),
            version: "1.0.0".into(),
            input,
            target: ResourceTarget::Service {
                server_id: "s".into(),
                service: "nginx".into(),
            },
            requested_at: Utc::now().to_rfc3339(),
            conversation_id: None,
            agent_run_id: None,
        }
    }
    #[test]
    fn production_mutation_is_high() {
        let decision = evaluate(
            &definition(true),
            &call("service.restart", serde_json::json!({"service":"nginx"})),
            Environment::Production,
            "root",
            PermissionMode::ConfirmChanges,
        );
        assert!(matches!(
            decision,
            PolicyDecision::Confirm(RiskAssessment {
                level: RiskLevel::High,
                ..
            })
        ));
    }
    #[test]
    fn every_read_only_tool_is_allowed_without_approval() {
        for definition in crate::tools::definitions()
            .into_iter()
            .filter(|tool| !tool.metadata.mutation)
        {
            let call = ToolCall {
                id: Uuid::new_v4().to_string(),
                name: definition.name.clone(),
                version: definition.version.clone(),
                input: serde_json::json!({}),
                target: crate::tools::ResourceTarget::Server {
                    server_id: "server".into(),
                },
                requested_at: Utc::now().to_rfc3339(),
                conversation_id: None,
                agent_run_id: None,
            };
            assert!(
                matches!(
                    evaluate(
                        &definition,
                        &call,
                        Environment::Production,
                        "root",
                        PermissionMode::ConfirmChanges
                    ),
                    PolicyDecision::Allow(_)
                ),
                "{} unexpectedly requires approval",
                definition.name
            );
        }
    }
    #[test]
    fn hard_block_overrides_policy() {
        let mut d = definition(true);
        d.name = "shell.execute".into();
        let decision = evaluate(
            &d,
            &call("shell.execute", serde_json::json!({"command":"rm -rf /"})),
            Environment::Dev,
            "user",
            PermissionMode::ConfirmChanges,
        );
        assert!(matches!(
            decision,
            PolicyDecision::Deny(
                RiskAssessment {
                    level: RiskLevel::Blocked,
                    ..
                },
                _
            )
        ));
    }
    #[test]
    fn hash_is_stable_for_object_key_order() {
        let d = definition(true);
        let a = call("service.restart", serde_json::json!({"b":2,"a":1}));
        let mut b = a.clone();
        b.input = serde_json::from_str(r#"{"a":1,"b":2}"#).unwrap();
        let risk = RiskAssessment {
            level: RiskLevel::Caution,
            score: 50,
            reasons: vec![],
            matched_rules: vec![],
        };
        assert_eq!(request_hash(&d, &a, &risk), request_hash(&d, &b, &risk));
    }

    fn approval_fixture() -> (ApprovalRecord, ApprovalGrant) {
        (
            ApprovalRecord {
                id: "a".into(),
                tool_call_id: "c".into(),
                request_hash: "hash".into(),
                risk_level: "high".into(),
                summary: "restart".into(),
                impact: vec![],
                status: ApprovalStatus::Pending,
                created_at: Utc::now().to_rfc3339(),
                expires_at: (Utc::now() + Duration::minutes(5)).to_rfc3339(),
            },
            ApprovalGrant {
                approval_id: "a".into(),
                request_hash: "hash".into(),
                decision: ApprovalDecision::Approve,
                typed_confirmation: Some("server/nginx".into()),
            },
        )
    }
    #[test]
    fn approval_hash_mismatch_is_blocked() {
        let (record, mut grant) = approval_fixture();
        grant.request_hash = "tampered".into();
        assert!(
            validate_approval(&record, &grant, "hash", "server/nginx", Utc::now())
                .unwrap_err()
                .contains("HB-007")
        );
    }
    #[test]
    fn expired_approval_is_blocked() {
        let (mut record, grant) = approval_fixture();
        record.expires_at = (Utc::now() - Duration::seconds(1)).to_rfc3339();
        assert!(
            validate_approval(&record, &grant, "hash", "server/nginx", Utc::now())
                .unwrap_err()
                .contains("HB-008")
        );
    }
    #[test]
    fn consumed_approval_replay_is_blocked() {
        let (mut record, grant) = approval_fixture();
        record.status = ApprovalStatus::Consumed;
        assert!(
            validate_approval(&record, &grant, "hash", "server/nginx", Utc::now())
                .unwrap_err()
                .contains("HB-008")
        );
    }
    #[test]
    fn high_risk_requires_exact_target() {
        let (record, mut grant) = approval_fixture();
        grant.typed_confirmation = Some("other".into());
        assert!(validate_approval(&record, &grant, "hash", "server/nginx", Utc::now()).is_err());
    }

    #[test]
    fn read_only_mode_denies_even_read_only_tools() {
        // ReadOnly means "no tool execution at all", fail-closed for mutations too.
        let decision = evaluate(
            &definition(false),
            &call("service.status", serde_json::json!({"service":"nginx"})),
            Environment::Dev,
            "user",
            PermissionMode::ReadOnly,
        );
        assert!(matches!(decision, PolicyDecision::Deny(_, _)));
        let mutation = evaluate(
            &definition(true),
            &call("service.restart", serde_json::json!({"service":"nginx"})),
            Environment::Dev,
            "user",
            PermissionMode::ReadOnly,
        );
        assert!(
            matches!(mutation, PolicyDecision::Confirm(_)),
            "mutation stays on the confirm path"
        );
    }

    #[test]
    fn ask_only_mode_converts_reads_to_confirmations() {
        let decision = evaluate(
            &definition(false),
            &call("system.memory", serde_json::json!({})),
            Environment::Dev,
            "user",
            PermissionMode::AskOnly,
        );
        assert!(matches!(decision, PolicyDecision::Confirm(_)));
    }

    #[test]
    fn restricted_mode_blocks_shell_fallback_but_keeps_tools() {
        let mut shell = definition(false);
        shell.name = "shell.execute".into();
        let blocked = evaluate(
            &shell,
            &call("shell.execute", serde_json::json!({"command":"ls"})),
            Environment::Dev,
            "user",
            PermissionMode::Restricted,
        );
        assert!(matches!(blocked, PolicyDecision::Deny(_, _)));
        let allowed = evaluate(
            &definition(false),
            &call("system.memory", serde_json::json!({})),
            Environment::Dev,
            "user",
            PermissionMode::Restricted,
        );
        assert!(matches!(allowed, PolicyDecision::Allow(_)));
    }
}
