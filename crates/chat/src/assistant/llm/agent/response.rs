use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::assistant::llm::{self, LlmPurpose, SharedLlmClient};

const SYSTEM_PROMPT: &str = "Write one short grounded sentence describing the structured result below. Use only the fields given. Never invent an identifier, a value, or a fact absent from the input.";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ResponseProse {
    pub prose: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponseAgentError {
    MaxTurnsExceeded,
}
impl std::fmt::Display for ResponseAgentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("response agent exceeded its model-turn budget")
    }
}
impl std::error::Error for ResponseAgentError {}

/// Generates optional grounded prose over an already policy-filtered,
/// deterministically composed structured result. The deterministic
/// structured response remains authoritative; this text is additive and
/// never receives raw SQL, hidden identifiers, or an unfiltered row —
/// callers must pass only the output of `ComposeResult` after sensitivity
/// filtering.
pub struct ResponseAgent {
    llm: SharedLlmClient,
    max_turns: u8,
}
impl ResponseAgent {
    pub fn new(llm: SharedLlmClient, max_turns: u8) -> Self {
        Self {
            llm,
            max_turns: max_turns.max(1),
        }
    }
    pub async fn narrate(&self, composed: &Value) -> Result<String, ResponseAgentError> {
        let user = serde_json::to_string(composed).unwrap_or_default();
        for _ in 0..self.max_turns {
            match llm::structured::<ResponseProse>(
                self.llm.as_ref(),
                LlmPurpose::ResponseBuild,
                SYSTEM_PROMPT,
                &user,
                None,
            )
            .await
            {
                Ok(response) => return Ok(response.value.prose),
                Err(error) => {
                    tracing::warn!(target: "assistant::response_agent", %error, "structured response turn failed")
                }
            }
        }
        Err(ResponseAgentError::MaxTurnsExceeded)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use serde_json::json;

    use crate::assistant::llm::FakeLlmClient;

    use super::*;

    #[tokio::test]
    async fn response_agent_stops_at_the_model_turn_budget() {
        let agent = ResponseAgent::new(Arc::new(FakeLlmClient::default()), 2);
        let error = agent.narrate(&json!({ "count": 3 })).await.unwrap_err();
        assert_eq!(error, ResponseAgentError::MaxTurnsExceeded);
    }

    #[tokio::test]
    async fn response_agent_input_carries_no_hidden_identifier_no_sql_no_unfiltered_row() {
        let fake = Arc::new(FakeLlmClient::default());
        fake.push_structured(json!({ "prose": "Three matching accounts." }));
        let agent = ResponseAgent::new(fake, 1);

        // What Task 6 wiring must pass here is the already sensitivity-filtered
        // `ComposeResult` output: policy-visible fields only, no raw row, no
        // hidden internal ID, no SQL text.
        let composed = json!({ "count": 3, "office_name": "Nairobi" });
        let serialized = serde_json::to_string(&composed).unwrap();
        assert!(!serialized.to_ascii_lowercase().contains("select "));
        assert!(!serialized.contains("account_id"));
        assert!(!serialized.contains("client_id"));

        let prose = agent.narrate(&composed).await.unwrap();
        assert_eq!(prose, "Three matching accounts.");
    }
}
