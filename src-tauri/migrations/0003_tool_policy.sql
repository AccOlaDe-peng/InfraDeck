ALTER TABLE audit_events RENAME TO audit_events_legacy;

CREATE TABLE audit_events (
  id TEXT PRIMARY KEY NOT NULL,
  timestamp TEXT NOT NULL,
  workspace_id TEXT NOT NULL,
  actor TEXT NOT NULL CHECK(actor IN ('user','ai','system')),
  server_id TEXT,
  connection_id TEXT,
  conversation_id TEXT,
  agent_run_id TEXT,
  action TEXT NOT NULL,
  tool_name TEXT,
  tool_version TEXT,
  tool_call_id TEXT,
  approval_id TEXT,
  risk_level TEXT,
  policy_action TEXT,
  outcome TEXT NOT NULL CHECK(outcome IN ('success','failed','denied','cancelled','partial')),
  arguments_digest TEXT,
  details_json TEXT NOT NULL DEFAULT '{}'
);

INSERT INTO audit_events
  (id,timestamp,workspace_id,actor,server_id,action,tool_call_id,approval_id,outcome,details_json)
SELECT id,timestamp,'default',actor,server_id,action,tool_call_id,approval_id,outcome,details_json
FROM audit_events_legacy;

DROP TABLE audit_events_legacy;
CREATE INDEX idx_audit_timestamp ON audit_events(timestamp DESC);
CREATE INDEX idx_audit_tool_call ON audit_events(tool_call_id);
CREATE INDEX idx_audit_agent_run ON audit_events(agent_run_id);

CREATE TABLE approvals (
  id TEXT PRIMARY KEY NOT NULL,
  tool_call_id TEXT NOT NULL,
  request_hash TEXT NOT NULL,
  risk_level TEXT NOT NULL,
  summary TEXT NOT NULL,
  impact_json TEXT NOT NULL,
  status TEXT NOT NULL CHECK(status IN ('pending','approved','rejected','expired','consumed')),
  approved_by TEXT,
  created_at TEXT NOT NULL,
  expires_at TEXT NOT NULL,
  resolved_at TEXT,
  consumed_at TEXT
);

CREATE UNIQUE INDEX idx_approval_tool_call ON approvals(tool_call_id);
