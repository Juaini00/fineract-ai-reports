use anyhow::{Result, bail};

use crate::{
    assistant::{
        AssistantIntent, DeterministicExtraction, PlannerInputSnapshot,
        execution::plan::{EvidenceEvaluation, ExecutionPlan, ExecutionPlanType, RetrievalPlan},
    },
    knowledge::{catalog::parameter_policy::EvaluationContext, model::KnowledgeCatalog},
};

use super::parameters::{
    executable_capability, params_from_verified, validate_snapshot_parameters,
    verify_capability_metric,
};

pub(super) fn plan_selected_capability(
    catalog: &KnowledgeCatalog,
    capability_id: &str,
    intent: &AssistantIntent,
) -> Result<ExecutionPlan> {
    let legacy_extraction = DeterministicExtraction {
        entities: intent.entities.clone(),
        ..Default::default()
    };
    plan_selected_capability_verified(
        catalog,
        capability_id,
        intent,
        Some(&legacy_extraction),
        None,
    )
}

pub(super) fn plan_selected_capability_verified(
    catalog: &KnowledgeCatalog,
    capability_id: &str,
    intent: &AssistantIntent,
    deterministic_extraction: Option<&DeterministicExtraction>,
    ctx: Option<&EvaluationContext>,
) -> Result<ExecutionPlan> {
    if let Some(error) = deterministic_extraction.and_then(|value| value.temporal_error.as_ref()) {
        bail!("{}: {}", error.code, error.message);
    }
    let capability = catalog
        .capabilities
        .iter()
        .find(|item| item.id == capability_id && item.status == "approved_mvp")
        .ok_or_else(|| anyhow::anyhow!("selected capability is not executable"))?;
    verify_capability_metric(capability.metrics.as_slice(), deterministic_extraction)?;
    let query = catalog
        .queries
        .iter()
        .find(|item| item.id == capability.query_id)
        .ok_or_else(|| anyhow::anyhow!("selected capability has no approved query"))?;
    let params = params_from_verified(
        query,
        intent,
        deterministic_extraction,
        &capability.parameter_policies,
        ctx,
    )?;
    let dataset_selection = capability
        .dataset_recipe
        .as_ref()
        .map(|recipe| {
            let dataset = catalog
                .datasets
                .iter()
                .find(|dataset| dataset.id == recipe.dataset_id)
                .ok_or_else(|| anyhow::anyhow!("selected capability has no approved dataset"))?;
            crate::knowledge::dataset::resolve::resolve_recipe(dataset, recipe, &params)
        })
        .transpose()?;

    Ok(ExecutionPlan {
        plan_type: ExecutionPlanType::Atomic,
        domain: capability.domain.clone(),
        capability: capability.id.clone(),
        query_id: query.id.clone(),
        dataset_selection,
        output_mode: capability.output_mode.clone(),
        params,
        retrieval_plan: RetrievalPlan {
            vector_query: intent.reason.clone(),
            keyword_query: intent
                .entities
                .iter()
                .map(|entity| entity.value.as_str())
                .collect::<Vec<_>>()
                .join(" "),
            graph_query: format!("{} -> {}", capability.id, query.id),
            metadata_filter: [("capability".into(), capability.id.clone())].into(),
        },
        evidence_evaluation: EvidenceEvaluation {
            enough: true,
            source_count: 1,
            source_types: vec!["capability".into()],
            reason: None,
        },
        requires_policy_check: true,
    })
}

pub(super) fn plan_from_snapshot(
    catalog: &KnowledgeCatalog,
    snapshot: &PlannerInputSnapshot,
) -> Result<ExecutionPlan> {
    let capability = executable_capability(catalog, &snapshot.selected_capability_id)?;
    let query = catalog
        .queries
        .iter()
        .find(|item| item.id == capability.query_id)
        .ok_or_else(|| anyhow::anyhow!("selected capability has no approved query"))?;
    validate_snapshot_parameters(query, &snapshot.normalized_parameters)?;
    let dataset_selection = capability
        .dataset_recipe
        .as_ref()
        .map(|recipe| {
            let dataset = catalog
                .datasets
                .iter()
                .find(|dataset| dataset.id == recipe.dataset_id)
                .ok_or_else(|| anyhow::anyhow!("selected capability has no approved dataset"))?;
            crate::knowledge::dataset::resolve::resolve_recipe(
                dataset,
                recipe,
                &snapshot.normalized_parameters,
            )
        })
        .transpose()?;
    Ok(ExecutionPlan {
        plan_type: ExecutionPlanType::Atomic,
        domain: capability.domain.clone(),
        capability: capability.id.clone(),
        query_id: query.id.clone(),
        dataset_selection,
        output_mode: capability.output_mode.clone(),
        params: snapshot.normalized_parameters.clone(),
        retrieval_plan: RetrievalPlan::default(),
        evidence_evaluation: EvidenceEvaluation {
            enough: true,
            source_count: 1,
            source_types: vec!["planner_input_snapshot".into()],
            reason: None,
        },
        requires_policy_check: true,
    })
}
