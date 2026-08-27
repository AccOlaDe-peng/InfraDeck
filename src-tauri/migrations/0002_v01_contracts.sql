ALTER TABLE servers ADD COLUMN connect_timeout_ms INTEGER NOT NULL DEFAULT 15000;
ALTER TABLE servers ADD COLUMN keep_alive_interval_sec INTEGER NOT NULL DEFAULT 30;

CREATE TABLE IF NOT EXISTS known_hosts (
  host TEXT NOT NULL,
  port INTEGER NOT NULL,
  key_type TEXT NOT NULL,
  fingerprint TEXT NOT NULL,
  first_seen_at TEXT NOT NULL,
  last_seen_at TEXT NOT NULL,
  PRIMARY KEY(host, port, key_type)
);

CREATE TABLE IF NOT EXISTS app_settings (
  id INTEGER PRIMARY KEY CHECK (id = 1),
  permission_mode TEXT NOT NULL DEFAULT 'confirmChanges',
  telemetry_enabled INTEGER NOT NULL DEFAULT 0,
  conversation_persistence_enabled INTEGER NOT NULL DEFAULT 1,
  updated_at TEXT NOT NULL
);

INSERT OR IGNORE INTO app_settings
  (id, permission_mode, telemetry_enabled, conversation_persistence_enabled, updated_at)
VALUES (1, 'confirmChanges', 0, 1, CURRENT_TIMESTAMP);
