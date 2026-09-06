use super::*;

#[cfg(test)]
pub(super) fn merge_deterministic_extraction(
    memory: &mut JobMemory,
    intent: &mut AssistantIntent,
    message: &str,
) {
    let extraction = extract_message_facts(message);
    let conflicts = extraction.conflicts_with(intent);
    if !conflicts.is_empty() {
        memory.current_user_message_metadata["deterministic_extraction_conflicts"] =
            serde_json::to_value(conflicts).unwrap_or_else(|_| json!([]));
    }
    extraction.merge_into(intent);
    record_extraction_metadata(memory, &extraction);
}

pub(super) fn merge_deterministic_extraction_at(
    memory: &mut JobMemory,
    intent: &mut AssistantIntent,
    message: &str,
    canonical: Option<&CanonicalRuntimeContext>,
) {
    let extraction = extract_for_context(message, canonical);
    let conflicts = extraction.conflicts_with(intent);
    if !conflicts.is_empty() {
        memory.current_user_message_metadata["deterministic_extraction_conflicts"] =
            serde_json::to_value(conflicts).unwrap_or_else(|_| json!([]));
    }
    extraction.merge_into(intent);
    record_extraction_metadata(memory, &extraction);
}

/// Records deterministic extraction metadata for a clarification-reply turn.
///
/// Bug 08-B: previously this only re-extracted facts from the *original*
/// Turn-1 prompt (`payload.source_intent.prompt`), which clobbers whatever
/// the user actually said in their Turn-2 reply. `verify_capability_metric`
/// (tool.rs) then gates the newly *selected* capability against a metric
/// extracted from Turn-1's wording — even though the user just explicitly
/// picked a different capability in Turn 2. Refresh from the current turn's
/// message and let it take priority; fall back to the Turn-1 extraction only
/// for fields the current turn's message didn't mention (e.g. a bare "3"
/// limit stated only in Turn 1).
pub(super) fn record_source_extraction_metadata(
    memory: &mut JobMemory,
    payload: &ClarificationPayload,
    canonical: Option<&CanonicalRuntimeContext>,
    current_message: &str,
) {
    let source_extraction = payload
        .source_intent
        .as_ref()
        .map(|source| extract_for_context(&source.prompt, canonical))
        .unwrap_or_default();
    let current_extraction = extract_for_context(current_message, canonical);
    let refreshed = prefer_current_turn_extraction(source_extraction, current_extraction);
    record_extraction_metadata(memory, &refreshed);
}

/// Merges two extraction passes, letting the current turn's signals win over
/// the original (Turn-1) source prompt's — see `record_source_extraction_metadata`.
pub(super) fn prefer_current_turn_extraction(
    source: DeterministicExtraction,
    current: DeterministicExtraction,
) -> DeterministicExtraction {
    DeterministicExtraction {
        constraints: crate::assistant::AssistantConstraints {
            quantity: current.constraints.quantity.or(source.constraints.quantity),
            from_date: current
                .constraints
                .from_date
                .or(source.constraints.from_date),
            to_date: current.constraints.to_date.or(source.constraints.to_date),
            currency_code: current
                .constraints
                .currency_code
                .or(source.constraints.currency_code),
            product_ids: current
                .constraints
                .product_ids
                .or(source.constraints.product_ids),
            office_ids: current
                .constraints
                .office_ids
                .or(source.constraints.office_ids),
            metric: current.constraints.metric.or(source.constraints.metric),
            transaction_amount: current
                .constraints
                .transaction_amount
                .or(source.constraints.transaction_amount),
        },
        domain: current.domain.or(source.domain),
        entities: if current.entities.is_empty() {
            source.entities
        } else {
            current.entities
        },
        candidates: if current.candidates.is_empty() {
            source.candidates
        } else {
            current.candidates
        },
        temporal_provenance: current.temporal_provenance.or(source.temporal_provenance),
        temporal_error: current.temporal_error.or(source.temporal_error),
    }
}

pub(super) fn extract_for_context(
    message: &str,
    canonical: Option<&CanonicalRuntimeContext>,
) -> DeterministicExtraction {
    canonical
        .map(|context| {
            extract_message_facts_at(
                message,
                context.reference_instant,
                context.business_today,
                366,
            )
        })
        .unwrap_or_else(|| extract_message_facts(message))
}

pub(super) fn record_extraction_metadata(
    memory: &mut JobMemory,
    extraction: &crate::assistant::DeterministicExtraction,
) {
    if !extraction.is_empty() {
        memory.current_user_message_metadata["deterministic_extraction"] =
            serde_json::to_value(extraction).unwrap_or_else(|_| json!({}));
    }
}
