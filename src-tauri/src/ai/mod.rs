use async_trait::async_trait;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;
use tokio_util::sync::CancellationToken;

use crate::error::AppError;

pub mod conversation;

pub const DEFAULT_BASE_URL: &str = "https://api.openai.com/v1";
pub const DEFAULT_MODEL: &str = "gpt-4o-mini";
pub const MAX_TOOL_ITERATIONS_LIMIT: u32 = 20;
pub const MIN_TOOL_ITERATIONS: u32 = 1;
pub const MIN_TOOL_OUTPUT_CHARS: u32 = 500;
pub const MAX_TOOL_OUTPUT_CHARS_LIMIT: u32 = 50_000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AiProviderSettings {
    pub provider_kind: String,
    pub base_url: String,
    pub model: String,
    pub api_key_credential_id: Option<String>,
    pub max_tool_iterations: u32,
    pub max_tool_output_chars: u32,
    pub updated_at: String,
}

impl Default for AiProviderSettings {
    fn default() -> Self {
        Self {
            provider_kind: "openaiCompatible".into(),
            base_url: DEFAULT_BASE_URL.into(),
            model: DEFAULT_MODEL.into(),
            api_key_credential_id: None,
            max_tool_iterations: 8,
            max_tool_output_chars: 8000,
            updated_at: Utc::now().to_rfc3339(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiProviderSettingsInput {
    pub provider_kind: Option<String>,
    pub base_url: String,
    pub model: String,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub api_key_credential_id: Option<String>,
    pub max_tool_iterations: Option<u32>,
    pub max_tool_output_chars: Option<u32>,
}

pub fn validate_settings_input(input: &AiProviderSettingsInput) -> Result<(), AppError> {
    let base_url = input.base_url.trim();
    if !(base_url.starts_with("http://") || base_url.starts_with("https://")) {
        return Err(AppError::Validation(
            "AI Provider Base URL 必须以 http:// 或 https:// 开头".into(),
        ));
    }
    if input.model.trim().is_empty() {
        return Err(AppError::Validation("AI 模型名不能为空".into()));
    }
    if let Some(kind) = &input.provider_kind {
        if kind != "openaiCompatible" {
            return Err(AppError::Validation(
                "暂只支持 openaiCompatible Provider".into(),
            ));
        }
    }
    if let Some(iterations) = input.max_tool_iterations {
        if !(MIN_TOOL_ITERATIONS..=MAX_TOOL_ITERATIONS_LIMIT).contains(&iterations) {
            return Err(AppError::Validation(format!(
                "最大工具迭代次数必须在 {MIN_TOOL_ITERATIONS}-{MAX_TOOL_ITERATIONS_LIMIT} 之间"
            )));
        }
    }
    if let Some(chars) = input.max_tool_output_chars {
        if !(MIN_TOOL_OUTPUT_CHARS..=MAX_TOOL_OUTPUT_CHARS_LIMIT).contains(&chars) {
            return Err(AppError::Validation(format!(
                "工具输出字符上限必须在 {MIN_TOOL_OUTPUT_CHARS}-{MAX_TOOL_OUTPUT_CHARS_LIMIT} 之间"
            )));
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<RequestedToolCallSpec>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

impl ChatMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: "system".into(),
            content: Some(content.into()),
            tool_calls: None,
            tool_call_id: None,
        }
    }
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".into(),
            content: Some(content.into()),
            tool_calls: None,
            tool_call_id: None,
        }
    }
    pub fn tool_result(tool_call_id: &str, content: impl Into<String>) -> Self {
        Self {
            role: "tool".into(),
            content: Some(content.into()),
            tool_calls: None,
            tool_call_id: Some(tool_call_id.into()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestedToolCallSpec {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub tools: Vec<ToolSpec>,
}

#[derive(Debug, Clone, Default)]
pub struct ChatResponse {
    pub content: Option<String>,
    pub tool_calls: Vec<RequestedToolCallSpec>,
    #[allow(dead_code)]
    pub prompt_tokens: u64,
    #[allow(dead_code)]
    pub completion_tokens: u64,
}

#[derive(Debug, Error)]
pub enum LlmError {
    #[error("AI Provider 请求失败: {0}")]
    Transport(String),
    #[error("AI Provider 响应无法解析: {0}")]
    MalformedResponse(String),
}

impl From<LlmError> for AppError {
    fn from(error: LlmError) -> Self {
        let retryable = matches!(error, LlmError::Transport(_));
        AppError::Ai(format!("{error}; retryable={retryable}"))
    }
}

/// LLM Provider abstraction. V0.1 ships an OpenAI-compatible implementation;
/// future providers (Anthropic, Ollama, …) implement the same trait.
#[derive(Debug, Clone)]
pub enum StreamFinishReason {
    Completed,
    Cancelled,
    Error(String),
}

/// Receives incremental model output as it arrives. Implementations must be
/// cheap — called once per SSE chunk on the streaming path.
pub trait StreamSink: Send + Sync {
    fn delta(&self, text: &str);
    fn finished(&self, reason: StreamFinishReason);
}

#[async_trait]
pub trait LlmProvider: Send + Sync {
    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse, LlmError>;
    /// Streaming chat. Default implementation falls back to `chat` and emits
    /// the full text as one delta, so test doubles need not implement SSE.
    async fn stream(
        &self,
        request: ChatRequest,
        sink: std::sync::Arc<dyn StreamSink>,
        cancel: tokio_util::sync::CancellationToken,
    ) -> Result<ChatResponse, LlmError> {
        let _ = &cancel;
        let response = self.chat(request).await?;
        if let Some(text) = &response.content {
            sink.delta(text);
        }
        sink.finished(StreamFinishReason::Completed);
        Ok(response)
    }
    #[allow(dead_code)]
    fn capabilities(&self) -> LlmCapabilities {
        LlmCapabilities {
            tool_calling: true,
            streaming: true,
        }
    }
}

#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub struct LlmCapabilities {
    pub tool_calling: bool,
    pub streaming: bool,
}

/// Aggregates OpenAI streaming deltas: content fragments and tool_calls that
/// arrive split across chunks (id/name first, arguments piecewise, by index).
#[derive(Default)]
pub(crate) struct StreamAggregator {
    content: String,
    tool_calls: Vec<RequestedToolCallSpec>,
}

impl StreamAggregator {
    /// Absorbs one `data: {json}` chunk; returns extracted text delta, if any.
    pub(crate) fn absorb(&mut self, payload: &Value) -> Option<String> {
        let delta = &payload["choices"][0]["delta"];
        if let Some(text) = delta["content"].as_str() {
            self.content.push_str(text);
            return Some(text.to_string());
        }
        if let Some(calls) = delta["tool_calls"].as_array() {
            for call in calls {
                let index = call["index"].as_u64().unwrap_or(0) as usize;
                while self.tool_calls.len() <= index {
                    self.tool_calls.push(RequestedToolCallSpec {
                        id: String::new(),
                        name: String::new(),
                        arguments: String::new(),
                    });
                }
                let slot = &mut self.tool_calls[index];
                if let Some(id) = call["id"].as_str() {
                    slot.id.push_str(id);
                }
                if let Some(name) = call["function"]["name"].as_str() {
                    slot.name.push_str(name);
                }
                if let Some(arguments) = call["function"]["arguments"].as_str() {
                    slot.arguments.push_str(arguments);
                }
            }
        }
        None
    }

    pub(crate) fn finish(self) -> ChatResponse {
        ChatResponse {
            content: if self.content.is_empty() {
                None
            } else {
                Some(self.content)
            },
            tool_calls: self.tool_calls,
            ..Default::default()
        }
    }
}

pub struct OpenAiCompatibleProvider {
    pub base_url: String,
    pub model: String,
    pub api_key: String,
    pub http: reqwest::Client,
}

impl OpenAiCompatibleProvider {
    pub fn new(base_url: String, model: String, api_key: String) -> Result<Self, LlmError> {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .map_err(|error| LlmError::Transport(error.to_string()))?;
        Ok(Self {
            base_url: base_url.trim_end_matches('/').into(),
            model,
            api_key,
            http,
        })
    }
}

#[async_trait]
impl LlmProvider for OpenAiCompatibleProvider {
    async fn stream(
        &self,
        request: ChatRequest,
        sink: std::sync::Arc<dyn StreamSink>,
        cancel: tokio_util::sync::CancellationToken,
    ) -> Result<ChatResponse, LlmError> {
        use futures_util::StreamExt;
        let model = if request.model.is_empty() {
            self.model.clone()
        } else {
            request.model.clone()
        };
        let body = serde_json::json!({
            "model": model,
            "messages": request.messages,
            "tools": request.tools.iter().map(|tool| serde_json::json!({
                "type": "function",
                "function": {"name": tool.name, "description": tool.description, "parameters": tool.parameters}
            })).collect::<Vec<_>>(),
            "stream": true,
        });
        let url = format!("{}/chat/completions", self.base_url);
        let response = tokio::select! {
            _ = cancel.cancelled() => {
                sink.finished(StreamFinishReason::Cancelled);
                return Err(LlmError::Transport("cancelled".into()));
            }
            response = self.http.post(&url).bearer_auth(&self.api_key).json(&body).send() => {
                response.map_err(|error| LlmError::Transport(error.to_string()))?
            }
        };
        let status = response.status();
        if !status.is_success() {
            let error = LlmError::Transport(format!("HTTP {status}: provider returned an error"));
            sink.finished(StreamFinishReason::Error(error.to_string()));
            return Err(error);
        }
        let mut byte_stream = response.bytes_stream();
        let mut buffer = String::new();
        let mut aggregator = StreamAggregator::default();
        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    sink.finished(StreamFinishReason::Cancelled);
                    return Err(LlmError::Transport("cancelled".into()));
                }
                chunk = byte_stream.next() => match chunk {
                    None => break,
                    Some(Err(error)) => {
                        let error = LlmError::Transport(error.to_string());
                        sink.finished(StreamFinishReason::Error(error.to_string()));
                        return Err(error);
                    }
                    Some(Ok(bytes)) => {
                        buffer.push_str(&String::from_utf8_lossy(&bytes));
                        // SSE events are newline-delimited; process complete lines.
                        while let Some(position) = buffer.find('\n') {
                            let line: String = buffer.drain(..position + 1).collect();
                            let line = line.trim();
                            let Some(payload) = line.strip_prefix("data: ") else { continue };
                            let payload = payload.trim();
                            if payload == "[DONE]" {
                                continue;
                            }
                            match serde_json::from_str::<Value>(payload) {
                                Ok(value) => {
                                    if let Some(text) = aggregator.absorb(&value) {
                                        sink.delta(&text);
                                    }
                                }
                                Err(_) => continue, // keep-alive comments and partial lines
                            }
                        }
                    }
                }
            }
        }
        sink.finished(StreamFinishReason::Completed);
        Ok(aggregator.finish())
    }

    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse, LlmError> {
        let model = if request.model.is_empty() {
            self.model.clone()
        } else {
            request.model.clone()
        };
        let body = serde_json::json!({
            "model": model,
            "messages": request.messages,
            "tools": request.tools.iter().map(|tool| serde_json::json!({
                "type": "function",
                "function": {"name": tool.name, "description": tool.description, "parameters": tool.parameters}
            })).collect::<Vec<_>>(),
        });
        let url = format!("{}/chat/completions", self.base_url);
        let response = self
            .http
            .post(&url)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|error| LlmError::Transport(error.to_string()))?;
        let status = response.status();
        let payload: Value = response
            .json()
            .await
            .map_err(|error| LlmError::MalformedResponse(error.to_string()))?;
        if !status.is_success() {
            let message = payload["error"]["message"]
                .as_str()
                .unwrap_or("provider returned an error")
                .to_string();
            return Err(LlmError::Transport(format!("HTTP {status}: {message}")));
        }
        let choice = payload["choices"]
            .get(0)
            .ok_or_else(|| LlmError::MalformedResponse("missing choices[0]".into()))?;
        let message = &choice["message"];
        let tool_calls = message["tool_calls"]
            .as_array()
            .unwrap_or(&Vec::new())
            .iter()
            .filter_map(|call| {
                let function = &call["function"];
                Some(RequestedToolCallSpec {
                    id: call["id"].as_str()?.to_string(),
                    name: function["name"].as_str()?.to_string(),
                    arguments: function["arguments"].as_str().unwrap_or("{}").to_string(),
                })
            })
            .collect();
        Ok(ChatResponse {
            content: message["content"].as_str().map(str::to_owned),
            tool_calls,
            prompt_tokens: payload["usage"]["prompt_tokens"].as_u64().unwrap_or(0),
            completion_tokens: payload["usage"]["completion_tokens"].as_u64().unwrap_or(0),
        })
    }
}

/// Mutable state of one agent run, kept in memory while the run is active or
/// paused waiting for an approval. `token` allows cancelling between steps.
#[derive(Clone)]
pub struct AgentRunState {
    pub run_id: String,
    pub conversation_id: String,
    pub server_id: String,
    pub title: String,
    pub messages: Vec<ChatMessage>,
    pub steps: Vec<AgentToolStep>,
    pub pending_tool_call_id: Option<String>,
    pub iterations: u32,
    /// Messages below this index are already written to ai_messages.
    pub persisted_seq: u32,
    pub token: CancellationToken,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentToolStep {
    pub tool_call_id: String,
    pub name: String,
    pub input: Value,
    pub status: String,
    pub summary: Option<String>,
}

/// Strip ANSI escape sequences; remote output is untrusted data, never instructions.
pub fn strip_ansi(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars().peekable();
    while let Some(current) = chars.next() {
        if current == '\x1b' {
            if chars.peek() == Some(&'[') {
                chars.next();
                for next in chars.by_ref() {
                    if next.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
            continue;
        }
        out.push(current);
    }
    out
}

/// Tool output hygiene before feeding results back to the model:
/// ANSI stripping, secret redaction and a hard size cap.
pub fn sanitize_tool_output(value: &str, max_chars: u32) -> String {
    let cleaned = strip_ansi(value);
    let redacted = crate::tools::redact(&cleaned);
    if redacted.chars().count() > max_chars as usize {
        let mut truncated: String = redacted.chars().take(max_chars as usize).collect();
        truncated.push_str("\n…[输出已截断]");
        truncated
    } else {
        redacted
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_validation_rejects_bad_input() {
        let valid = AiProviderSettingsInput {
            provider_kind: None,
            base_url: "https://api.example.com/v1".into(),
            model: "gpt-x".into(),
            api_key: None,
            api_key_credential_id: None,
            max_tool_iterations: Some(8),
            max_tool_output_chars: Some(8000),
        };
        assert!(validate_settings_input(&valid).is_ok());
        let mut bad = clone_input(&valid);
        bad.base_url = "ftp://api.example.com".into();
        assert!(validate_settings_input(&bad).is_err());
        let mut bad = clone_input(&valid);
        bad.model = "  ".into();
        assert!(validate_settings_input(&bad).is_err());
        let mut bad = clone_input(&valid);
        bad.max_tool_iterations = Some(99);
        assert!(validate_settings_input(&bad).is_err());
        let mut bad = clone_input(&valid);
        bad.max_tool_output_chars = Some(10);
        assert!(validate_settings_input(&bad).is_err());
        let mut bad = clone_input(&valid);
        bad.provider_kind = Some("anthropic".into());
        assert!(validate_settings_input(&bad).is_err());
    }

    fn clone_input(input: &AiProviderSettingsInput) -> AiProviderSettingsInput {
        AiProviderSettingsInput {
            provider_kind: input.provider_kind.clone(),
            base_url: input.base_url.clone(),
            model: input.model.clone(),
            api_key: input.api_key.clone(),
            api_key_credential_id: input.api_key_credential_id.clone(),
            max_tool_iterations: input.max_tool_iterations,
            max_tool_output_chars: input.max_tool_output_chars,
        }
    }

    #[test]
    fn strips_ansi_and_redacts_secrets() {
        assert_eq!(strip_ansi("\x1b[31mred\x1b[0m"), "red");
        assert_eq!(strip_ansi("plain"), "plain");
        let sanitized = sanitize_tool_output("\x1b[1mtoken=abcdef\x1b[0m", 1000);
        assert_eq!(sanitized, "[REDACTED]");
    }

    #[test]
    fn sanitize_truncates_long_output() {
        let sanitized = sanitize_tool_output(&"x".repeat(3000), 100);
        assert!(sanitized.contains("输出已截断"));
        assert!(sanitized.chars().count() < 200);
    }

    #[test]
    fn stream_aggregator_reassembles_split_deltas() {
        use super::StreamAggregator;
        let mut aggregator = StreamAggregator::default();
        let chunks = [
            serde_json::json!({"choices":[{"delta":{"content":"内存"}}]}),
            serde_json::json!({"choices":[{"delta":{"content":"使用率 90%"}}]}),
        ];
        let mut text = String::new();
        for chunk in &chunks {
            text.push_str(&aggregator.absorb(chunk).expect("text delta"));
        }
        assert_eq!(text, "内存使用率 90%");
        // Tool call split across three chunks: id+name first, arguments piecewise.
        let calls = [
            serde_json::json!({"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_9","function":{"name":"service.restart","arguments":""}}]}}]}),
            serde_json::json!({"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{\"serv"}}]}}]}),
            serde_json::json!({"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"ice\":\"nginx\"}"}}]}}]}),
        ];
        for call in &calls {
            assert!(
                aggregator.absorb(call).is_none(),
                "tool chunks carry no text delta"
            );
        }
        let response = aggregator.finish();
        assert_eq!(response.content.as_deref(), Some("内存使用率 90%"));
        assert_eq!(response.tool_calls.len(), 1);
        assert_eq!(response.tool_calls[0].id, "call_9");
        assert_eq!(response.tool_calls[0].name, "service.restart");
        assert_eq!(response.tool_calls[0].arguments, r#"{"service":"nginx"}"#);
    }

    #[test]
    fn stream_aggregator_handles_two_parallel_tool_calls() {
        use super::StreamAggregator;
        let mut aggregator = StreamAggregator::default();
        aggregator.absorb(&serde_json::json!({"choices":[{"delta":{"tool_calls":[{"index":0,"id":"a","function":{"name":"tool.a","arguments":"{}"}}]}}]}));
        aggregator.absorb(&serde_json::json!({"choices":[{"delta":{"tool_calls":[{"index":1,"id":"b","function":{"name":"tool.b","arguments":"{}"}}]}}]}));
        let response = aggregator.finish();
        assert_eq!(response.tool_calls.len(), 2);
        assert_eq!(response.tool_calls[1].name, "tool.b");
    }

    #[test]
    fn openai_response_parses_tool_calls() {
        let payload: Value = serde_json::from_str(
            r#"{"choices":[{"message":{"content":null,"tool_calls":[{"id":"call_1","type":"function","function":{"name":"system.memory","arguments":"{}"}}]}}],"usage":{"prompt_tokens":10,"completion_tokens":5}}"#,
        )
        .unwrap();
        let calls = payload["choices"][0]["message"]["tool_calls"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|call| {
                Some(RequestedToolCallSpec {
                    id: call["id"].as_str()?.to_string(),
                    name: call["function"]["name"].as_str()?.to_string(),
                    arguments: call["function"]["arguments"]
                        .as_str()
                        .unwrap_or("{}")
                        .to_string(),
                })
            })
            .collect::<Vec<_>>();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "system.memory");
        assert_eq!(payload["usage"]["prompt_tokens"].as_u64(), Some(10));
    }
}
