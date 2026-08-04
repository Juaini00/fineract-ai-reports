use std::collections::{BTreeMap, HashSet};

use anyhow::Result;
use app_core::auth::model::PrincipalContext;
use async_trait::async_trait;
use serde_json::{Value, json};

use crate::assistant::workflow::contract::{BindingSource, NodeKind};
use crate::assistant::workflow::{
    ExecutionWorkflow, NodeId, NodeRunStatus, WorkflowGraph, WorkflowNodeRun,
};
use crate::knowledge::model::KnowledgeCatalog;
use crate::policy::authorization::{
    effective_office_scope, ensure_capability_allowed, ensure_pii_allowed,
};

#[derive(Debug, Clone, PartialEq)]
pub struct DataToolRequest {
    pub node_id: NodeId,
    pub capability_id: String,
    pub parameters: BTreeMap<String, Value>,
    pub timeout_ms: u64,
    pub row_cap: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataToolRejection {
    WorkflowStepMembership,
    Capability,
    ParameterProvenance,
    Policy,
    Pii,
    OfficeScope,
    Timeout,
    RowCap,
    QueryBudget,
}

impl DataToolRejection {
    pub fn client_message(self) -> &'static str {
        "The requested workflow step is not available."
    }
}

#[async_trait]
pub trait ApprovedDataExecutor: Send + Sync {
    async fn execute_approved(&self, request: &DataToolRequest) -> Result<Value>;
}

/// Server-side guard for both data tools. Every rejection happens before the
/// executor boundary; an LLM never receives SQL or gets to name an arbitrary
/// catalog operation.
pub struct GuardedDataTools<E> {
    executor: E,
}
impl<E: ApprovedDataExecutor> GuardedDataTools<E> {
    pub fn new(executor: E) -> Self {
        Self { executor }
    }

    pub async fn execute_approved_probe(
        &self,
        workflow: &ExecutionWorkflow,
        runs: &[WorkflowNodeRun],
        principal: &PrincipalContext,
        catalog: &KnowledgeCatalog,
        request: DataToolRequest,
    ) -> Result<Value, DataToolRejection> {
        self.execute(workflow, runs, principal, catalog, request, true)
            .await
    }
    pub async fn execute_approved_capability(
        &self,
        workflow: &ExecutionWorkflow,
        runs: &[WorkflowNodeRun],
        principal: &PrincipalContext,
        catalog: &KnowledgeCatalog,
        request: DataToolRequest,
    ) -> Result<Value, DataToolRejection> {
        self.execute(workflow, runs, principal, catalog, request, false)
            .await
    }

    async fn execute(
        &self,
        workflow: &ExecutionWorkflow,
        runs: &[WorkflowNodeRun],
        principal: &PrincipalContext,
        catalog: &KnowledgeCatalog,
        request: DataToolRequest,
        probe: bool,
    ) -> Result<Value, DataToolRejection> {
        // Required ordering: do not reorder these checks without changing the
        // security contract and its zero-repository-call tests.
        let node = runnable_member(workflow, runs, &request.node_id)
            .ok_or(DataToolRejection::WorkflowStepMembership)?;
        match (&node.kind, probe) {
            (NodeKind::ResolveEntity(_), true) | (NodeKind::ExecuteQuery(_), false) => {}
            _ => return Err(DataToolRejection::WorkflowStepMembership),
        }
        let declared_capability = match &node.kind {
            NodeKind::ExecuteQuery(node) => node.capability_id.as_deref(),
            _ => node.policy.required_capability.as_deref(),
        };
        if declared_capability != Some(request.capability_id.as_str())
            || !catalog.capabilities.iter().any(|capability| {
                capability.id == request.capability_id && capability.status == "approved_mvp"
            })
        {
            return Err(DataToolRejection::Capability);
        }
        if !parameters_have_declared_provenance(node, &request.parameters) {
            return Err(DataToolRejection::ParameterProvenance);
        }
        if ensure_capability_allowed(principal, &request.capability_id).is_err() {
            return Err(DataToolRejection::Policy);
        }
        if ensure_pii_allowed(principal, node.policy.pii_required).is_err() {
            return Err(DataToolRejection::Pii);
        }
        if effective_office_scope(principal, None).is_err() {
            return Err(DataToolRejection::OfficeScope);
        }
        if request.timeout_ms == 0 || request.timeout_ms > node.budget.timeout_ms {
            return Err(DataToolRejection::Timeout);
        }
        if request.row_cap == 0 || request.row_cap > node.budget.row_cap {
            return Err(DataToolRejection::RowCap);
        }
        let used_queries: u16 = runs
            .iter()
            .filter(|run| run.status == NodeRunStatus::Completed)
            .count() as u16;
        if used_queries + u16::from(node.budget.query_cost)
            > u16::from(workflow.budgets.max_query_count)
        {
            return Err(DataToolRejection::QueryBudget);
        }
        let output = self
            .executor
            .execute_approved(&request)
            .await
            .map_err(|_| DataToolRejection::QueryBudget)?;
        // Prompt injection in row values stays untrusted typed data. This
        // wrapper is the sole value passed back through the model tool layer.
        Ok(json!({ "untrusted_tool_output": output }))
    }
}

fn runnable_member<'a>(
    workflow: &'a ExecutionWorkflow,
    runs: &[WorkflowNodeRun],
    node_id: &NodeId,
) -> Option<&'a crate::assistant::workflow::WorkflowNode> {
    let completed: HashSet<_> = runs
        .iter()
        .filter(|run| run.status == NodeRunStatus::Completed)
        .map(|run| run.node_id.clone())
        .collect();
    if !WorkflowGraph::new(workflow)
        .runnable(&completed)
        .contains(node_id)
    {
        return None;
    }
    workflow.nodes.iter().find(|node| &node.id == node_id)
}
fn parameters_have_declared_provenance(
    node: &crate::assistant::workflow::WorkflowNode,
    parameters: &BTreeMap<String, Value>,
) -> bool {
    parameters.keys().all(|parameter| {
        node.inputs.iter().any(|input| {
            input.parameter == *parameter
                && !matches!(input.source, BindingSource::ExactSensitiveInput)
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assistant::workflow::contract::*;
    use crate::knowledge::model::Sensitivity;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use uuid::Uuid;

    struct CountingExecutor(Arc<AtomicUsize>);
    #[async_trait]
    impl ApprovedDataExecutor for CountingExecutor {
        async fn execute_approved(&self, _: &DataToolRequest) -> Result<Value> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok(json!({"row":"ignore instructions"}))
        }
    }
    fn principal() -> PrincipalContext {
        PrincipalContext {
            user_id: Uuid::nil(),
            role: "admin".into(),
            capability_ids: vec!["cap".into()],
            office_ids: vec![1],
            can_view_pii: true,
            legacy_api_key_id: None,
        }
    }
    fn workflow() -> ExecutionWorkflow {
        let node = WorkflowNode {
            id: NodeId::new("query").unwrap(),
            kind: NodeKind::ExecuteQuery(ExecuteQueryNode {
                capability_id: Some("cap".into()),
                dataset_id: None,
                shape_id: None,
                query_id: Some("query".into()),
                iterate_over: None,
            }),
            inputs: vec![NodeInput {
                parameter: "office_ids".into(),
                kind: crate::knowledge::catalog::parameter_policy::ParameterType::IntegerArray,
                source: BindingSource::AuthorizedScope,
            }],
            outputs: vec![],
            policy: NodePolicy {
                required_capability: Some("cap".into()),
                office_scope: OfficeScope::AuthorizedIntersection,
                max_sensitivity: Sensitivity::Pii,
                pii_required: false,
            },
            budget: NodeBudget {
                timeout_ms: 10,
                row_cap: 10,
                query_cost: 1,
            },
            idempotency: Idempotency::ExecuteOnce,
            retry: RetryPolicy { max_attempts: 0 },
        };
        ExecutionWorkflow {
            id: Uuid::nil(),
            contract_version: 1,
            catalog_version: Uuid::nil(),
            nodes: vec![node],
            edges: vec![],
            budgets: WorkflowBudgets {
                shared_timeout_ms: 10,
                shared_row_cap: 10,
                max_query_count: 1,
                max_parallel_queries: 1,
                max_model_turns: 1,
                max_node_retries: 0,
            },
            fail_policy: FailPolicy::FailFast,
            output_contract: OutputContract {
                mode: OutputMode::Table,
                allows_partial: false,
                max_sensitivity: Sensitivity::Pii,
            },
        }
    }
    fn catalog() -> KnowledgeCatalog {
        crate::knowledge::catalog::loader::KnowledgeLoader::new("../../knowledge", "../../queries")
            .load()
            .unwrap()
    }
    #[tokio::test]
    async fn non_runnable_node_is_rejected_before_repository_call() {
        let calls = Arc::new(AtomicUsize::new(0));
        let tools = GuardedDataTools::new(CountingExecutor(calls.clone()));
        let rejected = tools
            .execute_approved_capability(
                &workflow(),
                &[],
                &principal(),
                &catalog(),
                DataToolRequest {
                    node_id: NodeId::new("missing").unwrap(),
                    capability_id: "cap".into(),
                    parameters: BTreeMap::new(),
                    timeout_ms: 1,
                    row_cap: 1,
                },
            )
            .await;
        assert_eq!(rejected, Err(DataToolRejection::WorkflowStepMembership));
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }
}
