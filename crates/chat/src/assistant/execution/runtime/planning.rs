use super::*;

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
        let observations = original_request_observations(
            memory.job_id,
            &source_id,
            intent,
            &extraction,
            context.observed_at,
        );
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
        let first_sequence = context
            .repository
            .list_observations(memory.job_id)
            .await?
            .len() as i64
            + 1;
        let observations = deterministic_observations(
            memory.job_id,
            &source_id,
            first_sequence,
            FactSourceKind::Clarification,
            &extraction,
            context.observed_at,
        );
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
