use super::*;

impl JobService {
    pub(super) async fn shadow_write(
        &self,
        memory: &mut JobMemory,
        client: &PrincipalContext,
        turn: CanonicalTurn,
        revision: i64,
    ) -> Result<()> {
        let source_id = turn.message_id.to_string();
        let extraction = memory
            .current_user_message_metadata
            .get("deterministic_extraction")
            .cloned()
            .and_then(|value| serde_json::from_value::<DeterministicExtraction>(value).ok())
            .unwrap_or_default();
        let effective = if turn.initial {
            if self
                .canonical_state
                .get_original_intent(memory.job_id)
                .await?
                .is_some()
            {
                self.canonical_state
                    .get_effective_constraints(memory.job_id, 0)
                    .await?
                    .ok_or_else(|| anyhow::anyhow!("missing canonical baseline"))?
            } else {
                let intent = memory
                    .intent
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("missing accepted initial parse"))?;
                let mut original = OriginalIntent {
                    id: stable_uuid(memory.job_id, 1),
                    job_id: memory.job_id,
                    schema_version: 1,
                    raw_message_id: turn.message_id,
                    locale: intent.language.clone(),
                    action: intent.intent.clone(),
                    entities: intent.entities.clone(),
                    metrics: intent.constraints.metric.clone().into_iter().collect(),
                    groupings: Vec::new(),
                    output: None,
                    parameters: BTreeMap::new(),
                    pii_request: false,
                    extraction_provenance: vec![ExtractionProvenance {
                        extractor: "semantic_router".into(),
                        version: "legacy_shadow_v1".into(),
                        source_identifiers: vec![source_id.clone()],
                        source_spans: Vec::new(),
                        rule: None,
                        reference_instant: None,
                        timezone: None,
                    }],
                    created_at: turn.reference_instant,
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
                    turn.observed_at,
                );
                let mut effective = merge_observations(
                    memory.job_id,
                    0,
                    &observations,
                    &executable_constraint_contracts(),
                )?;
                effective.id = stable_uuid(memory.job_id, 2);
                effective.created_at = turn.observed_at;
                self.canonical_state
                    .insert_initial_state(&original, &observations, &effective)
                    .await?
                    .2
            }
        } else {
            let first_sequence = self
                .canonical_state
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
                turn.observed_at,
            );
            self.canonical_state
                .append_observations(memory.job_id, &observations)
                .await?;
            self.canonical_state
                .derive_and_insert_effective(
                    memory.job_id,
                    revision,
                    &executable_constraint_contracts(),
                )
                .await?
        };
        self.shadow_snapshot(memory, client, turn.reference_instant, &effective)
            .await?;
        let canonical_hash = sanitized_hash(&effective.values);
        let legacy_hash = sanitized_hash(&memory.tool_params);
        tracing::info!(
            job_id = %memory.job_id,
            revision = effective.revision,
            decision_code = ?memory.terminal_state,
            selected_capability_id = memory.selected_capability.as_deref().unwrap_or("none"),
            field_count = effective.values.len(),
            field_names = ?effective.values.keys().collect::<Vec<_>>(),
            canonical_hash,
            legacy_hash,
            "canonical shadow comparison"
        );
        Ok(())
    }

    async fn shadow_snapshot(
        &self,
        memory: &mut JobMemory,
        client: &PrincipalContext,
        reference_instant: DateTime<Utc>,
        effective: &EffectiveConstraints,
    ) -> Result<()> {
        let (Some(capability), Some(catalog_version), Some(original)) = (
            memory.selected_capability.clone(),
            self.knowledge.latest_embedded_catalog().await?,
            self.canonical_state
                .get_original_intent(memory.job_id)
                .await?,
        ) else {
            return Ok(());
        };
        let snapshot = PlannerInputSnapshot {
            id: stable_uuid(memory.job_id, effective.revision as u128 + 100),
            job_id: memory.job_id,
            revision: effective.revision,
            original_intent_id: original.id,
            effective_constraints_id: effective.id,
            capability_catalog_version: catalog_version.id,
            principal_projection: PrincipalProjection {
                user_id: client.user_id,
                role: client.role.clone(),
                capability_ids: client.capability_ids.clone(),
                office_ids: client.office_ids.clone(),
                can_view_pii: client.can_view_pii,
                legacy_api_key_id: client.legacy_api_key_id,
            },
            reference_instant,
            timezone: "Asia/Jakarta".into(),
            selected_capability_id: capability,
            normalized_parameters: memory.tool_params.clone(),
            created_at: effective.created_at,
        };
        memory.planner_snapshot_id = Some(
            self.canonical_state
                .insert_planner_snapshot(&snapshot)
                .await?
                .id,
        );
        Ok(())
    }
}

fn sanitized_hash(value: &impl serde::Serialize) -> u64 {
    let mut hasher = DefaultHasher::new();
    serde_json::to_vec(value)
        .unwrap_or_default()
        .hash(&mut hasher);
    hasher.finish()
}
