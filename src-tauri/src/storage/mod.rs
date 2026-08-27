use std::{fs, path::{Path, PathBuf}};
use chrono::Utc;
use rusqlite::{params, Connection};
use tracing::{info, instrument};

use crate::{error::AppError, models::{AuthRef, Environment, ServerProfile}};

const MIGRATION_VERSION: i64 = 1;

pub struct Database { conn: Connection, path: PathBuf }

impl Database {
    pub fn open_default() -> Result<Self, AppError> {
        let root = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
        let dir = root.join("InfraDeck");
        fs::create_dir_all(&dir).map_err(|e| AppError::Internal(format!("create data directory: {e}")))?;
        Self::open(dir.join("infradeck.sqlite3"))
    }

    pub fn open(path: impl AsRef<Path>) -> Result<Self, AppError> {
        let path = path.as_ref().to_path_buf();
        let conn = Connection::open(&path)?;
        conn.execute_batch("PRAGMA foreign_keys = ON; PRAGMA busy_timeout = 5000;")?;
        let db = Self { conn, path };
        db.migrate()?;
        info!(target: "infradeck::storage", path = %db.path.display(), "database ready");
        Ok(db)
    }

    fn migrate(&self) -> Result<(), AppError> {
        self.conn.execute_batch("CREATE TABLE IF NOT EXISTS schema_migrations (version INTEGER PRIMARY KEY, applied_at TEXT NOT NULL);")?;
        let applied: Option<i64> = self.conn.query_row("SELECT MAX(version) FROM schema_migrations", [], |row| row.get(0))?;
        if applied.unwrap_or(0) < MIGRATION_VERSION {
            self.conn.execute_batch(include_str!("../../migrations/0001_initial.sql"))?;
            self.conn.execute("INSERT OR IGNORE INTO schema_migrations(version, applied_at) VALUES (?1, ?2)", params![MIGRATION_VERSION, Utc::now().to_rfc3339()])?;
        }
        Ok(())
    }

    #[instrument(skip(self, profile), target = "infradeck::storage")]
    pub fn upsert_server_profile(&self, profile: &ServerProfile) -> Result<(), AppError> {
        let now = Utc::now().to_rfc3339();
        let (auth_kind, credential_ref, key_path) = match &profile.auth {
            AuthRef::Password { credential_id } => ("password", Some(credential_id.as_str()), None),
            AuthRef::PrivateKey { key_path, passphrase_credential_id } => ("privateKey", passphrase_credential_id.as_deref(), Some(key_path.as_str())),
            AuthRef::Agent => ("agent", None, None),
        };
        self.conn.execute("INSERT INTO servers(id,name,host,port,username,auth_kind,credential_ref,key_path,environment,tags_json,created_at,updated_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?11) ON CONFLICT(id) DO UPDATE SET name=excluded.name,host=excluded.host,port=excluded.port,username=excluded.username,auth_kind=excluded.auth_kind,credential_ref=excluded.credential_ref,key_path=excluded.key_path,environment=excluded.environment,tags_json=excluded.tags_json,updated_at=excluded.updated_at", params![profile.id, profile.name, profile.host, profile.port, profile.username, auth_kind, credential_ref, key_path, profile.environment.as_str(), serde_json::to_string(&profile.tags).map_err(|e| AppError::Internal(e.to_string()))?, now])?;
        Ok(())
    }

    pub fn list_server_profiles(&self) -> Result<Vec<ServerProfile>, AppError> {
        let mut stmt = self.conn.prepare("SELECT id,name,host,port,username,auth_kind,credential_ref,key_path,environment,tags_json FROM servers ORDER BY updated_at DESC")?;
        let rows = stmt.query_map([], |row| {
            let auth_kind: String = row.get(5)?;
            let credential_ref: Option<String> = row.get(6)?;
            let key_path: Option<String> = row.get(7)?;
            let auth = match auth_kind.as_str() {
                "password" => AuthRef::Password { credential_id: credential_ref.unwrap_or_default() },
                "privateKey" => AuthRef::PrivateKey { key_path: key_path.unwrap_or_default(), passphrase_credential_id: credential_ref },
                _ => AuthRef::Agent,
            };
            let environment = match row.get::<_, String>(8)?.as_str() { "dev" => Environment::Dev, "staging" => Environment::Staging, "production" => Environment::Production, _ => Environment::Unknown };
            let tags_json: String = row.get(9)?;
            Ok(ServerProfile { id: row.get(0)?, name: row.get(1)?, host: row.get(2)?, port: row.get(3)?, username: row.get(4)?, auth, environment, tags: serde_json::from_str(&tags_json).unwrap_or_default() })
        })?;
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
        let profile = ServerProfile { id: Uuid::new_v4().to_string(), name: "test".into(), host: "localhost".into(), port: 22, username: "dev".into(), auth: AuthRef::Agent, environment: Environment::Dev, tags: vec!["local".into()] };
        db.upsert_server_profile(&profile).expect("insert");
        let profiles = db.list_server_profiles().expect("list");
        assert_eq!(profiles[0].host, "localhost");
        assert_eq!(profiles[0].tags, vec!["local"]);
    }
}
