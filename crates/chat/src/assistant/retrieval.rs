use std::{collections::HashMap, sync::Arc};

use anyhow::Result;

use crate::{
    assistant::{
        AssistantEntityType, Evidence, RequestGrouping, RequestOperation, RequestOutput,
        RequestPii, RequestSubject, RetrievalPlan, llm::SharedLlmClient,
    },
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
        let compatible = catalog.map(|catalog| compatible_ids(plan, catalog));
        if compatible.as_ref().is_some_and(Vec::is_empty) {
            return Ok(Vec::new());
        }
        let search_ids = compatible.clone().or_else(|| allowed_ids(plan));
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
                        search_ids.as_deref(),
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
        if let Some(compatible) = compatible {
            evidence.retain(|item| compatible.contains(&item.capability_id));
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
    let compatible = compatible_ids(plan, catalog);
    catalog
        .capabilities
        .iter()
        .filter(|cap| compatible.contains(&cap.id))
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

pub fn compatible_ids(plan: &RetrievalPlan, catalog: &KnowledgeCatalog) -> Vec<String> {
    catalog
        .capabilities
        .iter()
        .filter(|cap| matches!(cap.status.as_str(), "approved_mvp" | "active"))
        .filter(|cap| plan.allow_all_capabilities || plan.allowed_capabilities.contains(&cap.id))
        .filter(|cap| domain_compatible(plan, &cap.domain))
        .filter(|cap| shape_compatible(&plan.request_shape, &cap.request_shape))
        .filter(|cap| metric_compatible(plan, &cap.metrics))
        .filter(|cap| parameters_feasible(plan, &cap.required_parameters))
        .map(|cap| cap.id.clone())
        .collect()
}

fn domain_compatible(plan: &RetrievalPlan, domain: &str) -> bool {
    matches!(plan.domain, crate::assistant::AssistantDomain::Unknown)
        || format!("{:?}", plan.domain).eq_ignore_ascii_case(domain)
}

fn shape_compatible(
    request: &crate::assistant::RequestShape,
    capability: &crate::assistant::RequestShape,
) -> bool {
    enum_compatible(
        &request.operation,
        &capability.operation,
        &RequestOperation::Unknown,
    ) && enum_compatible(
        &request.subject,
        &capability.subject,
        &RequestSubject::Unknown,
    ) && enum_compatible(
        &request.grouping,
        &capability.grouping,
        &RequestGrouping::Unknown,
    ) && enum_compatible(&request.output, &capability.output, &RequestOutput::Unknown)
        && pii_compatible(&request.pii, &capability.pii)
}

fn enum_compatible<T: PartialEq>(request: &T, capability: &T, unknown: &T) -> bool {
    request == unknown || request == capability
}

fn pii_compatible(request: &RequestPii, capability: &RequestPii) -> bool {
    matches!(request, RequestPii::Unknown)
        || request == capability
        || matches!(
            (request, capability),
            (
                RequestPii::ClientIdentity,
                RequestPii::ConditionalClientIdentity
            )
        )
}

fn metric_compatible(plan: &RetrievalPlan, metrics: &[String]) -> bool {
    let requested = plan
        .constraints
        .get("metric")
        .and_then(|value| value.as_str())
        .into_iter()
        .chain(
            plan.entities
                .iter()
                .filter(|entity| entity.entity_type == AssistantEntityType::Metric)
                .map(|entity| entity.canonical.as_deref().unwrap_or(&entity.value)),
        )
        .flat_map(keyword_terms)
        .filter(|term| !matches!(term.as_str(), "amount" | "total"))
        .collect::<Vec<_>>();
    requested.is_empty()
        || requested.iter().all(|term| {
            metrics
                .iter()
                .any(|metric| keyword_terms(metric).contains(term))
        })
}

fn parameters_feasible(plan: &RetrievalPlan, required: &[String]) -> bool {
    !required.iter().any(|parameter| parameter == "search")
        || plan
            .entities
            .iter()
            .any(|entity| entity.entity_type == AssistantEntityType::PersonName)
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
