use super::*;
use crate::assistant::execution::tool::approved_default_patch;
use crate::assistant::{
    approved_default_observations, deterministic_observations_excluding_fields,
    observations_from_patch,
};

pub(super) async fn authoritative_plan(
    context: &CanonicalRuntimeContext,
    memory: &mut JobMemory,
    catalog: &KnowledgeCatalog,
    current_client: &PrincipalContext,
    capability_id: &str,
) -> anyhow::Result<
    Option<(
        crate::assistant::execution::plan::ExecutionPlan,
        PrincipalContext,
    )>,
> {
    let catalog_version = context
        .catalog_version
        .ok_or_else(|| anyhow::anyhow!("missing canonical catalog version"))?;
    if let Some(snapshot_id) = memory.planner_snapshot_id {
        let loaded = context
            .repository
            .get_planner_snapshot(snapshot_id, memory.job_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("missing planner snapshot"))?;
        anyhow::ensure!(
            loaded.capability_catalog_version == catalog_version,
            "mismatched planner snapshot catalog"
        );
        if loaded.revision == context.revision {
            let plan = plan_from_snapshot(catalog, &loaded)?;
            let principal = principal_from_snapshot(loaded.principal_projection);
            return Ok(Some((plan, principal)));
        }
        anyhow::ensure!(
            loaded.revision < context.revision,
            "mismatched planner snapshot revision"
        );
    }
    let extraction = memory
        .current_user_message_metadata
        .get("deterministic_extraction")
        .cloned()
        .and_then(|value| serde_json::from_value::<DeterministicExtraction>(value).ok())
        .unwrap_or_default();
    let source_id = context.message_id.to_string();
    let structured_patch = memory
        .current_user_message_metadata
        .get("validated_constraint_patch")
        .cloned()
        .and_then(|value| serde_json::from_value(value).ok())
        .unwrap_or_default();
    let extraction = memory
        .current_user_message_metadata
        .get("structured_deterministic_extraction")
        .cloned()
        .and_then(|value| serde_json::from_value::<DeterministicExtraction>(value).ok())
        .unwrap_or(extraction);
    let clarification_source_id = memory
        .current_user_message_metadata
        .get("clarification_id")
        .and_then(serde_json::Value::as_str)
        .map(|id| {
            let revision = memory
                .current_user_message_metadata
                .get("clarification_revision")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or_default();
            format!("clarification:{id}:{revision}")
        })
        .unwrap_or_else(|| source_id.clone());
    let approved_defaults = approved_default_patch(catalog, capability_id)?;
    let effective = if context.initial {
        let intent = memory
            .intent
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("missing accepted initial parse"))?;
        let mut original = OriginalIntent {
            id: stable_uuid(memory.job_id, 1),
            job_id: memory.job_id,
            schema_version: 1,
            raw_message_id: context.message_id,
            locale: intent.language.clone(),
            action: intent.intent.clone(),
            entities: intent.entities.clone(),
            metrics: intent.constraints.metric.clone().into_iter().collect(),
            groupings: vec![format!("{:?}", intent.request_shape.grouping).to_lowercase()],
            output: Some(format!("{:?}", intent.request_shape.output).to_lowercase()),
            parameters: Default::default(),
            pii_request: false,
            extraction_provenance: vec![ExtractionProvenance {
                extractor: "semantic_router".into(),
                version: "canonical_v1".into(),
                source_identifiers: vec![source_id.clone()],
                source_spans: Vec::new(),
                rule: None,
                reference_instant: None,
                timezone: None,
            }],
            created_at: context.reference_instant,
        };
        if let Some(provenance) = &extraction.temporal_provenance {
            original.extraction_provenance.push(ExtractionProvenance {
                extractor: "deterministic_temporal_resolver".into(),
                version: "v1".into(),
                source_identifiers: vec![source_id.clone()],
                source_spans: vec![provenance.phrase_span],
                rule: Some(provenance.rule.clone()),
                reference_instant: Some(provenance.reference_instant),
                timezone: Some(provenance.timezone.clone()),
            });
        }
        let mut observations = original_request_observations(
            memory.job_id,
            &source_id,
            intent,
            &extraction,
            context.observed_at,
        );
        observations.extend(approved_default_observations(
            memory.job_id,
            &format!("approved_default:{capability_id}"),
            observations.len() as i64 + 1,
            &approved_defaults,
            context.observed_at,
            &executable_constraint_contracts(),
        )?);
        let mut effective = merge_observations(
            memory.job_id,
            context.revision,
            &observations,
            &executable_constraint_contracts(),
        )?;
        effective.id = stable_uuid(memory.job_id, context.revision as u128 + 2);
        effective.created_at = context.observed_at;
        context
            .repository
            .insert_initial_state(&original, &observations, &effective)
            .await?
            .2
    } else {
        let existing = context.repository.list_observations(memory.job_id).await?;
        let first_sequence = existing.len() as i64 + 1;
        let contracts = executable_constraint_contracts();
        let mut observations = observations_from_patch(
            memory.job_id,
            &clarification_source_id,
            first_sequence,
            &structured_patch,
            context.observed_at,
            &contracts,
        )?;
        let patch_fields = structured_patch.keys().cloned().collect();
        observations.extend(deterministic_observations_excluding_fields(
            memory.job_id,
            &source_id,
            first_sequence + observations.len() as i64,
            FactSourceKind::Clarification,
            &extraction,
            &patch_fields,
            context.observed_at,
        ));
        observations.extend(approved_default_observations(
            memory.job_id,
            &format!("approved_default:{capability_id}"),
            first_sequence + observations.len() as i64,
            &approved_defaults,
            context.observed_at,
            &contracts,
        )?);
        observations.retain(|candidate| {
            !existing.iter().any(|existing| {
                existing.source_kind == candidate.source_kind
                    && existing.source_id == candidate.source_id
                    && existing.field_path == candidate.field_path
                    && existing.typed_value == candidate.typed_value
            })
        });
        context
            .repository
            .append_observations(memory.job_id, &observations)
            .await?;
        context
            .repository
            .derive_and_insert_effective(
                memory.job_id,
                context.revision,
                &executable_constraint_contracts(),
            )
            .await?
    };
    let original = context
        .repository
        .get_original_intent(memory.job_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("missing canonical original intent"))?;
    let snapshot = PlannerInputSnapshot {
        id: stable_uuid(memory.job_id, context.revision as u128 + 100),
        job_id: memory.job_id,
        revision: context.revision,
        original_intent_id: original.id,
        effective_constraints_id: effective.id,
        capability_catalog_version: catalog_version,
        principal_projection: PrincipalProjection {
            user_id: current_client.user_id,
            role: current_client.role.clone(),
            capability_ids: current_client.capability_ids.clone(),
            office_ids: current_client.office_ids.clone(),
            can_view_pii: current_client.can_view_pii,
            legacy_api_key_id: current_client.legacy_api_key_id,
        },
        reference_instant: context.reference_instant,
        timezone: context.timezone.clone(),
        selected_capability_id: capability_id.to_owned(),
        normalized_parameters: normalize_effective_parameters(catalog, capability_id, &effective)?,
        created_at: context.observed_at,
    };
    let snapshot_id = context
        .repository
        .insert_planner_snapshot(&snapshot)
        .await?
        .id;
    memory.planner_snapshot_id = Some(snapshot_id);
    let loaded = context
        .repository
        .get_planner_snapshot(snapshot_id, memory.job_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("missing planner snapshot"))?;
    anyhow::ensure!(
        loaded.job_id == memory.job_id
            && loaded.original_intent_id == original.id
            && loaded.effective_constraints_id == effective.id
            && loaded.capability_catalog_version == catalog_version,
        "mismatched planner snapshot"
    );
    let plan = plan_from_snapshot(catalog, &loaded)?;
    let principal = principal_from_snapshot(loaded.principal_projection);
    Ok(Some((plan, principal)))
}

pub(super) fn principal_from_snapshot(projection: PrincipalProjection) -> PrincipalContext {
    PrincipalContext {
        user_id: projection.user_id,
        role: projection.role,
        capability_ids: projection.capability_ids,
        office_ids: projection.office_ids,
        can_view_pii: projection.can_view_pii,
        legacy_api_key_id: projection.legacy_api_key_id,
    }
}
/// Builds a JSON audit trace of one retrieval pass for `state_json.retrieval_trace`.
/// Best-effort/debug-only shape — not part of the graph contract, so it is built
/// inline at the call site rather than added as a `JobMemory` field.
pub fn build_retrieval_trace(
    intent: &AssistantIntent,
    plan: &crate::assistant::evidence::RetrievalPlan,
    evidence: &[crate::assistant::evidence::Evidence],
    decision: &RerankerDecision,
) -> serde_json::Value {
    let candidates: Vec<_> = evidence
        .iter()
        .take(10)
        .map(|e| {
            json!({
                "capability_id": e.capability_id,
                "title": e.title,
                "score": e.score,
                "source_type": e.source_type,
            })
        })
        .collect();

    let kind = match decision.decision {
        RerankerVerdict::Select => "select",
        RerankerVerdict::Clarify => "clarify",
        RerankerVerdict::Unsupported => "unsupported",
        RerankerVerdict::FailedOperational => "failed_operational",
    };
    let decision_json = json!({
        "kind": kind,
        "capability_id": decision.capability_id,
        "confidence": decision.confidence,
        "alternatives": decision.alternatives,
        "reason": decision.reason,
    });

    json!({
        "router_intent": {
            "intent": intent.intent,
            "domain": intent.domain,
            "request_shape": intent.request_shape,
            "confidence": intent.confidence,
        },
        "plan": {
            "query_text": plan.query_text,
            "allowed_capability_count": plan.allowed_capabilities.len(),
            "allow_all_capabilities": plan.allow_all_capabilities,
        },
        "candidates": candidates,
        "decision": decision_json,
    })
}
