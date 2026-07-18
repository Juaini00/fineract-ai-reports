pub mod answer;
pub mod evidence;
pub mod lqr;
pub mod model;
pub mod parser;
pub mod resolver;
pub mod retrieval;
pub mod router;

use anyhow::{Result, bail};
use app_core::auth::model::PrincipalContext;
use serde_json::{Value, json};

use crate::assistant::legacy_pipeline::model::{RouteDecision, StrictPipelineState};
use crate::assistant::llm::planner_client::LlmPlannerClient;

pub struct StrictPipelineInput<'a> {
    pub message: &'a str,
    pub client: &'a PrincipalContext,
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

fn conversation_context(client: &PrincipalContext) -> Value {
    json!({
        "user_id": client.user_id,
        "capabilities": client.capability_ids,
        "office_ids": client.office_ids,
        "can_view_pii": client.can_view_pii,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn conversation_context_excludes_secrets_and_includes_scope() {
        let client = PrincipalContext {
            user_id: Uuid::nil(),
            role: "admin".to_string(),
            office_ids: vec![1, 2],
            capability_ids: vec!["savings_activity_list".to_string()],
            can_view_pii: true,
            legacy_api_key_id: None,
        };

        let context = conversation_context(&client);

        assert_eq!(context["office_ids"], json!([1, 2]));
        assert!(context.get("key_prefix").is_none());
        assert!(context.get("raw_api_key").is_none());
    }
}
