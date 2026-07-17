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
        // Auth boundary: restrict to caller's allowed_capabilities.
        // Catalog-wide search is NOT the same as widening auth.
        let search_ids = allowed_ids(plan);

        let mut evidence: Vec<Evidence> = Vec::new();

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
                        16,
                    )
                    .await?
                    .into_iter()
                    .map(Evidence::from),
            );
        }

        if let Some(catalog) = catalog {
            evidence.extend(catalog_fallback(plan, catalog));
        }

        // Boost each candidate by shape match against the plan (up to +0.30).
        if let Some(catalog) = catalog {
            let shape_boost = 0.30;
            for item in evidence.iter_mut() {
                if let Some(cap) = catalog
                    .capabilities
                    .iter()
                    .find(|c| c.id == item.capability_id)
                {
                    let score = shape_score(plan, cap);
                    item.score = (item.score + score * shape_boost).min(0.99);
                }
            }
        }

        Ok(merge(evidence))
    }
}

/// Score in [0.0, 1.0] measuring how many request_shape dimensions match.
/// 5 dimensions weighted equally; each match contributes 0.2. PII match
/// includes the ClientIdentity -> ConditionalClientIdentity relaxation
/// already used by `pii_compatible`.
pub fn shape_score(
    plan: &RetrievalPlan,
    capability: &crate::knowledge::model::CapabilityKnowledge,
) -> f32 {
    let request = &plan.request_shape;
    let cap = &capability.request_shape;
    let mut hits = 0u8;
    if enum_compatible(
        &request.operation,
        &cap.operation,
        &RequestOperation::Unknown,
    ) {
        hits += 1;
    }
    if enum_compatible(&request.subject, &cap.subject, &RequestSubject::Unknown) {
        hits += 1;
    }
    if enum_compatible(&request.grouping, &cap.grouping, &RequestGrouping::Unknown) {
        hits += 1;
    }
    if enum_compatible(&request.output, &cap.output, &RequestOutput::Unknown) {
        hits += 1;
    }
    if pii_compatible(&request.pii, &cap.pii) {
        hits += 1;
    }
    (hits as f32) / 5.0
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

// ponytail: plain English stopwords that would otherwise count as a keyword
// "hit" against nearly every capability description, saturating scores to
// the 0.99 cap and erasing the very ranking signal catalog_fallback exists
// to provide. Removing the shape hard-gate (issue 01) makes this matter far
// more than before, since it used to mask ties this loose. Upgrade path: a
// real term-frequency/reranker (issue 02) replaces this heuristic entirely.
const STOPWORDS: &[&str] = &[
    "the", "and", "for", "with", "this", "that", "show", "give", "want", "please", "most", "have",
    "where", "report", "see", "need", "berikan", "pada", "saya", "coba", "tahun", "ini",
];

pub fn catalog_fallback(plan: &RetrievalPlan, catalog: &KnowledgeCatalog) -> Vec<Evidence> {
    let terms: Vec<String> = plan_terms(plan)
        .into_iter()
        .filter(|term| !STOPWORDS.contains(&term.as_str()))
        .collect();
    catalog
        .capabilities
        .iter()
        .filter(|cap| matches!(cap.status.as_str(), "approved_mvp" | "active"))
        .filter(|cap| plan.allow_all_capabilities || plan.allowed_capabilities.iter().any(|id| id == &cap.id))
        // shape/domain no longer gate here — shape_score in retrieve() scores it instead
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
        .filter(|cap| shape_compatible(&plan.request_shape, &cap.request_shape))
        .filter(|cap| metric_compatible(plan, &cap.metrics))
        .filter(|cap| parameters_feasible(plan, &cap.required_parameters))
        .map(|cap| cap.id.clone())
        .collect()
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
    // ponytail: HashMap iteration order is randomized per-process; without a
    // stable tiebreak, capability ties (increasingly common now that shape is
    // a score, not a gate) would rank non-deterministically. Preserve
    // first-seen order (embedding search, then catalog order) as the tiebreak
    // for the stable sort below.
    let mut order: Vec<String> = Vec::new();
    let mut best: HashMap<String, Evidence> = HashMap::new();
    for item in items {
        if !best.contains_key(&item.capability_id) {
            order.push(item.capability_id.clone());
        }
        best.entry(item.capability_id.clone())
            .and_modify(|old| {
                if item.score > old.score {
                    *old = item.clone();
                }
            })
            .or_insert(item);
    }
    let mut merged: Vec<_> = order
        .into_iter()
        .filter_map(|id| best.remove(&id))
        .collect();
    merged.sort_by(|a, b| b.score.total_cmp(&a.score));
    merged
}
