use std::{collections::HashMap, sync::Arc};

use anyhow::Result;

use crate::{
    assistant::{Evidence, RetrievalPlan, llm::SharedLlmClient},
    knowledge::{index::repository::KnowledgeRepository, model::KnowledgeCatalog},
};

pub struct RetrievalEngine;

impl RetrievalEngine {
    pub async fn retrieve(
        plan: &RetrievalPlan,
        llm: Option<&SharedLlmClient>,
        knowledge: Option<&KnowledgeRepository>,
        catalog: Option<&Arc<KnowledgeCatalog>>,
    ) -> Result<Vec<Evidence>> {
        let mut evidence = Vec::new();
        if let (Some(llm), Some(knowledge)) = (llm, knowledge) {
            let embedding = llm
                .embed(
                    crate::assistant::llm::LlmPurpose::EvidenceRetrieval,
                    &plan.query_text,
                )
                .await?
                .vector;
            evidence.extend(
                knowledge
                    .search_hybrid_by_source_type(
                        "capability",
                        embedding,
                        &keyword_terms(&plan.query_text),
                        allowed_ids(plan).as_deref(),
                        &plan.metadata_filters,
                        8,
                    )
                    .await?
                    .into_iter()
                    .map(Evidence::from),
            );
        }
        if let Some(catalog) = catalog {
            evidence.extend(catalog_fallback(plan, catalog));
        }
        Ok(merge(evidence))
    }
}

fn allowed_ids(plan: &RetrievalPlan) -> Option<Vec<String>> {
    (!plan.allow_all_capabilities).then(|| plan.allowed_capabilities.clone())
}

fn keyword_terms(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|part| part.len() > 2)
        .map(|part| part.to_lowercase())
        .collect()
}

fn plan_terms(plan: &RetrievalPlan) -> Vec<String> {
    let mut terms = keyword_terms(&plan.query_text);
    for entity in &plan.entities {
        terms.extend(keyword_terms(&entity.value));
        if let Some(canonical) = &entity.canonical {
            terms.extend(keyword_terms(canonical));
        }
    }
    terms.sort();
    terms.dedup();
    terms
}

pub fn catalog_fallback(plan: &RetrievalPlan, catalog: &KnowledgeCatalog) -> Vec<Evidence> {
    let terms = plan_terms(plan);
    catalog
        .capabilities
        .iter()
        .filter(|cap| plan.allow_all_capabilities || plan.allowed_capabilities.iter().any(|id| id == &cap.id))
        .filter(|cap| plan.metadata_filters.get("domain").map(|d| d == &cap.domain).unwrap_or(true))
        .map(|cap| {
            let haystack = format!("{} {} {} {}", cap.id, cap.display_name.clone().unwrap_or_default(), cap.description.clone().unwrap_or_default(), cap.examples.join(" ")).to_lowercase();
            let hits = terms.iter().filter(|term| haystack.contains(term.as_str())).count() as f32;
            let metric_terms = plan
                .entities
                .iter()
                .filter(|entity| matches!(entity.entity_type, crate::assistant::AssistantEntityType::Metric))
                .flat_map(|entity| keyword_terms(entity.canonical.as_deref().unwrap_or(&entity.value)))
                .collect::<Vec<_>>();
            let metric_boost = (!metric_terms.is_empty()
                && metric_terms.iter().all(|term| haystack.contains(term)))
                as i32 as f32
                * 0.25;
            Evidence {
                capability_id: cap.id.clone(),
                title: cap.display_name.clone().unwrap_or_else(|| cap.id.clone()),
                score: (0.25 + hits * 0.15 + metric_boost).min(0.99),
                source_type: "capability".into(),
                metadata: serde_json::json!({"domain": cap.domain, "query_id": cap.query_id, "description": cap.description}),
                conflicting: false,
            }
        })
        .filter(|e| e.score > 0.25)
        .collect()
}

fn merge(items: Vec<Evidence>) -> Vec<Evidence> {
    let mut best: HashMap<String, Evidence> = HashMap::new();
    for item in items {
        best.entry(item.capability_id.clone())
            .and_modify(|old| {
                if item.score > old.score {
                    *old = item.clone();
                }
            })
            .or_insert(item);
    }
    let mut merged: Vec<_> = best.into_values().collect();
    merged.sort_by(|a, b| b.score.total_cmp(&a.score));
    merged
}
