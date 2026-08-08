use crate::assistant::llm::tool::{MetadataTool, registry};
use crate::assistant::{
    llm::{self, LlmPurpose, SharedLlmClient},
    workflow::WorkflowProposal,
};
use crate::knowledge::model::KnowledgeCatalog;

const SYSTEM_PROMPT: &str = "Return one JSON WorkflowProposal. It may reference only approved catalog IDs and typed bindings. Never include SQL, policies, or data access instructions.";

pub struct PlanningAgent {
    llm: SharedLlmClient,
    max_turns: u8,
    dynamic_tools: bool,
    dynamic_context: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanningAgentError {
    MaxTurnsExceeded,
    InvalidProposal,
}
impl std::fmt::Display for PlanningAgentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::MaxTurnsExceeded => "planning agent exceeded its model-turn budget",
            Self::InvalidProposal => "planning agent returned an invalid workflow proposal",
        })
    }
}
impl std::error::Error for PlanningAgentError {}
impl PlanningAgent {
    pub fn new(llm: SharedLlmClient, max_turns: u8) -> Self {
        Self {
            llm,
            max_turns: max_turns.max(1),
            dynamic_tools: false,
            dynamic_context: false,
        }
    }
    pub fn max_turns(&self) -> u8 {
        self.max_turns
    }
    pub fn dynamic_tools(&self) -> bool {
        self.dynamic_tools
    }
    pub fn dynamic_context(&self) -> bool {
        self.dynamic_context
    }
    pub fn tools(&self, catalog: &KnowledgeCatalog) -> Vec<MetadataTool> {
        registry(catalog)
    }
    pub async fn propose(
        &self,
        request: &str,
        catalog: &KnowledgeCatalog,
    ) -> Result<WorkflowProposal, PlanningAgentError> {
        let context = format!(
            "request: {request}\nmetadata tools: {}",
            self.tools(catalog)
                .into_iter()
                .map(|tool| tool.description)
                .collect::<Vec<_>>()
                .join("\n")
        );
        for _ in 0..self.max_turns {
            match llm::structured::<WorkflowProposal>(
                self.llm.as_ref(),
                LlmPurpose::WorkflowPlanning,
                SYSTEM_PROMPT,
                &context,
                None,
            )
            .await
            {
                Ok(response) if proposal_is_safe(&response.value) => return Ok(response.value),
                Ok(_) => return Err(PlanningAgentError::InvalidProposal),
                Err(error) => {
                    tracing::warn!(target: "assistant::planning_agent", %error, "structured planning turn failed")
                }
            }
        }
        Err(PlanningAgentError::MaxTurnsExceeded)
    }
}
fn proposal_is_safe(proposal: &WorkflowProposal) -> bool {
    !proposal
        .capability_ids
        .iter()
        .any(|value| suspicious(value))
        && !serde_json::to_string(proposal).map_or(true, |value| suspicious(&value))
}
fn suspicious(value: &str) -> bool {
    let value = value.to_ascii_lowercase();
    value.contains("select ")
        || value.contains("insert ")
        || value.contains("update ")
        || value.contains("delete ")
        || value.contains(";--")
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use app_core::auth::model::PrincipalContext;
    use uuid::Uuid;

    use crate::{
        assistant::{
            llm::FakeLlmClient,
            workflow::{WorkflowBudgets, compile, verify},
        },
        knowledge::catalog::loader::KnowledgeLoader,
    };

    use super::*;

    fn budgets() -> WorkflowBudgets {
        WorkflowBudgets {
            shared_timeout_ms: 30_000,
            shared_row_cap: 1_000,
            max_query_count: 10,
            max_parallel_queries: 2,
            max_model_turns: 2,
            max_node_retries: 0,
        }
    }

    fn principal(catalog: &KnowledgeCatalog) -> PrincipalContext {
        PrincipalContext {
            user_id: Uuid::nil(),
            role: "admin".into(),
            capability_ids: catalog
                .capabilities
                .iter()
                .filter(|capability| capability.status == "approved_mvp")
                .map(|capability| capability.id.clone())
                .collect(),
            office_ids: vec![1],
            can_view_pii: true,
            legacy_api_key_id: None,
        }
    }

    #[tokio::test]
    async fn planning_agent_stops_at_the_model_turn_budget_without_a_partial_proposal() {
        let catalog = KnowledgeLoader::new("../../knowledge", "../../queries")
            .load()
            .unwrap();
        let agent = PlanningAgent::new(Arc::new(FakeLlmClient::default()), 2);
        assert_eq!(agent.tools(&catalog).len(), 6);
        assert!(!agent.dynamic_tools());
        assert!(!agent.dynamic_context());
        assert_eq!(
            agent.propose("report", &catalog).await.unwrap_err(),
            PlanningAgentError::MaxTurnsExceeded
        );
    }

    #[tokio::test]
    async fn valid_agent_proposal_flows_through_the_real_compiler_and_verifier() {
        let catalog = KnowledgeLoader::new("../../knowledge", "../../queries")
            .load()
            .unwrap();
        let capability_id = catalog
            .capabilities
            .iter()
            .find(|capability| capability.status == "approved_mvp")
            .expect("approved capability")
            .id
            .clone();
        let fake = std::sync::Arc::new(FakeLlmClient::default());
        fake.push_structured(serde_json::json!({
            "capability_ids": [capability_id],
            "nodes": [],
            "edges": [],
        }));
        let agent = PlanningAgent::new(fake, 1);

        let proposal = agent.propose("approved report", &catalog).await.unwrap();
        let workflow = compile(proposal, &catalog, Uuid::nil(), budgets()).unwrap();
        verify(workflow, &principal(&catalog), &catalog).expect("verified workflow");
    }

    /// The sentence -> proposal path with a REAL model (DeepSeek). Gated behind
    /// `RUN_LIVE_DEEPSEEK_PLANNER=1` + `DEEPSEEK_API_KEY` (mirrors the
    /// `RUN_LIVE_SCENARIO_MATRIX` gate): the deterministic `FakeLlmClient`
    /// cannot exercise a genuine model turning natural language into a
    /// `WorkflowProposal`. Proves the planner's output still travels through the
    /// real compiler + verifier — the model is never trusted to emit SQL or an
    /// unapproved capability.
    #[tokio::test]
    async fn live_deepseek_planner_proposes_a_compilable_workflow_from_a_sentence() {
        if std::env::var("RUN_LIVE_DEEPSEEK_PLANNER").ok().as_deref() != Some("1") {
            eprintln!("skipping: set RUN_LIVE_DEEPSEEK_PLANNER=1 to run the live planner test");
            return;
        }
        let Ok(api_key) = std::env::var("DEEPSEEK_API_KEY") else {
            eprintln!("skipping: DEEPSEEK_API_KEY unset");
            return;
        };
        if api_key.trim().is_empty() {
            eprintln!("skipping: DEEPSEEK_API_KEY empty");
            return;
        }

        let config = app_core::config::LlmConfig {
            provider: "deepseek".into(),
            api_key,
            chat_completions_url: std::env::var("DEEPSEEK_CHAT_COMPLETIONS_URL")
                .unwrap_or_else(|_| "https://api.deepseek.com/chat/completions".into()),
            base_url: std::env::var("DEEPSEEK_BASE_URL")
                .unwrap_or_else(|_| "https://api.deepseek.com".into()),
            model: std::env::var("DEEPSEEK_MODEL").unwrap_or_else(|_| "deepseek-chat".into()),
            timeout_ms: 30_000,
            max_retries: 2,
            max_output_tokens: 2_000,
            temperature: 0.0,
        };
        let client: crate::assistant::llm::SharedLlmClient = Arc::new(
            crate::assistant::llm::provider::LlmProvider::new(&config, None)
                .expect("build DeepSeek provider"),
        );

        let catalog = KnowledgeLoader::new("../../knowledge", "../../queries")
            .load()
            .unwrap();
        let agent = PlanningAgent::new(client, 4);

        let proposal = agent
            .propose(
                "Show a savings overview for a client: account count, unpaid charges, transactions.",
                &catalog,
            )
            .await
            .expect("the planner proposes a workflow from a real sentence");

        assert!(
            !proposal.capability_ids.is_empty(),
            "the planner selected at least one capability from the sentence"
        );
        for id in &proposal.capability_ids {
            assert!(
                catalog
                    .capabilities
                    .iter()
                    .any(|capability| &capability.id == id && capability.status == "approved_mvp"),
                "planner proposed a non-approved capability id: {id}"
            );
        }

        // Contract: the planner SELECTS approved capabilities; the deterministic
        // compiler builds the graph (a raw model's hand-built nodes are not
        // trusted). Feed a capability-ids-only proposal — the same shape
        // `propose_workflow` emits — through the real compiler + verifier so the
        // model can never smuggle SQL or an unsafe node past them.
        let selection = WorkflowProposal {
            capability_ids: proposal.capability_ids.clone(),
            nodes: vec![],
            edges: vec![],
        };
        let workflow = compile(selection, &catalog, Uuid::nil(), budgets())
            .expect("planner selection compiles");
        verify(workflow, &principal(&catalog), &catalog).expect("planner selection verifies");
    }
}
