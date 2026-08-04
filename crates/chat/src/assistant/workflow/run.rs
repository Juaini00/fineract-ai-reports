use std::collections::{BTreeMap, HashSet};
use std::time::Instant;

use anyhow::{Result, bail};
use app_core::auth::model::PrincipalContext;
use async_trait::async_trait;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::assistant::ClarificationPayload;
use crate::knowledge::model::KnowledgeCatalog;
use crate::policy::authorization::{
    effective_office_scope, ensure_capability_allowed, ensure_pii_allowed,
};

use super::contract::{
    EdgeCondition, ExecutionWorkflow, Idempotency, NodeId, NodeKind, WorkflowNode,
};
use super::state::{NodeRunStatus, WorkflowNodeRun, WorkflowStateRepository, persisted_output};

pub mod node;

#[derive(Debug, Clone, PartialEq)]
pub enum NodeExecution {
    Completed {
        output: Value,
        rows_returned: i32,
    },
    Waiting {
        clarification: Box<ClarificationPayload>,
    },
    Failed,
}

#[async_trait]
pub trait WorkflowNodeExecutor: Send + Sync {
    /// Implementations execute only approved catalog resources. The runner has
    /// already performed the per-node policy check and written a running row.
    async fn execute(
        &self,
        node: &WorkflowNode,
        bindings: &BTreeMap<String, Value>,
    ) -> Result<NodeExecution>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkflowRunOutcome {
    Completed,
    WaitingForUserInput { node_id: NodeId },
    Failed,
}

pub struct WorkflowRunner<E> {
    state: WorkflowStateRepository,
    executor: E,
    catalog: std::sync::Arc<KnowledgeCatalog>,
}

impl<E> WorkflowRunner<E>
where
    E: WorkflowNodeExecutor,
{
    pub fn new(
        state: WorkflowStateRepository,
        executor: E,
        catalog: std::sync::Arc<KnowledgeCatalog>,
    ) -> Self {
        Self {
            state,
            executor,
            catalog,
        }
    }

    /// Runs only dependency-ready nodes. Every node is authorized immediately
    /// before execution, rather than relying on a workflow-level preflight.
    pub async fn run(
        &self,
        job_id: Uuid,
        user_id: Uuid,
        principal: &PrincipalContext,
        workflow: &ExecutionWorkflow,
    ) -> Result<WorkflowRunOutcome> {
        let mut runs = self.state.node_runs(job_id, workflow.id).await?;
        loop {
            let completed = completed_ids(&runs);
            let Some(node) = workflow
                .nodes
                .iter()
                .find(|node| runnable(node, workflow, &runs, &completed))
            else {
                return Ok(
                    if workflow.nodes.iter().any(|node| {
                        matches!(node.kind, NodeKind::Complete(_)) && completed.contains(&node.id)
                    }) {
                        WorkflowRunOutcome::Completed
                    } else {
                        WorkflowRunOutcome::Failed
                    },
                );
            };
            if node.idempotency == Idempotency::ExecuteOnce
                && runs
                    .iter()
                    .any(|run| run.node_id == node.id && run.status == NodeRunStatus::Completed)
            {
                bail!("execute-once workflow node reached twice");
            }
            check_node_policy(node, principal, &self.catalog)?;
            let attempt = next_attempt(&runs, &node.id)?;
            let run = self
                .state
                .begin_node(
                    job_id,
                    workflow.id,
                    &node.id,
                    attempt,
                    json!({
                        "node_id": node.id, "policy_checked": true,
                    }),
                )
                .await?;
            let started = Instant::now();
            let bindings = bindings_for(node, workflow, &runs, principal)?;
            let execution = match &node.kind {
                NodeKind::CardinalityBranch(branch) => NodeExecution::Completed {
                    output: json!({ "cardinality": match node::branch::cardinality_for(&runs, &branch.source)? {
                        super::contract::Cardinality::Zero => "zero",
                        super::contract::Cardinality::One => "one",
                        super::contract::Cardinality::Many => "many",
                    } }),
                    rows_returned: 0,
                },
                NodeKind::Complete(_) => NodeExecution::Completed {
                    output: json!({}),
                    rows_returned: 0,
                },
                _ => self.executor.execute(node, &bindings).await?,
            };
            let elapsed = i32::try_from(started.elapsed().as_millis()).unwrap_or(i32::MAX);
            match execution {
                NodeExecution::Completed {
                    output,
                    rows_returned,
                } => {
                    let branch_decision = matches!(node.kind, NodeKind::CardinalityBranch(_))
                        .then(|| {
                            output
                                .get("cardinality")
                                .and_then(Value::as_str)
                                .map(str::to_owned)
                        })
                        .flatten();
                    let persisted = persisted_output(&node.inputs, output);
                    self.state
                        .complete_node(&run, persisted.clone(), rows_returned, elapsed)
                        .await?;
                    if let Some(cardinality) = branch_decision {
                        self.state
                            .record_branch_decision(job_id, workflow.id, &node.id, &cardinality)
                            .await?;
                    }
                    // `complete_node` above only updates the durable row; the
                    // in-memory `run` predates completion (from `begin_node`)
                    // and must be refreshed with the real output/row count too
                    // — later nodes in this same run (branch decisions,
                    // `PriorStep`/`AuthorizedDataProbe` bindings) read `runs`,
                    // not the database, to resolve a prior node's output.
                    runs.push(completed_run(run, elapsed, persisted, rows_returned));
                }
                NodeExecution::Waiting { mut clarification } => {
                    clarification.workflow_id = Some(workflow.id);
                    clarification.node_id = Some(node.id.as_str().to_owned());
                    clarification.entity_kind = match &node.kind {
                        NodeKind::ClarificationInterrupt(interrupt) => workflow
                            .nodes
                            .iter()
                            .find(|candidate| candidate.id == interrupt.option_source)
                            .and_then(|candidate| match &candidate.kind {
                                NodeKind::ResolveEntity(resolve) => {
                                    Some(resolve.entity_kind.clone())
                                }
                                _ => None,
                            }),
                        _ => None,
                    };
                    clarification.resume_node_id = match &node.kind {
                        NodeKind::ClarificationInterrupt(interrupt) => {
                            Some(interrupt.resume.as_str().to_owned())
                        }
                        _ => None,
                    };
                    self.state
                        .mark_workflow_paused(
                            job_id,
                            user_id,
                            workflow.id,
                            &node.id,
                            &clarification,
                        )
                        .await?;
                    return Ok(WorkflowRunOutcome::WaitingForUserInput {
                        node_id: node.id.clone(),
                    });
                }
                NodeExecution::Failed => {
                    self.state.fail_node(&run, elapsed).await?;
                    return Ok(WorkflowRunOutcome::Failed);
                }
            }
        }
    }
}

fn completed_run(
    mut run: WorkflowNodeRun,
    duration_ms: i32,
    output_json: Value,
    rows_returned: i32,
) -> WorkflowNodeRun {
    run.status = NodeRunStatus::Completed;
    run.duration_ms = Some(duration_ms);
    run.output_json = Some(output_json);
    run.rows_returned = rows_returned;
    run
}

fn completed_ids(runs: &[WorkflowNodeRun]) -> HashSet<NodeId> {
    runs.iter()
        .filter(|run| run.status == NodeRunStatus::Completed)
        .map(|run| run.node_id.clone())
        .collect()
}
fn next_attempt(runs: &[WorkflowNodeRun], node_id: &NodeId) -> Result<i16> {
    let attempt = runs
        .iter()
        .filter(|run| &run.node_id == node_id)
        .map(|run| run.attempt)
        .max()
        .map_or(0, |attempt| attempt + 1);
    Ok(attempt)
}

fn runnable(
    node: &WorkflowNode,
    workflow: &ExecutionWorkflow,
    runs: &[WorkflowNodeRun],
    completed: &HashSet<NodeId>,
) -> bool {
    if completed.contains(&node.id)
        || runs.iter().any(|run| {
            run.node_id == node.id
                && matches!(run.status, NodeRunStatus::Running | NodeRunStatus::Waiting)
        })
    {
        return false;
    }
    let incoming = workflow
        .edges
        .iter()
        .filter(|edge| edge.to == node.id)
        .collect::<Vec<_>>();
    // A node with no incoming edges is an entry point (mirrors
    // `WorkflowGraph::entry_nodes`, which `verify()` uses the same way) and is
    // runnable immediately, regardless of whether it also has outgoing edges
    // — an entry node driving the rest of the graph is the common case, not
    // an exception.
    incoming.is_empty()
        || incoming.iter().any(|edge| {
            completed.contains(&edge.from)
                && edge_condition_matches(&edge.condition, &edge.from, runs)
        })
}

fn edge_condition_matches(
    condition: &EdgeCondition,
    source: &NodeId,
    runs: &[WorkflowNodeRun],
) -> bool {
    match condition {
        EdgeCondition::Always | EdgeCondition::ClarificationAnswered => true,
        EdgeCondition::Cardinality(expected) => {
            node::branch::cardinality_for(runs, source).is_ok_and(|actual| &actual == expected)
        }
    }
}

fn check_node_policy(
    node: &WorkflowNode,
    principal: &PrincipalContext,
    catalog: &KnowledgeCatalog,
) -> Result<()> {
    if let Some(capability) = node.policy.required_capability.as_deref() {
        if !catalog
            .capabilities
            .iter()
            .any(|item| item.id == capability && item.status == "approved_mvp")
        {
            bail!("workflow node capability is unavailable");
        }
        ensure_capability_allowed(principal, capability)?;
    }
    if let NodeKind::ExecuteQuery(query) = &node.kind
        && let Some(capability) = query.capability_id.as_deref()
    {
        if !catalog
            .capabilities
            .iter()
            .any(|item| item.id == capability && item.status == "approved_mvp")
        {
            bail!("workflow node capability is unavailable");
        }
        ensure_capability_allowed(principal, capability)?;
    }
    ensure_pii_allowed(principal, node.policy.pii_required)?;
    effective_office_scope(principal, None)?;
    Ok(())
}

fn bindings_for(
    node: &WorkflowNode,
    _workflow: &ExecutionWorkflow,
    runs: &[WorkflowNodeRun],
    principal: &PrincipalContext,
) -> Result<BTreeMap<String, Value>> {
    let mut bindings = BTreeMap::new();
    for input in &node.inputs {
        let value = match &input.source {
            super::contract::BindingSource::AuthorizedScope => {
                json!(effective_office_scope(principal, None)?)
            }
            super::contract::BindingSource::PriorStep { node, slot }
            | super::contract::BindingSource::AuthorizedDataProbe { node, slot } => {
                output_slot(runs, node, slot)?
            }
            // Catalog defaults, extractions, and user answers are preserved in
            // workflow state by the compiler/resume path; a runner never makes
            // up an untrusted value for them.
            super::contract::BindingSource::ExactSensitiveInput => Value::Null,
            _ => Value::Null,
        };
        bindings.insert(input.parameter.clone(), value);
    }
    Ok(bindings)
}
fn output_slot(runs: &[WorkflowNodeRun], node: &NodeId, slot: &str) -> Result<Value> {
    runs.iter()
        .rev()
        .find(|run| &run.node_id == node && run.status == NodeRunStatus::Completed)
        .and_then(|run| run.output_json.as_ref())
        .and_then(|output| output.get(slot))
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("completed workflow output slot is missing"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assistant::workflow::contract::{Cardinality, NodeBudget, NodePolicy, RetryPolicy};
    use crate::knowledge::model::Sensitivity;

    fn node(id: &str) -> WorkflowNode {
        WorkflowNode {
            id: NodeId::new(id).unwrap(),
            kind: NodeKind::Complete(super::super::contract::CompleteNode {
                terminal: super::super::contract::TerminalState::Success,
            }),
            inputs: vec![],
            outputs: vec![],
            policy: NodePolicy {
                required_capability: None,
                office_scope: super::super::contract::OfficeScope::AuthorizedIntersection,
                max_sensitivity: Sensitivity::Pii,
                pii_required: false,
            },
            budget: NodeBudget {
                timeout_ms: 0,
                row_cap: 0,
                query_cost: 0,
            },
            idempotency: Idempotency::Pure,
            retry: RetryPolicy { max_attempts: 0 },
        }
    }
    #[test]
    fn branch_only_schedules_matching_arm() {
        let start = node("start");
        let zero = node("zero");
        let many = node("many");
        let workflow = ExecutionWorkflow {
            id: Uuid::nil(),
            contract_version: 1,
            catalog_version: Uuid::nil(),
            nodes: vec![start, zero.clone(), many.clone()],
            edges: vec![
                super::super::contract::WorkflowEdge {
                    from: NodeId::new("start").unwrap(),
                    to: zero.id.clone(),
                    condition: EdgeCondition::Cardinality(Cardinality::Zero),
                },
                super::super::contract::WorkflowEdge {
                    from: NodeId::new("start").unwrap(),
                    to: many.id.clone(),
                    condition: EdgeCondition::Cardinality(Cardinality::Many),
                },
            ],
            budgets: super::super::contract::WorkflowBudgets {
                shared_timeout_ms: 1,
                shared_row_cap: 1,
                max_query_count: 1,
                max_parallel_queries: 1,
                max_model_turns: 1,
                max_node_retries: 0,
            },
            fail_policy: super::super::contract::FailPolicy::FailFast,
            output_contract: super::super::contract::OutputContract {
                mode: super::super::contract::OutputMode::Table,
                allows_partial: false,
                max_sensitivity: Sensitivity::Pii,
            },
        };
        let run = WorkflowNodeRun {
            id: Uuid::nil(),
            job_id: Uuid::nil(),
            workflow_id: Uuid::nil(),
            node_id: NodeId::new("start").unwrap(),
            attempt: 0,
            status: NodeRunStatus::Completed,
            output_json: Some(json!({"row_count": 0})),
            provenance_json: json!({}),
            rows_returned: 0,
            duration_ms: Some(0),
            started_at: None,
            finished_at: None,
        };
        let completed = completed_ids(std::slice::from_ref(&run));
        assert!(runnable(
            &zero,
            &workflow,
            std::slice::from_ref(&run),
            &completed
        ));
        assert!(!runnable(&many, &workflow, &[run], &completed));
    }
}
