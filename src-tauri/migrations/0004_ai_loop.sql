CREATE TABLE IF NOT EXISTS ai_provider_settings (
  id INTEGER PRIMARY KEY CHECK (id = 1),
  provider_kind TEXT NOT NULL DEFAULT 'openaiCompatible',
  base_url TEXT NOT NULL DEFAULT 'https://api.openai.com/v1',
  model TEXT NOT NULL DEFAULT 'gpt-4o-mini',
  api_key_credential_id TEXT,
  max_tool_iterations INTEGER NOT NULL DEFAULT 8,
  max_tool_output_chars INTEGER NOT NULL DEFAULT 8000,
  updated_at TEXT NOT NULL
);

INSERT OR IGNORE INTO ai_provider_settings
  (id, provider_kind, base_url, model, api_key_credential_id, max_tool_iterations, max_tool_output_chars, updated_at)
VALUES (1, 'openaiCompatible', 'https://api.openai.com/v1', 'gpt-4o-mini', NULL, 8, 8000, CURRENT_TIMESTAMP);
