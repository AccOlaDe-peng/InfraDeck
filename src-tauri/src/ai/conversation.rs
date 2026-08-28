use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Conversation metadata surfaced to the conversation picker.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiConversationDto {
    pub id: String,
    pub title: String,
    pub server_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub message_count: u32,
    pub status: String,
}

/// One persisted turn. `content` for role=tool is always the sanitized JSON
/// produced by `tool_message_content` — raw remote output is never stored.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiMessageDto {
    pub id: String,
    pub conversation_id: String,
    pub seq: u32,
    pub role: String,
    pub content: Option<String>,
    pub tool_call_id: Option<String>,
    pub tool_calls: Option<Vec<super::RequestedToolCallSpec>>,
    pub agent_run_id: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationListQuery {
    pub server_id: Option<String>,
    pub query: Option<String>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

impl ConversationListQuery {
    pub fn normalized(mut self) -> Self {
        self.limit = Some(self.limit.unwrap_or(30).clamp(1, 100));
        self.offset = Some(self.offset.unwrap_or(0));
        self.query = self
            .query
            .map(|value| value.trim().to_lowercase())
            .filter(|value| !value.is_empty());
        self
    }
}

/// Turns a chat message into its persisted form; ids and timestamps are minted here.
pub fn message_dto(
    conversation_id: &str,
    seq: u32,
    message: &super::ChatMessage,
    agent_run_id: Option<&str>,
) -> AiMessageDto {
    AiMessageDto {
        id: Uuid::new_v4().to_string(),
        conversation_id: conversation_id.to_string(),
        seq,
        role: message.role.clone(),
        content: message.content.clone(),
        tool_call_id: message.tool_call_id.clone(),
        tool_calls: message.tool_calls.clone(),
        agent_run_id: agent_run_id.map(str::to_owned),
        created_at: Utc::now().to_rfc3339(),
    }
}

pub fn conversation_title(first_user_message: &str) -> String {
    let title: String = first_user_message.chars().take(40).collect();
    if title.is_empty() {
        "新会话".into()
    } else {
        title
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_normalization_clamps_and_trims() {
        let query = ConversationListQuery {
            server_id: Some("srv".into()),
            query: Some("  nginx  ".into()),
            limit: Some(9999),
            offset: None,
        }
        .normalized();
        assert_eq!(query.limit, Some(100));
        assert_eq!(query.offset, Some(0));
        assert_eq!(query.query.as_deref(), Some("nginx"));

        let query = ConversationListQuery {
            server_id: None,
            query: Some("   ".into()),
            limit: None,
            offset: Some(5),
        }
        .normalized();
        assert_eq!(query.limit, Some(30));
        assert_eq!(query.query, None);
    }

    #[test]
    fn wire_contract_matches_fixture() {
        let dto: AiMessageDto =
            serde_json::from_str(include_str!("../../../tests/contracts/ai_message.json"))
                .expect("fixture");
        assert_eq!(dto.seq, 3);
        assert_eq!(dto.role, "tool");
        assert_eq!(dto.tool_call_id.as_deref(), Some("call_0"));
        assert!(dto.tool_calls.is_none());
        let wire = serde_json::to_value(&dto).expect("serialize");
        assert_eq!(
            wire["conversationId"],
            "1a1a1b2c-3d4e-4f50-8611-223344556677"
        );
        assert_eq!(wire["agentRunId"], "2b2b1b2c-3d4e-4f50-8611-223344556677");
        assert_eq!(wire["createdAt"], "2026-08-28T04:00:00+00:00");
    }

    #[test]
    fn title_takes_first_40_chars() {
        assert_eq!(conversation_title("short"), "short");
        assert_eq!(conversation_title("").len(), 9);
        let long = conversation_title(&"x".repeat(100));
        assert_eq!(long.chars().count(), 40);
    }
}
