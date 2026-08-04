use app_core::auth::model::PrincipalContext;

use crate::assistant::{
    llm::{self, LlmPurpose, SharedLlmClient},
    understanding::gateway::{
        CapabilitySummary, LlmGatewayExtraction, build_gateway_prompt, capability_summary,
    },
};
use crate::knowledge::model::KnowledgeCatalog;

const SYSTEM_PROMPT: &str = "You are the reporting assistant's structured understanding agent. \
    Return a single JSON object matching the LlmGatewayExtraction schema and nothing else.";

/// Bounded, structured extraction boundary for the understanding stage.
///
/// `LlmProvider` executes each model call through Rig's structured agent. This
/// layer adds the catalog-visible vocabulary check before an extraction can
/// reach retrieval, planning, or execution. Its output remains advisory: it
/// has no capability-selection authority.
pub struct UnderstandingAgent {
    llm: SharedLlmClient,
    max_turns: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnderstandingAgentError {
    CatalogVocabulary,
    MaxTurnsExceeded,
}

impl std::fmt::Display for UnderstandingAgentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CatalogVocabulary => {
                f.write_str("understanding extraction used unavailable catalog vocabulary")
            }
            Self::MaxTurnsExceeded => {
                f.write_str("understanding agent exceeded its model-turn budget")
            }
        }
    }
}

impl std::error::Error for UnderstandingAgentError {}

impl UnderstandingAgent {
    pub fn new(llm: SharedLlmClient, max_turns: u8) -> Self {
        Self {
            llm,
            max_turns: max_turns.max(1),
        }
    }

    pub fn max_turns(&self) -> u8 {
        self.max_turns
    }

    pub async fn extract(
        &self,
        user_message: &str,
        history: Option<&str>,
        catalog: &KnowledgeCatalog,
        principal: &PrincipalContext,
    ) -> Result<LlmGatewayExtraction, UnderstandingAgentError> {
        let visible = visible_capabilities(catalog, principal);
        let summary: Vec<CapabilitySummary<'_>> = visible
            .iter()
            .map(|capability| capability_summary(capability))
            .collect();
        let user = build_gateway_prompt(user_message, &summary, history);

        for turn in 0..self.max_turns {
            match llm::structured::<LlmGatewayExtraction>(
                self.llm.as_ref(),
                LlmPurpose::RouteIntent,
                SYSTEM_PROMPT,
                &user,
                None,
            )
            .await
            {
                Ok(response) => {
                    let extraction = response.value;
                    if extraction.candidates.iter().all(|candidate| {
                        visible
                            .iter()
                            .any(|capability| capability.id == candidate.capability_id)
                    }) {
                        return Ok(extraction);
                    }
                    return Err(UnderstandingAgentError::CatalogVocabulary);
                }
                Err(error) => tracing::warn!(
                    target: "assistant::understanding_agent",
                    turn = turn + 1,
                    max_turns = self.max_turns,
                    %error,
                    "structured understanding turn failed"
                ),
            }
        }

        Err(UnderstandingAgentError::MaxTurnsExceeded)
    }
}

fn visible_capabilities<'a>(
    catalog: &'a KnowledgeCatalog,
    principal: &PrincipalContext,
) -> Vec<&'a crate::knowledge::model::CapabilityKnowledge> {
    catalog
        .capabilities
        .iter()
        .filter(|capability| {
            capability.status == "approved_mvp"
                && principal
                    .capability_ids
                    .iter()
                    .any(|id| id == &capability.id)
        })
        .collect()
}
