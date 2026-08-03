use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    assistant::{
        AssistantDomain, AssistantEntity, AssistantIntent, AssistantIntentKind, RequestShape,
    },
    knowledge::index::repository::RetrievedKnowledgeCandidate,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RetrievalPlan {
    pub query_text: String,
    pub domain: AssistantDomain,
    pub intent: AssistantIntentKind,
    pub request_shape: RequestShape,
    #[serde(default)]
    pub entities: Vec<AssistantEntity>,
    #[serde(default)]
    pub constraints: serde_json::Value,
    #[serde(default)]
    pub metadata_filters: std::collections::BTreeMap<String, String>,
    pub allow_all_capabilities: bool,
    pub allowed_capabilities: Vec<String>,
    #[serde(default)]
    pub source_snippets: Vec<String>,
}

impl RetrievalPlan {
    pub fn new(
        query_text: impl Into<String>,
        intent: &AssistantIntent,
        allow_all_capabilities: bool,
        allowed_capabilities: Vec<String>,
    ) -> Self {
        Self {
            query_text: query_text.into(),
            domain: intent.domain.clone(),
            intent: intent.intent.clone(),
            request_shape: intent.request_shape.clone(),
            entities: intent.entities.clone(),
            constraints: serde_json::to_value(&intent.constraints).unwrap_or_default(),
            // Deliberately empty. The router's one-word domain guess used to
            // land here and `knowledge::index::repository` turned it into
            // `AND metadata_json->>'domain' = $n`, so the whole vector arm was
            // restricted to a domain nobody authorised — "which office has the
            // highest savings balance?" guessed `savings` and thereby excluded
            // `organization_office_savings_summary`, which declares
            // `domain: organization`. Domain is now a score term
            // (`retrieval::engine::domain_score`), not a WHERE clause.
            // Callers with a genuine hard filter may still populate this map.
            metadata_filters: Default::default(),
            allow_all_capabilities,
            allowed_capabilities,
            source_snippets: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Evidence {
    pub capability_id: String,
    pub title: String,
    pub score: f32,
    pub source_type: String,
    pub metadata: serde_json::Value,
    #[serde(default)]
    pub conflicting: bool,
}

impl From<RetrievedKnowledgeCandidate> for Evidence {
    fn from(candidate: RetrievedKnowledgeCandidate) -> Self {
        Self {
            capability_id: candidate.source_id,
            title: candidate.title,
            score: (1.0 - candidate.distance as f32).clamp(0.0, 1.0),
            source_type: candidate.source_type,
            metadata: candidate.metadata_json,
            conflicting: false,
        }
    }
}
