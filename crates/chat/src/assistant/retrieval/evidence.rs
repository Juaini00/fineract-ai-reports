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
            metadata_filters: domain_filter(&intent.domain),
            allow_all_capabilities,
            allowed_capabilities,
            source_snippets: Vec::new(),
        }
    }
}

fn domain_filter(domain: &AssistantDomain) -> std::collections::BTreeMap<String, String> {
    let mut filters = std::collections::BTreeMap::new();
    if !matches!(domain, AssistantDomain::Unknown) {
        filters.insert("domain".into(), format!("{:?}", domain).to_lowercase());
    }
    filters
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
