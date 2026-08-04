//! Refuse candidates that cannot honour a filter the user named.
//!
//! The worst failure mode this system has is answering a *different* question
//! and reporting success. Production evidence: "berikan saya 5 client yg ada
//! pada <office>" was answered by `client_random_sample`, which at the time had
//! no user-supplied office parameter at all — every client in all eight
//! authorized offices came back, `terminal_state: "completed"`.
//!
//! `reranker::RERANKER_SYSTEM` gained a rule telling the model never to pick a
//! candidate that drops a filter the user named. That is an instruction, not a
//! gate: it holds most of the time and fails silently the rest. This module is
//! the gate. It runs between retrieval and reranking, so an insufficient
//! candidate is never offered to the model in the first place, and an empty
//! candidate list reaches the reranker's existing `unsupported` short-circuit.
//!
//! Everything about a capability's side of the comparison is read from the
//! catalog — the query's declared parameters plus
//! `knowledge/parameter-bindings/bindings.yaml`. There is deliberately no list
//! of capability ids here: a capability gains the ability to honour a filter the
//! moment the catalog says a non-scope parameter of its query is filled by that
//! fact, and loses it the moment the catalog stops saying so.

use std::collections::BTreeSet;

use super::evidence::Evidence;
use crate::assistant::{AssistantIntent, ConstraintField, DeterministicExtraction, TypedFactValue};
use crate::knowledge::model::KnowledgeCatalog;

/// Facts whose absence changes *which rows* come back, so a capability that
/// cannot bind one is answering a different question rather than the same
/// question less precisely.
///
/// Deliberately narrow. `from_date`/`to_date` are the tempting next entries and
/// are left out on purpose: the temporal extractor fires on incidental wording
/// ("how many accounts are there today"), and refusing every snapshot capability
/// because the user said "today" turns working questions into "unsupported" —
/// a false refusal is its own harm, just a quieter one. `limit` and `metric` are
/// out because neither is a row filter: a limit has a catalog default, and a
/// wrong metric is already blocked by `verify_capability_metric`.
///
/// ponytail: hand-picked field set, with a known ceiling — it catches "which
/// rows" filters only. Widen it when a *measured* production failure names a
/// field, not before.
const GATED_FILTERS: &[ConstraintField] = &[
    ConstraintField::Office,
    ConstraintField::PersonName,
    ConstraintField::ClientId,
    ConstraintField::Product,
    ConstraintField::ChargeType,
    ConstraintField::AccountNumber,
    ConstraintField::TransactionAmount,
];

/// The filters this turn's message *clearly* expressed.
///
/// The bar is: the value the fact carries occurs in the user's own message.
/// That is the same standard the deterministic extractor already meets (it is a
/// substring reader over the message), and it is what makes a model-supplied
/// entity admissible here even when `MODEL_TRUSTED_ENTITIES` would not let it
/// *bind* to SQL — the two decisions carry different risks. Binding a
/// hallucinated office name returns the wrong rows, so binding demands a
/// verified fact; refusing on a hallucinated office name blocks a question the
/// user never asked to be narrowed, so refusing demands evidence the words were
/// typed. A surface match is that evidence, and it costs one `contains` —
/// literally the same `occurs_verbatim` binding now uses for a person name.
///
/// Consequence worth stating: an office the user meant but never spelled — one
/// only an LLM inferred from context — does not gate. That is the conservative
/// direction.
pub fn expressed_filters(
    message: &str,
    intent: Option<&AssistantIntent>,
    extraction: Option<&DeterministicExtraction>,
) -> BTreeSet<ConstraintField> {
    let facts = crate::assistant::execution::tool::request_facts(Some(message), intent, extraction);
    facts
        .into_iter()
        .filter(|(field, _)| GATED_FILTERS.contains(field))
        .filter(|(_, value)| {
            surface_text(value).is_some_and(|text| {
                crate::assistant::execution::tool::occurs_verbatim(message, &text)
            })
        })
        .map(|(field, _)| field)
        .collect()
}

/// The literal the user would have had to type for this fact to be theirs.
/// `None` for fact shapes that are computed rather than quoted — those cannot
/// clear the surface-match bar and so never gate.
fn surface_text(value: &TypedFactValue) -> Option<String> {
    match value {
        TypedFactValue::Office(text)
        | TypedFactValue::PersonName(text)
        | TypedFactValue::Product(text)
        | TypedFactValue::ChargeType(text)
        | TypedFactValue::AccountNumber(text)
        | TypedFactValue::Decimal(text) => Some(text.clone()),
        TypedFactValue::ClientId(id) => Some(id.to_string()),
        _ => None,
    }
}

/// Can this capability's approved query accept `field` from the user?
///
/// `authorized_scope` parameters are excluded by design. `office_ids` is the
/// authorization boundary — the set of offices this caller may see at all — and
/// binding it is not the same act as honouring "clients in <office>". Treating
/// it as satisfaction is precisely how a request naming one office was answered
/// with all eight.
pub fn capability_honours(
    catalog: &KnowledgeCatalog,
    capability_id: &str,
    field: &ConstraintField,
) -> bool {
    let Some(query) = catalog
        .capabilities
        .iter()
        .find(|cap| cap.id == capability_id)
        .and_then(|cap| catalog.queries.iter().find(|q| q.id == cap.query_id))
    else {
        // Not a catalog capability (or a capability with no approved query):
        // nothing to judge, so judge nothing. Refusing here would turn a
        // catalog-shape problem into a user-visible "unsupported".
        return true;
    };
    query.parameters.iter().any(|parameter| {
        parameter.source.as_deref() != Some("authorized_scope")
            && catalog.binding_fields(&parameter.name).contains(field)
    })
}

/// Drops every candidate that cannot honour a filter the user expressed.
///
/// Drop rather than ask: no answer the user could give changes what the approved
/// catalog can bind, so a clarification round here would collect a value with
/// nowhere to put it. The two honest outcomes both fall out of this one filter.
/// When some candidate *can* honour the filter, the insufficient ones simply
/// stop competing and the reranker chooses among capabilities that actually
/// answer the question — no round trip, no user cost. When none can, the list
/// empties and `LlmReranker::rerank` returns `unsupported` on its existing
/// empty-candidates path, which is the truthful answer: this catalog cannot
/// filter that way.
pub fn drop_insufficient(
    catalog: &KnowledgeCatalog,
    expressed: &BTreeSet<ConstraintField>,
    evidence: Vec<Evidence>,
) -> Vec<Evidence> {
    if expressed.is_empty() {
        return evidence;
    }
    evidence
        .into_iter()
        .filter(|item| {
            let unhonoured: Vec<&ConstraintField> = expressed
                .iter()
                .filter(|field| !capability_honours(catalog, &item.capability_id, field))
                .collect();
            if !unhonoured.is_empty() {
                tracing::info!(
                    target: "assistant::mapping",
                    capability_id = %item.capability_id,
                    unhonoured = ?unhonoured,
                    "candidate dropped: cannot bind a filter the user named"
                );
            }
            unhonoured.is_empty()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assistant::extract_message_facts;
    use crate::knowledge::catalog::loader::KnowledgeLoader;

    fn catalog() -> KnowledgeCatalog {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        KnowledgeLoader::new(root.join("knowledge"), root.join("queries"))
            .load()
            .expect("load catalog")
    }

    fn evidence(id: &str) -> Evidence {
        Evidence {
            capability_id: id.into(),
            title: id.into(),
            score: 0.5,
            source_type: "capability".into(),
            metadata: serde_json::json!({}),
            conflicting: false,
        }
    }

    #[test]
    fn a_named_office_is_expressed_and_gates_capabilities_that_cannot_bind_it() {
        let catalog = catalog();
        let message = "give me 5 clients in Head Office";
        let extraction = extract_message_facts(message);
        let expressed = expressed_filters(message, None, Some(&extraction));
        assert!(
            expressed.contains(&ConstraintField::Office),
            "deterministic extraction should see the office: {expressed:?}"
        );

        // Driven off the catalog, never off an id: whichever approved
        // capabilities currently lack an office-binding parameter, those are the
        // ones that must not survive.
        let (honours, drops): (Vec<_>, Vec<_>) = catalog
            .capabilities
            .iter()
            .filter(|cap| cap.status == "approved_mvp")
            .map(|cap| cap.id.clone())
            .partition(|id| capability_honours(&catalog, id, &ConstraintField::Office));
        assert!(
            !drops.is_empty(),
            "premise gone: every approved capability now binds an office filter, \
             so this test can no longer observe a drop"
        );

        let survivors = drop_insufficient(
            &catalog,
            &expressed,
            honours
                .iter()
                .chain(drops.iter())
                .map(|id| evidence(id))
                .collect(),
        );
        let survivor_ids: Vec<&str> = survivors
            .iter()
            .map(|item| item.capability_id.as_str())
            .collect();
        assert_eq!(
            survivor_ids, honours,
            "only office-capable candidates survive"
        );
    }

    #[test]
    fn an_office_scope_parameter_does_not_count_as_honouring_the_user() {
        let catalog = catalog();
        // `office_ids` is `source: authorized_scope` everywhere in the catalog.
        // If it ever satisfied a user-expressed office, every capability would
        // pass this gate and the gate would be decoration.
        assert!(
            catalog.queries.iter().any(|query| {
                query.parameters.iter().any(|p| {
                    p.name == "office_ids" && p.source.as_deref() == Some("authorized_scope")
                })
            }),
            "premise: office_ids is an authorized_scope parameter"
        );
        assert!(
            catalog
                .capabilities
                .iter()
                .filter(|cap| cap.status == "approved_mvp")
                .any(|cap| !capability_honours(&catalog, &cap.id, &ConstraintField::Office)),
            "an authorized_scope office parameter must not satisfy a user office filter"
        );
    }

    #[test]
    fn a_value_the_user_never_typed_does_not_gate() {
        let intent: AssistantIntent = serde_json::from_value(serde_json::json!({
            "intent": "report_request",
            "domain": "client",
            "entities": [{ "entity_type": "office", "value": "Kuala Lumpur Branch" }],
        }))
        .expect("intent fixture");
        let expressed = expressed_filters("how many clients do we have?", Some(&intent), None);
        assert!(
            expressed.is_empty(),
            "an office nobody typed must not refuse anything: {expressed:?}"
        );
    }
}
