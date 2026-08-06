use std::sync::Arc;

use anyhow::{Context, Result};
use async_trait::async_trait;
use serde_json::{Value, json};
use sqlx::PgPool;

use crate::assistant::execution::plan::{ExecutionPlan, ExecutionPlanType, PolicyDecision};
use crate::assistant::llm::tool::data::{ApprovedDataExecutor, DataToolRequest};
use crate::assistant::understanding::extraction::SensitiveIdentifier;
use crate::execution::repository::{ExecutionLimits, execute_plan_with_sensitive};
use crate::knowledge::model::KnowledgeCatalog;

/// Concrete `ApprovedDataExecutor` that runs a `DataToolRequest` through the
/// same approved-SQL execution path (`execute_plan_with_sensitive`) already
/// used by the legacy semantic pipeline.
pub struct FineractDataExecutor {
    pool: PgPool,
    catalog: Arc<KnowledgeCatalog>,
    policy: PolicyDecision,
    limits: ExecutionLimits,
    sensitive_identifier: Option<SensitiveIdentifier>,
}

impl FineractDataExecutor {
    // pub(crate), not pub: `SensitiveIdentifier` is pub(crate) in
    // `assistant::understanding::extraction`, so a `pub fn` taking it by
    // value would leak a more-private type through a public interface
    // (rustc E0446 / clippy deny).
    // ponytail: unused until Task 5 wires this into execute_selected_capability.
    #[allow(dead_code)]
    pub(crate) fn new(
        pool: PgPool,
        catalog: Arc<KnowledgeCatalog>,
        policy: PolicyDecision,
        limits: ExecutionLimits,
        sensitive_identifier: Option<SensitiveIdentifier>,
    ) -> Self {
        Self {
            pool,
            catalog,
            policy,
            limits,
            sensitive_identifier,
        }
    }
}

/// Builds a minimal single-capability `ExecutionPlan` from a guarded
/// `DataToolRequest`. Kept as a free function so it can be unit-tested
/// without a live database pool.
pub(super) fn build_execution_plan(
    request: &DataToolRequest,
    catalog: &KnowledgeCatalog,
) -> Result<ExecutionPlan> {
    let capability = catalog
        .capabilities
        .iter()
        .find(|capability| capability.id == request.capability_id)
        .with_context(|| format!("capability {} not found in catalog", request.capability_id))?;

    Ok(ExecutionPlan {
        plan_type: ExecutionPlanType::Atomic,
        domain: capability.domain.clone(),
        capability: request.capability_id.clone(),
        query_id: capability.query_id.clone(),
        dataset_selection: None,
        output_mode: capability.output_mode.clone(),
        params: json!(request.parameters),
        retrieval_plan: Default::default(),
        evidence_evaluation: Default::default(),
        requires_policy_check: true,
    })
}

#[async_trait]
impl ApprovedDataExecutor for FineractDataExecutor {
    async fn execute_approved(&self, request: &DataToolRequest) -> Result<Value> {
        let plan = build_execution_plan(request, &self.catalog)?;
        execute_plan_with_sensitive(
            &self.pool,
            &self.catalog,
            &plan,
            &self.policy,
            self.limits,
            self.sensitive_identifier.as_ref(),
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assistant::llm::tool::data::DataToolRequest;

    #[test]
    fn builds_execution_plan_from_request_params() {
        let request = DataToolRequest {
            node_id: crate::assistant::workflow::NodeId::new("n1").unwrap(),
            capability_id: "client_name_lookup".into(),
            parameters: std::collections::BTreeMap::from([(
                "person_name".into(),
                serde_json::json!("Alex"),
            )]),
            timeout_ms: 5_000,
            row_cap: 50,
        };
        let catalog = catalog();
        let plan = super::build_execution_plan(&request, &catalog)
            .expect("client_name_lookup is in the sample catalog");
        assert_eq!(plan.capability, "client_name_lookup");
        assert_eq!(plan.params["person_name"], serde_json::json!("Alex"));
    }

    /// Same fixture pattern as `assistant::execution::plan::tests::catalog`:
    /// load the real `knowledge/`/`queries/` trees from the workspace root.
    fn catalog() -> KnowledgeCatalog {
        let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let workspace_root = manifest_dir
            .parent()
            .and_then(|path| path.parent())
            .unwrap();
        crate::knowledge::catalog::loader::KnowledgeLoader::new(
            workspace_root.join("knowledge"),
            workspace_root.join("queries"),
        )
        .load()
        .unwrap()
    }
}
