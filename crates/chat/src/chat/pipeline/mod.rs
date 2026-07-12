pub mod answer;
pub mod evidence;
pub mod lqr;
pub mod model;
pub mod parser;
pub mod resolver;
pub mod retrieval;
pub mod router;

use anyhow::{Result, bail};
use app_core::auth::model::ClientContext;
use serde_json::{Value, json};

use crate::chat::llm::LlmPlannerClient;
use crate::chat::pipeline::model::{RouteDecision, StrictPipelineState};

pub struct StrictPipelineInput<'a> {
    pub message: &'a str,
    pub client: &'a ClientContext,
    pub llm: &'a LlmPlannerClient,
}

pub struct StrictPipelineOutput {
    pub state: StrictPipelineState,
}

pub async fn run_strict_pipeline(input: StrictPipelineInput<'_>) -> Result<StrictPipelineOutput> {
    if !input.llm.is_enabled() {
        bail!("pipeline_config_error: LLM_API_KEY is required for strict pipeline");
    }

    let mut state = StrictPipelineState {
        conversation_context: Some(conversation_context(input.client)),
        ..StrictPipelineState::default()
    };

    let context = state
        .conversation_context
        .clone()
        .unwrap_or_else(|| json!({}));
    let parsed = input.llm.parse_intent(input.message, &context).await?;
    state.parser = Some(serde_json::to_value(&parsed)?);

    let route = router::route_intent(&parsed);
    state.route = Some(json!({ "decision": route }));
    if route != RouteDecision::Report {
        bail!("unsupported_request: strict pipeline did not route to report");
    }

    let resolved = resolver::resolve_constraints(&parsed)?;
    state.resolver = Some(serde_json::to_value(&resolved)?);

    Ok(StrictPipelineOutput { state })
}

fn conversation_context(client: &ClientContext) -> Value {
    json!({
        "api_key_id": client.api_key_id,
        "allowed_capabilities": client.allowed_capabilities,
        "allowed_office_ids": client.allowed_office_ids,
        "can_view_pii": client.can_view_pii,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn conversation_context_excludes_secrets_and_includes_scope() {
        let client = ClientContext {
            api_key_id: Uuid::nil(),
            user_id: None,
            name: "local".to_string(),
            owner: "owner".to_string(),
            key_prefix: "air_test_x".to_string(),
            allowed_office_ids: vec![1, 2],
            allowed_capabilities: vec!["savings_activity_list".to_string()],
            allow_all_offices: false,
            allow_all_capabilities: false,
            can_view_pii: true,
            expires_at: None,
        };

        let context = conversation_context(&client);

        assert_eq!(context["allowed_office_ids"], json!([1, 2]));
        assert!(context.get("key_prefix").is_none());
        assert!(context.get("raw_api_key").is_none());
    }
}
