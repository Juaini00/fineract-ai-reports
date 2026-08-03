use std::{collections::HashMap, sync::Arc};

use anyhow::Result;

use crate::{
    assistant::{
        AssistantEntityType, Evidence, RequestGrouping, RequestOperation, RequestOutput,
        RequestPii, RequestSubject, RetrievalPlan, llm::SharedLlmClient,
    },
    knowledge::{index::repository::KnowledgeRepository, model::KnowledgeCatalog},
};

/// Boost applied to a candidate's score for each matched request_shape
/// dimension (see `shape_score`), up to a cap of 0.99.
const SHAPE_BOOST: f32 = 0.30;
/// Boost for a candidate whose declared `domain` equals the router's guess.
/// Same magnitude `catalog_fallback` already used, now applied in one place so
/// the embedding arm gets it too (it previously got a SQL `WHERE` instead).
const DOMAIN_BOOST: f32 = 0.05;
/// Boost for a candidate declaring every metric the request named, resolved
/// through `KnowledgeCatalog::resolve_metric_id`.
const METRIC_BOOST: f32 = 0.25;
/// Penalty applied when the router flagged the message as off-topic. A hint,
/// not a veto: a candidate with real keyword/metric/shape support still clears
/// the floor, while a candidate riding on one incidental word no longer does.
const OUT_OF_DOMAIN_PENALTY: f32 = 0.15;

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

        // Apply every catalog-derived prior in one place, so the embedding arm
        // and the catalog arm are ranked by the same rules. None of these is a
        // gate: a candidate the priors dislike is ranked down and left for the
        // reranker, which is the only stage allowed to answer "unsupported".
        if let Some(catalog) = catalog {
            let out_of_domain = plan.intent == crate::assistant::AssistantIntentKind::OutOfDomain;
            for item in evidence.iter_mut() {
                if let Some(cap) = catalog
                    .capabilities
                    .iter()
                    .find(|c| c.id == item.capability_id)
                {
                    item.score += shape_score(plan, cap) * SHAPE_BOOST
                        + domain_score(plan, &cap.domain) * DOMAIN_BOOST
                        + metric_score(plan, catalog, &cap.metrics) * METRIC_BOOST;
                }
                if out_of_domain {
                    item.score -= OUT_OF_DOMAIN_PENALTY;
                }
                item.score = item.score.clamp(0.0, 0.99);
            }
        }

        let evidence = merge(evidence);
        if let Some(catalog) = catalog {
            return Ok(evidence
                .into_iter()
                .filter(|item| item.score >= catalog.classification.min_floor)
                .collect());
        }

        Ok(evidence)
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
    if request.operation != RequestOperation::Unknown && request.operation == cap.operation {
        hits += 1;
    }
    if request.subject != RequestSubject::Unknown && request.subject == cap.subject {
        hits += 1;
    }
    if request.grouping != RequestGrouping::Unknown && request.grouping == cap.grouping {
        hits += 1;
    }
    if request.output != RequestOutput::Unknown && request.output == cap.output {
        hits += 1;
    }
    // Scoring keeps the strict reading that `compatible_ids` had to give up:
    // a request that did not ask for a person earns no bonus from a capability
    // that returns one. Relaxing the gate without keeping the score strict
    // would hand every identity-returning capability a free point and let it
    // tie with the population-level capability the question was actually about.
    if request.pii != RequestPii::Unknown && pii_matches(&request.pii, &cap.pii) {
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
        .filter(|cap| {
            plan.allow_all_capabilities || plan.allowed_capabilities.iter().any(|id| id == &cap.id)
        })
        .filter(|cap| parameters_feasible(plan, &cap.required_parameters))
        // shape/domain/metric no longer gate here — retrieve() scores them.
        // The metric gate in particular demanded exact token equality against
        // `cap.metrics`, so a router-emitted `deposits` matched no capability
        // declaring `deposit` and this arm returned nothing at all.
        .map(|cap| {
            let haystack = format!(
                "{} {} {} {}",
                cap.id,
                cap.display_name.clone().unwrap_or_default(),
                cap.description.clone().unwrap_or_default(),
                cap.examples.join(" ")
            )
            .to_lowercase();
            let hits = terms
                .iter()
                .filter(|term| haystack.contains(term.as_str()))
                .count() as f32;
            // Compare coverage of the request vocabulary, not raw hits. Raw
            // hit counts pushed unrelated broad capabilities to the 0.99 cap
            // after three shared terms, erasing the rank/gap signal before
            // catalog-specific terms could differentiate them.
            let keyword_score = if terms.is_empty() {
                0.0
            } else {
                hits / terms.len() as f32 * 0.34
            };
            // Metric and domain nudges used to live here, matched against the
            // description haystack. They now live in `retrieve()` so the
            // embedding arm is scored by the same rules, and the metric one
            // goes through `KnowledgeCatalog::resolve_metric_id` instead of
            // substring-matching prose.
            Evidence {
                capability_id: cap.id.clone(),
                title: cap
                    .display_name
                    .clone()
                    .unwrap_or_else(|| crate::assistant::clarification::humanize_id(&cap.id)),
                score: (0.25 + keyword_score).min(0.99),
                source_type: "capability".into(),
                metadata: serde_json::json!({
                    "domain": cap.domain,
                    "query_id": cap.query_id,
                    "description": cap.description,
                    "examples": cap.examples,
                    "output_mode": cap.output_mode,
                }),
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
        .filter(|cap| metric_declared(&requested_metric_ids(plan, catalog), catalog, &cap.metrics))
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

/// Issue 011: the router says `pii: none` for any question that does not *ask*
/// for a person, but "list savings charges" inevitably returns client identity,
/// so equality on this axis excluded every genuine candidate and
/// `compatible_ids` came back empty on both reported requests. What a
/// capability returns is gated by `policy::authorization`, not by retrieval —
/// so only the opposite direction is a real mismatch: a request that needs
/// identity cannot be served by a capability that returns none.
fn pii_compatible(request: &RequestPii, capability: &RequestPii) -> bool {
    !matches!(request, RequestPii::ClientIdentity) || !matches!(capability, RequestPii::None)
}

/// The strict reading, used only for ranking. `ClientIdentity` still satisfies
/// `ConditionalClientIdentity` — that relaxation was always about how a
/// capability declares itself, not about what the request wanted.
fn pii_matches(request: &RequestPii, capability: &RequestPii) -> bool {
    request == capability
        || matches!(
            (request, capability),
            (
                RequestPii::ClientIdentity,
                RequestPii::ConditionalClientIdentity
            )
        )
}

/// 1.0 when the capability's declared `domain` is the one the router guessed.
/// This used to be `evidence::domain_filter`, i.e. a SQL `WHERE` on the vector
/// arm; it is a ranking signal, not a boundary.
fn domain_score(plan: &RetrievalPlan, capability_domain: &str) -> f32 {
    capability_domain.eq_ignore_ascii_case(&format!("{:?}", plan.domain)) as i32 as f32
}

/// Metric ids the request named, resolved through the catalog so a surface
/// form the router emitted (`deposits`, `savings balance`) lands on the same
/// canonical id the capability declares. Spellings the catalog does not
/// recognise are dropped: an unresolvable name cannot match a declaration, and
/// treating it as a requirement is exactly what used to empty the candidate
/// list.
fn requested_metric_ids<'a>(plan: &RetrievalPlan, catalog: &'a KnowledgeCatalog) -> Vec<&'a str> {
    let mut ids: Vec<&str> = plan
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
        .filter_map(|raw| catalog.resolve_metric_id(raw))
        .collect();
    ids.sort_unstable();
    ids.dedup();
    ids
}

/// 1.0 when the capability declares every metric the request named. Formerly a
/// `.filter()`, which meant one unrecognised spelling removed every candidate.
fn metric_score(plan: &RetrievalPlan, catalog: &KnowledgeCatalog, metrics: &[String]) -> f32 {
    let requested = requested_metric_ids(plan, catalog);
    if requested.is_empty() {
        return 0.0;
    }
    metric_declared(&requested, catalog, metrics) as i32 as f32
}

fn metric_declared(requested: &[&str], catalog: &KnowledgeCatalog, metrics: &[String]) -> bool {
    let declared: Vec<&str> = metrics
        .iter()
        .filter_map(|metric| catalog.resolve_metric_id(metric))
        .collect();
    requested.iter().all(|id| declared.contains(id))
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
