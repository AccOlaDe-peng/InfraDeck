-- V0.1 (0001) already created a minimal ai_conversations table; extend it.
ALTER TABLE ai_conversations ADD COLUMN server_id TEXT;
ALTER TABLE ai_conversations ADD COLUMN message_count INTEGER NOT NULL DEFAULT 0;
ALTER TABLE ai_conversations ADD COLUMN status TEXT NOT NULL DEFAULT 'active';

CREATE TABLE IF NOT EXISTS ai_messages (
  id TEXT PRIMARY KEY,
  conversation_id TEXT NOT NULL REFERENCES ai_conversations(id) ON DELETE CASCADE,
  seq INTEGER NOT NULL,
  role TEXT NOT NULL CHECK (role IN ('user','assistant','tool','system')),
  content TEXT,
  tool_call_id TEXT,
  tool_calls_json TEXT,
  agent_run_id TEXT,
  created_at TEXT NOT NULL,
  UNIQUE (conversation_id, seq)
);
CREATE INDEX IF NOT EXISTS idx_ai_messages_conversation ON ai_messages(conversation_id, seq);
