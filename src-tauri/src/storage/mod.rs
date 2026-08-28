use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use std::{
    fs,
    path::{Path, PathBuf},
};
use tracing::{info, instrument};

use crate::{
    error::AppError,
    models::{AuthRef, Environment, ServerProfile},
    policy::{ApprovalRecord, ApprovalStatus},
    tools::AuditEvent,
};

pub struct Database {
    conn: Connection,
    path: PathBuf,
}

impl Database {
    pub fn open_default() -> Result<Self, AppError> {
        let dir = crate::platform::app_data_dir();
        fs::create_dir_all(&dir)
            .map_err(|e| AppError::Internal(format!("create data directory: {e}")))?;
        Self::open(dir.join("infradeck.sqlite3"))
    }

    pub fn open(path: impl AsRef<Path>) -> Result<Self, AppError> {
        let path = path.as_ref().to_path_buf();
        let conn = Connection::open(&path)?;
        conn.execute_batch("PRAGMA foreign_keys = ON; PRAGMA busy_timeout = 5000;")?;
        let mut db = Self { conn, path };
        db.migrate()?;
        info!(target: "infradeck::storage", path = %db.path.display(), "database ready");
        Ok(db)
    }

    fn migrate(&mut self) -> Result<(), AppError> {
        self.conn.execute_batch("CREATE TABLE IF NOT EXISTS schema_migrations (version INTEGER PRIMARY KEY, applied_at TEXT NOT NULL);")?;
        let applied: Option<i64> =
            self.conn
                .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
                    row.get(0)
                })?;
        let current = applied.unwrap_or(0);
        let migrations: &[(i64, &str)] = &[
            (1, include_str!("../../migrations/0001_initial.sql")),
            (2, include_str!("../../migrations/0002_v01_contracts.sql")),
            (3, include_str!("../../migrations/0003_tool_policy.sql")),
        ];
        for (version, sql) in migrations.iter().filter(|(version, _)| *version > current) {
            let tx = self.conn.transaction()?;
            tx.execute_batch(sql)?;
            tx.execute(
                "INSERT OR IGNORE INTO schema_migrations(version, applied_at) VALUES (?1, ?2)",
                params![version, Utc::now().to_rfc3339()],
            )?;
            tx.commit()?;
        }
        Ok(())
    }

    #[instrument(skip(self, profile), target = "infradeck::storage")]
    pub fn upsert_server_profile(&self, profile: &ServerProfile) -> Result<(), AppError> {
        let now = Utc::now().to_rfc3339();
        let (auth_kind, credential_ref, key_path) = match &profile.auth {
            AuthRef::Password { credential_id } => ("password", Some(credential_id.as_str()), None),
            AuthRef::PrivateKey {
                key_path,
                passphrase_credential_id,
            } => (
                "privateKey",
                passphrase_credential_id.as_deref(),
                Some(key_path.as_str()),
            ),
            AuthRef::Agent => ("agent", None, None),
        };
        self.conn.execute("INSERT INTO servers(id,name,host,port,username,auth_kind,credential_ref,key_path,environment,tags_json,connect_timeout_ms,keep_alive_interval_sec,created_at,updated_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14) ON CONFLICT(id) DO UPDATE SET name=excluded.name,host=excluded.host,port=excluded.port,username=excluded.username,auth_kind=excluded.auth_kind,credential_ref=excluded.credential_ref,key_path=excluded.key_path,environment=excluded.environment,tags_json=excluded.tags_json,connect_timeout_ms=excluded.connect_timeout_ms,keep_alive_interval_sec=excluded.keep_alive_interval_sec,updated_at=excluded.updated_at", params![profile.id, profile.name, profile.host, profile.port, profile.username, auth_kind, credential_ref, key_path, profile.environment.as_str(), serde_json::to_string(&profile.tags).map_err(|e| AppError::Internal(e.to_string()))?, profile.connect_timeout_ms, profile.keep_alive_interval_sec, if profile.created_at.is_empty() { now.clone() } else { profile.created_at.clone() }, now])?;
        Ok(())
    }

    pub fn list_server_profiles(&self) -> Result<Vec<ServerProfile>, AppError> {
        let mut stmt = self.conn.prepare("SELECT id,name,host,port,username,auth_kind,credential_ref,key_path,environment,tags_json,connect_timeout_ms,keep_alive_interval_sec,created_at,updated_at FROM servers ORDER BY updated_at DESC")?;
        let rows = stmt.query_map([], |row| {
            let auth_kind: String = row.get(5)?;
            let credential_ref: Option<String> = row.get(6)?;
            let key_path: Option<String> = row.get(7)?;
            let auth = match auth_kind.as_str() {
                "password" => AuthRef::Password {
                    credential_id: credential_ref.unwrap_or_default(),
                },
                "privateKey" => AuthRef::PrivateKey {
                    key_path: key_path.unwrap_or_default(),
                    passphrase_credential_id: credential_ref,
                },
                _ => AuthRef::Agent,
            };
            let environment = match row.get::<_, String>(8)?.as_str() {
                "dev" => Environment::Dev,
                "staging" => Environment::Staging,
                "production" => Environment::Production,
                _ => Environment::Unknown,
            };
            let tags_json: String = row.get(9)?;
            Ok(ServerProfile {
                id: row.get(0)?,
                name: row.get(1)?,
                host: row.get(2)?,
                port: row.get(3)?,
                username: row.get(4)?,
                auth,
                environment,
                tags: serde_json::from_str(&tags_json).unwrap_or_default(),
                connect_timeout_ms: row.get(10)?,
                keep_alive_interval_sec: row.get(11)?,
                created_at: row.get(12)?,
                updated_at: row.get(13)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(AppError::from)
    }

    pub fn known_host_fingerprint(
        &self,
        host: &str,
        port: u16,
        key_type: &str,
    ) -> Result<Option<String>, AppError> {
        self.conn
            .query_row(
                "SELECT fingerprint FROM known_hosts WHERE host=?1 AND port=?2 AND key_type=?3",
                params![host, port, key_type],
                |row| row.get(0),
            )
            .optional()
            .map_err(AppError::from)
    }

    pub fn save_known_host(
        &self,
        host: &str,
        port: u16,
        key_type: &str,
        fingerprint: &str,
    ) -> Result<(), AppError> {
        let now = Utc::now().to_rfc3339();
        self.conn.execute("INSERT INTO known_hosts(host,port,key_type,fingerprint,first_seen_at,last_seen_at) VALUES (?1,?2,?3,?4,?5,?5) ON CONFLICT(host,port,key_type) DO UPDATE SET fingerprint=excluded.fingerprint,last_seen_at=excluded.last_seen_at", params![host, port, key_type, fingerprint, now])?;
        Ok(())
    }

    pub fn list_known_host_fingerprints(&self) -> Result<Vec<(String, u16, String)>, AppError> {
        let mut stmt = self
            .conn
            .prepare("SELECT host, port, fingerprint FROM known_hosts")?;
        let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?;
        rows.collect::<Result<Vec<_>, _>>().map_err(AppError::from)
    }

    pub fn append_audit(&self, event: &AuditEvent) -> Result<(), AppError> {
        self.conn.execute(
            "INSERT INTO audit_events(id,timestamp,workspace_id,actor,server_id,connection_id,conversation_id,agent_run_id,action,tool_name,tool_version,tool_call_id,approval_id,risk_level,policy_action,outcome,arguments_digest,details_json) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18)",
            params![event.id,event.timestamp,event.workspace_id,event.actor,event.server_id,event.connection_id,event.conversation_id,event.agent_run_id,event.action,event.tool_name,event.tool_version,event.tool_call_id,event.approval_id,event.risk_level,event.policy_action,event.outcome,event.arguments_digest,serde_json::to_string(&event.sanitized_details).map_err(|error| AppError::Internal(error.to_string()))?],
        )?;
        Ok(())
    }

    pub fn list_audit(&self, limit: usize) -> Result<Vec<AuditEvent>, AppError> {
        let mut stmt = self.conn.prepare("SELECT id,timestamp,workspace_id,actor,server_id,connection_id,conversation_id,agent_run_id,action,tool_name,tool_version,tool_call_id,approval_id,risk_level,policy_action,outcome,arguments_digest,details_json FROM audit_events ORDER BY timestamp DESC LIMIT ?1")?;
        let rows = stmt.query_map([limit.min(500) as i64], |row| {
            let details: String = row.get(17)?;
            Ok(AuditEvent {
                id: row.get(0)?,
                timestamp: row.get(1)?,
                workspace_id: row.get(2)?,
                actor: row.get(3)?,
                server_id: row.get(4)?,
                connection_id: row.get(5)?,
                conversation_id: row.get(6)?,
                agent_run_id: row.get(7)?,
                action: row.get(8)?,
                tool_name: row.get(9)?,
                tool_version: row.get(10)?,
                tool_call_id: row.get(11)?,
                approval_id: row.get(12)?,
                risk_level: row.get(13)?,
                policy_action: row.get(14)?,
                outcome: row.get(15)?,
                arguments_digest: row.get(16)?,
                sanitized_details: serde_json::from_str(&details).unwrap_or_default(),
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(AppError::from)
    }

    pub fn create_approval(&self, approval: &ApprovalRecord) -> Result<(), AppError> {
        self.conn.execute("INSERT INTO approvals(id,tool_call_id,request_hash,risk_level,summary,impact_json,status,created_at,expires_at) VALUES (?1,?2,?3,?4,?5,?6,'pending',?7,?8)", params![approval.id,approval.tool_call_id,approval.request_hash,approval.risk_level,approval.summary,serde_json::to_string(&approval.impact).map_err(|error| AppError::Internal(error.to_string()))?,approval.created_at,approval.expires_at])?;
        Ok(())
    }

    pub fn approval(&self, id: &str) -> Result<Option<ApprovalRecord>, AppError> {
        self.conn.query_row("SELECT id,tool_call_id,request_hash,risk_level,summary,impact_json,status,created_at,expires_at FROM approvals WHERE id=?1", [id], |row| {
            let status: String = row.get(6)?;
            let impact: String = row.get(5)?;
            Ok(ApprovalRecord { id: row.get(0)?, tool_call_id: row.get(1)?, request_hash: row.get(2)?, risk_level: row.get(3)?, summary: row.get(4)?, impact: serde_json::from_str(&impact).unwrap_or_default(), status: ApprovalStatus::parse(&status), created_at: row.get(7)?, expires_at: row.get(8)? })
        }).optional().map_err(AppError::from)
    }

    pub fn resolve_approval(
        &mut self,
        id: &str,
        from: ApprovalStatus,
        to: ApprovalStatus,
        actor: &str,
    ) -> Result<bool, AppError> {
        let tx = self.conn.transaction()?;
        let now = Utc::now().to_rfc3339();
        let changed = tx.execute("UPDATE approvals SET status=?1,approved_by=?2,resolved_at=?3,consumed_at=CASE WHEN ?1='consumed' THEN ?3 ELSE consumed_at END WHERE id=?4 AND status=?5", params![to.as_str(),actor,now,id,from.as_str()])? == 1;
        tx.commit()?;
        Ok(changed)
    }

    pub fn ai_provider_settings(&self) -> Result<Option<crate::ai::AiProviderSettings>, AppError> {
        self.conn.query_row("SELECT provider_kind,base_url,model,api_key_credential_id,max_tool_iterations,max_tool_output_chars,updated_at FROM ai_provider_settings WHERE id=1", [], |row| {
            Ok(crate::ai::AiProviderSettings {
                provider_kind: row.get(0)?,
                base_url: row.get(1)?,
                model: row.get(2)?,
                api_key_credential_id: row.get(3)?,
                max_tool_iterations: row.get::<_, i64>(4)?.max(1) as u32,
                max_tool_output_chars: row.get::<_, i64>(5)?.max(500) as u32,
                updated_at: row.get(6)?,
            })
        })
        .optional()
        .map_err(AppError::from)
    }

    pub fn save_ai_provider_settings(
        &self,
        settings: &crate::ai::AiProviderSettings,
    ) -> Result<(), AppError> {
        self.conn.execute(
            "UPDATE ai_provider_settings SET provider_kind=?1,base_url=?2,model=?3,api_key_credential_id=?4,max_tool_iterations=?5,max_tool_output_chars=?6,updated_at=?7 WHERE id=1",
            params![settings.provider_kind,settings.base_url,settings.model,settings.api_key_credential_id,settings.max_tool_iterations,settings.max_tool_output_chars,settings.updated_at],
        )?;
        Ok(())
    }

    pub fn app_settings(&self) -> Result<crate::config::AppSettings, AppError> {
        let row = self.conn.query_row(
            "SELECT permission_mode,telemetry_enabled,conversation_persistence_enabled FROM app_settings WHERE id=1",
            [],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            },
        );
        let default = || crate::config::AppSettings {
            telemetry_enabled: false,
            conversation_persistence: true,
            ..crate::config::AppSettings::default()
        };
        match row {
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(default()),
            Err(error) => Err(error.into()),
            Ok((mode, telemetry, persistence)) => Ok(crate::config::AppSettings {
                version: 1,
                permission_mode: match mode.as_str() {
                    "readOnly" => crate::config::PermissionMode::ReadOnly,
                    "advanced" => crate::config::PermissionMode::Advanced,
                    "restricted" => crate::config::PermissionMode::Restricted,
                    "askOnly" => crate::config::PermissionMode::AskOnly,
                    _ => crate::config::PermissionMode::ConfirmChanges,
                },
                telemetry_enabled: telemetry != 0,
                conversation_persistence: persistence != 0,
            }),
        }
    }

    pub fn save_app_settings(
        &self,
        settings: &crate::config::AppSettings,
        conversation_persistence: bool,
    ) -> Result<(), AppError> {
        let mode = match settings.permission_mode {
            crate::config::PermissionMode::AskOnly => "askOnly",
            crate::config::PermissionMode::ReadOnly => "readOnly",
            crate::config::PermissionMode::ConfirmChanges => "confirmChanges",
            crate::config::PermissionMode::Advanced => "advanced",
            crate::config::PermissionMode::Restricted => "restricted",
        };
        self.conn.execute(
            "UPDATE app_settings SET permission_mode=?1,telemetry_enabled=?2,conversation_persistence_enabled=?3,updated_at=?4 WHERE id=1",
            params![mode, settings.telemetry_enabled as i64, conversation_persistence as i64, Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    pub fn expire_stale_approvals(&self) -> Result<usize, AppError> {
        self.conn.execute("UPDATE approvals SET status='expired',resolved_at=?1 WHERE status IN ('pending','approved')", [Utc::now().to_rfc3339()]).map_err(AppError::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;
    #[test]
    fn migrates_and_round_trips_server_profile() {
        let mut db = Database::open(":memory:").expect("database");
        let profile = ServerProfile {
            id: Uuid::new_v4().to_string(),
            name: "test".into(),
            host: "localhost".into(),
            port: 22,
            username: "dev".into(),
            auth: AuthRef::Agent,
            environment: Environment::Dev,
            tags: vec!["local".into()],
            connect_timeout_ms: 15_000,
            keep_alive_interval_sec: 30,
            created_at: String::new(),
            updated_at: String::new(),
        };
        db.upsert_server_profile(&profile).expect("insert");
        let profiles = db.list_server_profiles().expect("list");
        assert_eq!(profiles[0].host, "localhost");
        assert_eq!(profiles[0].tags, vec!["local"]);
        db.save_known_host("localhost", 22, "ssh-ed25519", "SHA256:test")
            .expect("known host");
        assert_eq!(
            db.known_host_fingerprint("localhost", 22, "ssh-ed25519")
                .expect("known host lookup")
                .as_deref(),
            Some("SHA256:test")
        );

        let request = crate::policy::ApprovalRequest {
            approval_id: Uuid::new_v4().to_string(),
            tool_call_id: Uuid::new_v4().to_string(),
            request_hash: "hash".into(),
            risk: crate::policy::RiskAssessment {
                level: crate::policy::RiskLevel::Caution,
                score: 50,
                reasons: vec![],
                matched_rules: vec![],
            },
            summary: "restart".into(),
            target_label: "server/nginx".into(),
            impact: vec!["restart service".into()],
            proposed_change: None,
            expires_at: (Utc::now() + chrono::Duration::minutes(5)).to_rfc3339(),
            required_confirmation: crate::policy::RequiredConfirmation::Button,
        };
        db.create_approval(&crate::policy::record(&request))
            .expect("create approval");
        assert!(db
            .resolve_approval(
                &request.approval_id,
                ApprovalStatus::Pending,
                ApprovalStatus::Approved,
                "user"
            )
            .expect("approve"));
        assert!(db
            .resolve_approval(
                &request.approval_id,
                ApprovalStatus::Approved,
                ApprovalStatus::Consumed,
                "user"
            )
            .expect("consume"));
        assert!(!db
            .resolve_approval(
                &request.approval_id,
                ApprovalStatus::Approved,
                ApprovalStatus::Consumed,
                "user"
            )
            .expect("block replay"));

        let event = crate::tools::AuditEvent {
            id: Uuid::new_v4().to_string(),
            timestamp: Utc::now().to_rfc3339(),
            workspace_id: "default".into(),
            actor: "user".into(),
            server_id: Some(profile.id),
            connection_id: None,
            conversation_id: None,
            agent_run_id: None,
            action: "tool.execute".into(),
            tool_name: Some("system.memory".into()),
            tool_version: Some("1.0.0".into()),
            tool_call_id: Some(Uuid::new_v4().to_string()),
            approval_id: None,
            risk_level: Some("safe".into()),
            policy_action: Some("allow".into()),
            outcome: "success".into(),
            arguments_digest: Some("digest".into()),
            sanitized_details: serde_json::Map::new(),
        };
        db.append_audit(&event).expect("append audit");
        let events = db.list_audit(10).expect("list audit");
        assert_eq!(events[0].id, event.id);
    }
}
