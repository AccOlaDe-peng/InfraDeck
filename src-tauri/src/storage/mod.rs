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
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;
    #[test]
    fn migrates_and_round_trips_server_profile() {
        let db = Database::open(":memory:").expect("database");
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
    }
}
