use std::collections::BTreeMap;

use uuid::Uuid;

use crate::assistant::context::canonical_state::ConstraintField;
use crate::knowledge::{
    catalog::parameter_policy::{ParameterPolicy, ParameterType, ResolutionStrategy},
    dataset::model::{FilterOperator, ShapeRole},
    model::{CapabilityKnowledge, KnowledgeCatalog, Sensitivity},
};

use super::contract::*;

#[derive(Debug, Clone, Default)]
pub struct AcquisitionFacts {
    pub deterministic: Vec<ConstraintField>,
    pub verified_user: Vec<ConstraintField>,
    pub safe_prior: BTreeMap<String, NodeId>,
    pub prior_steps: BTreeMap<String, (NodeId, String)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AmbiguityOutcome {
    Select {
        capability_id: String,
        confidence_overridden: bool,
    },
    Probe {
        dataset_id: String,
        shape_id: String,
    },
    Clarify {
        options: Vec<(String, String)>,
    },
    Unsupported,
}

/// Applies the clarification gate after retrieval. Confidence is intentionally absent:
/// catalog compatibility, deterministic facts, and bounded resolver probes decide whether
/// a human question is warranted.
pub fn resolve_ambiguity(
    candidate_ids: &[String],
    facts: &AcquisitionFacts,
    catalog: &KnowledgeCatalog,
) -> AmbiguityOutcome {
    let mut candidates = candidate_ids
        .iter()
        .filter_map(|id| {
            catalog
                .capabilities
                .iter()
                .find(|capability| capability.id == *id && capability.status == "approved_mvp")
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| left.id.cmp(&right.id));
    candidates.dedup_by(|left, right| left.id == right.id);
    if candidates.len() == 1 {
        return AmbiguityOutcome::Select {
            capability_id: candidates[0].id.clone(),
            confidence_overridden: true,
        };
    }
    let fact_filtered = candidates
        .iter()
        .copied()
        .filter(|capability| {
            capability.parameter_policies.iter().all(|policy| {
                !policy.required
                    || constraint_for(&policy.name).is_none_or(|field| {
                        facts.deterministic.contains(&field) || facts.verified_user.contains(&field)
                    })
            })
        })
        .collect::<Vec<_>>();
    if fact_filtered.len() == 1 {
        return AmbiguityOutcome::Select {
            capability_id: fact_filtered[0].id.clone(),
            confidence_overridden: true,
        };
    }
    if candidates.len() > 1
        && let Some((dataset_id, shape_id)) = catalog.datasets.iter().find_map(|dataset| {
            dataset.shapes.iter().find_map(|shape| {
                matches!(shape.role, ShapeRole::Resolver | ShapeRole::Probe)
                    .then(|| (dataset.id.clone(), shape.id.clone()))
            })
        })
    {
        return AmbiguityOutcome::Probe {
            dataset_id,
            shape_id,
        };
    }
    match candidates.len() {
        0 => AmbiguityOutcome::Unsupported,
        _ => AmbiguityOutcome::Clarify {
            options: candidates
                .into_iter()
                .map(|capability| {
                    (
                        capability.id.clone(),
                        capability
                            .display_name
                            .clone()
                            .unwrap_or_else(|| capability.id.clone()),
                    )
                })
                .collect(),
        },
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompileError {
    UnknownCapability(String),
    Unsupported,
    GroupedQueryPreferred(String),
    BudgetExceeded,
    InvalidProposal,
}
impl std::fmt::Display for CompileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownCapability(_) => f.write_str("requested capability is unavailable"),
            Self::Unsupported => f.write_str("request is unsupported"),
            Self::GroupedQueryPreferred(id) => {
                write!(f, "grouped shape {id} must be used instead of iteration")
            }
            Self::BudgetExceeded => f.write_str("workflow exceeds configured budget"),
            Self::InvalidProposal => f.write_str("workflow proposal is invalid"),
        }
    }
}
impl std::error::Error for CompileError {}

pub fn compile(
    proposal: WorkflowProposal,
    catalog: &KnowledgeCatalog,
    catalog_version: Uuid,
    budgets: WorkflowBudgets,
) -> Result<ExecutionWorkflow, CompileError> {
    compile_with_facts(
        proposal,
        catalog,
        catalog_version,
        budgets,
        &AcquisitionFacts::default(),
    )
}
pub fn compile_with_facts(
    proposal: WorkflowProposal,
    catalog: &KnowledgeCatalog,
    catalog_version: Uuid,
    budgets: WorkflowBudgets,
    facts: &AcquisitionFacts,
) -> Result<ExecutionWorkflow, CompileError> {
    if !proposal.nodes.is_empty() {
        resolve_proposal_resources(&proposal, catalog)?;
        let workflow = ExecutionWorkflow {
            id: Uuid::new_v4(),
            contract_version: WORKFLOW_CONTRACT_VERSION,
            catalog_version,
            nodes: proposal.nodes,
            edges: proposal.edges,
            budgets,
            fail_policy: FailPolicy::FailFast,
            output_contract: OutputContract {
                mode: OutputMode::Table,
                allows_partial: false,
                max_sensitivity: Sensitivity::Pii,
            },
        };
        return expand_iteration(workflow, catalog);
    }
    for capability_id in &proposal.capability_ids {
        approved_capability(catalog, capability_id)?;
    }
    let capability_id = proposal
        .capability_ids
        .first()
        .ok_or(CompileError::Unsupported)?;
    let capability = catalog
        .capabilities
        .iter()
        .find(|capability| capability.id == *capability_id && capability.status == "approved_mvp")
        .ok_or_else(|| CompileError::UnknownCapability(capability_id.clone()))?;
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    let execute_id = NodeId::new("execute_query").expect("constant node id");
    let inputs = acquisition_inputs(
        capability,
        catalog,
        facts,
        &mut nodes,
        &mut edges,
        &execute_id,
    )?;
    nodes.push(WorkflowNode {
        id: execute_id.clone(),
        kind: NodeKind::ExecuteQuery(ExecuteQueryNode {
            capability_id: Some(capability.id.clone()),
            dataset_id: capability
                .dataset_recipe
                .as_ref()
                .map(|recipe| recipe.dataset_id.clone()),
            shape_id: capability
                .dataset_recipe
                .as_ref()
                .map(|recipe| recipe.shape_id.clone()),
            query_id: Some(capability.query_id.clone()),
            iterate_over: None,
        }),
        inputs,
        outputs: vec![],
        policy: policy(Some(capability.id.clone())),
        budget: NodeBudget {
            timeout_ms: catalog
                .queries
                .iter()
                .find(|query| query.id == capability.query_id)
                .and_then(|query| query.timeout_ms)
                .unwrap_or(5_000),
            row_cap: capability
                .guards
                .max_limit
                .unwrap_or(100)
                .try_into()
                .unwrap_or(100),
            query_cost: 1,
        },
        idempotency: Idempotency::ExecuteOnce,
        retry: RetryPolicy {
            max_attempts: budgets.max_node_retries,
        },
    });
    let complete_id = NodeId::new("complete").expect("constant node id");
    nodes.push(WorkflowNode {
        id: complete_id.clone(),
        kind: NodeKind::Complete(CompleteNode {
            terminal: TerminalState::Success,
        }),
        inputs: vec![],
        outputs: vec![],
        policy: policy(None),
        budget: NodeBudget {
            timeout_ms: 0,
            row_cap: 0,
            query_cost: 0,
        },
        idempotency: Idempotency::Pure,
        retry: RetryPolicy { max_attempts: 0 },
    });
    edges.push(WorkflowEdge {
        from: execute_id,
        to: complete_id,
        condition: EdgeCondition::Always,
    });
    expand_iteration(
        ExecutionWorkflow {
            id: Uuid::new_v4(),
            contract_version: WORKFLOW_CONTRACT_VERSION,
            catalog_version,
            nodes,
            edges,
            budgets,
            fail_policy: FailPolicy::FailFast,
            output_contract: OutputContract {
                mode: output_mode(&capability.output_mode),
                allows_partial: false,
                max_sensitivity: Sensitivity::Pii,
            },
        },
        catalog,
    )
}

fn acquisition_inputs(
    capability: &CapabilityKnowledge,
    catalog: &KnowledgeCatalog,
    facts: &AcquisitionFacts,
    nodes: &mut Vec<WorkflowNode>,
    edges: &mut Vec<WorkflowEdge>,
    execute: &NodeId,
) -> Result<Vec<NodeInput>, CompileError> {
    let mut inputs = vec![NodeInput {
        parameter: "office_ids".into(),
        kind: ParameterType::IntegerArray,
        source: BindingSource::AuthorizedScope,
    }];
    for policy_item in capability
        .parameter_policies
        .iter()
        .filter(|policy| policy.required && policy.name != "office_ids")
    {
        let source = acquisition_source(policy_item, facts, nodes, edges, execute, catalog)?;
        inputs.push(NodeInput {
            parameter: policy_item.name.clone(),
            kind: policy_item.kind,
            source,
        });
    }
    Ok(inputs)
}
fn acquisition_source(
    parameter_policy: &ParameterPolicy,
    facts: &AcquisitionFacts,
    nodes: &mut Vec<WorkflowNode>,
    edges: &mut Vec<WorkflowEdge>,
    execute: &NodeId,
    catalog: &KnowledgeCatalog,
) -> Result<BindingSource, CompileError> {
    let strategies = if parameter_policy.resolution.is_empty() {
        vec![
            ResolutionStrategy::AuthorizedScope,
            ResolutionStrategy::CatalogDefault,
            ResolutionStrategy::DeterministicExtraction,
            ResolutionStrategy::VerifiedUserText,
            ResolutionStrategy::SafePriorSelection,
            ResolutionStrategy::PriorStep,
            ResolutionStrategy::AuthorizedDataProbe,
            ResolutionStrategy::Clarify,
        ]
    } else {
        parameter_policy.resolution.clone()
    };
    for strategy in strategies {
        match strategy {
            ResolutionStrategy::AuthorizedScope if parameter_policy.name == "office_ids" => {
                return Ok(BindingSource::AuthorizedScope);
            }
            ResolutionStrategy::CatalogDefault if parameter_policy.default.is_some() => {
                return Ok(BindingSource::CatalogDefault);
            }
            ResolutionStrategy::DeterministicExtraction => {
                if let Some(field) = constraint_for(&parameter_policy.name)
                    .filter(|field| facts.deterministic.contains(field))
                {
                    return Ok(BindingSource::DeterministicExtraction { field });
                }
            }
            ResolutionStrategy::VerifiedUserText => {
                if let Some(field) = constraint_for(&parameter_policy.name)
                    .filter(|field| facts.verified_user.contains(field))
                {
                    return Ok(BindingSource::VerifiedUserText { field });
                }
            }
            ResolutionStrategy::SafePriorSelection => {
                if let Some(clarification) = facts.safe_prior.get(&parameter_policy.name) {
                    return Ok(BindingSource::SafePriorSelection {
                        clarification: clarification.clone(),
                    });
                }
            }
            ResolutionStrategy::PriorStep => {
                if let Some((node, slot)) = facts.prior_steps.get(&parameter_policy.name) {
                    return Ok(BindingSource::PriorStep {
                        node: node.clone(),
                        slot: slot.clone(),
                    });
                }
            }
            ResolutionStrategy::AuthorizedDataProbe => {
                if let Some(probe) = &parameter_policy.probe {
                    let dataset = catalog
                        .datasets
                        .iter()
                        .find(|dataset| dataset.id == probe.dataset_id)
                        .ok_or(CompileError::InvalidProposal)?;
                    let shape = dataset
                        .shape(&probe.shape_id)
                        .filter(|shape| {
                            matches!(shape.role, ShapeRole::Resolver | ShapeRole::Probe)
                        })
                        .ok_or(CompileError::InvalidProposal)?;
                    let id = NodeId::new(format!("resolve_{}", parameter_policy.name))
                        .map_err(|_| CompileError::InvalidProposal)?;
                    let output = shape
                        .produces
                        .iter()
                        .find(|slot| slot.slot == probe.output_slot)
                        .ok_or(CompileError::InvalidProposal)?;
                    nodes.push(WorkflowNode {
                        id: id.clone(),
                        kind: NodeKind::ResolveEntity(ResolveEntityNode {
                            dataset_id: dataset.id.clone(),
                            resolver_shape_id: shape.id.clone(),
                            entity_kind: dataset
                                .entity
                                .as_ref()
                                .map(|entity| entity.kind.clone())
                                .unwrap_or_else(|| "entity".into()),
                            probe_row_cap: shape.row_cap.unwrap_or(25),
                        }),
                        inputs: vec![NodeInput {
                            parameter: "office_ids".into(),
                            kind: ParameterType::IntegerArray,
                            source: BindingSource::AuthorizedScope,
                        }],
                        outputs: vec![NodeOutputSlot {
                            name: output.slot.clone(),
                            kind: parse_kind(&output.kind),
                            sensitivity: output.sensitivity,
                            cardinality: map_cardinality(output.cardinality),
                        }],
                        policy: policy(None),
                        budget: NodeBudget {
                            timeout_ms: dataset.timeout_ms.unwrap_or(5_000),
                            row_cap: shape.row_cap.unwrap_or(25),
                            query_cost: 1,
                        },
                        idempotency: Idempotency::ExecuteOnce,
                        retry: RetryPolicy { max_attempts: 0 },
                    });
                    edges.push(WorkflowEdge {
                        from: id.clone(),
                        to: execute.clone(),
                        condition: EdgeCondition::Always,
                    });
                    return Ok(BindingSource::AuthorizedDataProbe {
                        node: id,
                        slot: probe.output_slot.clone(),
                    });
                }
            }
            ResolutionStrategy::Clarify
                if parameter_policy.user_required || parameter_policy.required =>
            {
                let id = NodeId::new(format!("clarify_{}", parameter_policy.name))
                    .map_err(|_| CompileError::InvalidProposal)?;
                nodes.push(WorkflowNode {
                    id: id.clone(),
                    kind: NodeKind::ClarificationInterrupt(ClarificationInterruptNode {
                        clarification_kind: "collect_fields".into(),
                        option_source: execute.clone(),
                        resume: execute.clone(),
                    }),
                    inputs: vec![],
                    outputs: vec![],
                    policy: policy(None),
                    budget: NodeBudget {
                        timeout_ms: 0,
                        row_cap: 0,
                        query_cost: 0,
                    },
                    idempotency: Idempotency::Replayable,
                    retry: RetryPolicy { max_attempts: 0 },
                });
                edges.push(WorkflowEdge {
                    from: id.clone(),
                    to: execute.clone(),
                    condition: EdgeCondition::ClarificationAnswered,
                });
                return Ok(BindingSource::SafePriorSelection { clarification: id });
            }
            _ => {}
        }
    }
    Err(CompileError::Unsupported)
}
fn resolve_proposal_resources(
    proposal: &WorkflowProposal,
    catalog: &KnowledgeCatalog,
) -> Result<(), CompileError> {
    for capability_id in &proposal.capability_ids {
        approved_capability(catalog, capability_id)?;
    }
    for node in &proposal.nodes {
        match &node.kind {
            NodeKind::ExecuteQuery(execute) => {
                let capability = execute
                    .capability_id
                    .as_deref()
                    .map(|id| approved_capability(catalog, id))
                    .transpose()?;
                if let Some(capability) = capability
                    && (execute.query_id.as_deref() != Some(&capability.query_id)
                        || execute.dataset_id.as_deref()
                            != capability
                                .dataset_recipe
                                .as_ref()
                                .map(|recipe| recipe.dataset_id.as_str())
                        || execute.shape_id.as_deref()
                            != capability
                                .dataset_recipe
                                .as_ref()
                                .map(|recipe| recipe.shape_id.as_str()))
                {
                    return Err(CompileError::InvalidProposal);
                }
                if let Some(query_id) = execute.query_id.as_deref()
                    && !catalog.queries.iter().any(|query| query.id == query_id)
                {
                    return Err(CompileError::InvalidProposal);
                }
                if let (Some(dataset_id), Some(shape_id)) =
                    (execute.dataset_id.as_deref(), execute.shape_id.as_deref())
                {
                    let dataset = catalog
                        .datasets
                        .iter()
                        .find(|dataset| dataset.id == dataset_id)
                        .ok_or(CompileError::InvalidProposal)?;
                    if dataset.shape(shape_id).is_none() {
                        return Err(CompileError::InvalidProposal);
                    }
                }
            }
            NodeKind::ResolveEntity(resolve) => {
                let dataset = catalog
                    .datasets
                    .iter()
                    .find(|dataset| dataset.id == resolve.dataset_id)
                    .ok_or(CompileError::InvalidProposal)?;
                if !matches!(
                    dataset
                        .shape(&resolve.resolver_shape_id)
                        .map(|shape| shape.role),
                    Some(ShapeRole::Resolver | ShapeRole::Probe)
                ) {
                    return Err(CompileError::InvalidProposal);
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn approved_capability<'a>(
    catalog: &'a KnowledgeCatalog,
    id: &str,
) -> Result<&'a CapabilityKnowledge, CompileError> {
    catalog
        .capabilities
        .iter()
        .find(|capability| capability.id == id && capability.status == "approved_mvp")
        .ok_or_else(|| CompileError::UnknownCapability(id.into()))
}

fn expand_iteration(
    mut workflow: ExecutionWorkflow,
    catalog: &KnowledgeCatalog,
) -> Result<ExecutionWorkflow, CompileError> {
    let mut expanded = Vec::new();
    let mut replacements = BTreeMap::<NodeId, Vec<NodeId>>::new();
    for node in std::mem::take(&mut workflow.nodes) {
        let NodeKind::ExecuteQuery(execute) = &node.kind else {
            expanded.push(node);
            continue;
        };
        let Some(iterate) = &execute.iterate_over else {
            expanded.push(node);
            continue;
        };
        if iterate.max == 0 || u16::from(iterate.max) > u16::from(workflow.budgets.max_query_count)
        {
            return Err(CompileError::BudgetExceeded);
        }
        let (dataset_id, shape_id) = (execute.dataset_id.as_deref(), execute.shape_id.as_deref());
        let (Some(dataset_id), Some(shape_id)) = (dataset_id, shape_id) else {
            return Err(CompileError::InvalidProposal);
        };
        let dataset = catalog
            .datasets
            .iter()
            .find(|dataset| dataset.id == dataset_id)
            .ok_or(CompileError::InvalidProposal)?;
        if let Some(grouped) = dataset
            .shapes
            .iter()
            .find(|shape| shape.grouped_by.as_deref() == Some(iterate.slot.as_str()))
        {
            return Err(CompileError::GroupedQueryPreferred(grouped.id.clone()));
        }
        if dataset.shape(shape_id).is_none()
            || !dataset.filters.iter().any(|filter| {
                filter.kind == "integer" && filter.operators.contains(&FilterOperator::In)
            })
        {
            return Err(CompileError::InvalidProposal);
        }
        let siblings = (0..iterate.max)
            .map(|index| {
                NodeId::new(format!("{}_iter_{}", node.id, index))
                    .map_err(|_| CompileError::InvalidProposal)
            })
            .collect::<Result<Vec<_>, _>>()?;
        for sibling in &siblings {
            let mut sibling_node = node.clone();
            sibling_node.id = sibling.clone();
            if let NodeKind::ExecuteQuery(execute) = &mut sibling_node.kind {
                execute.iterate_over = None;
            }
            expanded.push(sibling_node);
        }
        replacements.insert(node.id, siblings);
    }
    if replacements.is_empty() {
        workflow.nodes = expanded;
        return Ok(workflow);
    }
    let mut edges = Vec::new();
    for edge in workflow.edges {
        let from = replacements
            .get(&edge.from)
            .cloned()
            .unwrap_or_else(|| vec![edge.from.clone()]);
        let to = replacements
            .get(&edge.to)
            .cloned()
            .unwrap_or_else(|| vec![edge.to.clone()]);
        for from in &from {
            for to in &to {
                edges.push(WorkflowEdge {
                    from: from.clone(),
                    to: to.clone(),
                    condition: edge.condition.clone(),
                });
            }
        }
    }
    if expanded
        .iter()
        .map(|node| u16::from(node.budget.query_cost))
        .sum::<u16>()
        > u16::from(workflow.budgets.max_query_count)
    {
        return Err(CompileError::BudgetExceeded);
    }
    workflow.nodes = expanded;
    workflow.edges = edges;
    Ok(workflow)
}
fn constraint_for(parameter: &str) -> Option<ConstraintField> {
    match parameter {
        "client_id" => Some(ConstraintField::ClientId),
        "office_ids" => Some(ConstraintField::OfficeIds),
        "product_ids" => Some(ConstraintField::ProductIds),
        "currency_code" => Some(ConstraintField::CurrencyCode),
        "from_date" => Some(ConstraintField::FromDate),
        "to_date" => Some(ConstraintField::ToDate),
        "limit" => Some(ConstraintField::LimitValue),
        _ => None,
    }
}
fn parse_kind(kind: &str) -> ParameterType {
    match kind {
        "date" => ParameterType::Date,
        "integer_array" => ParameterType::IntegerArray,
        "string" => ParameterType::String,
        "currency" => ParameterType::Currency,
        _ => ParameterType::Integer,
    }
}
fn map_cardinality(value: crate::knowledge::dataset::model::Cardinality) -> Cardinality {
    match value {
        crate::knowledge::dataset::model::Cardinality::Zero => Cardinality::Zero,
        crate::knowledge::dataset::model::Cardinality::One => Cardinality::One,
        crate::knowledge::dataset::model::Cardinality::Many => Cardinality::Many,
    }
}
fn policy(capability: Option<String>) -> NodePolicy {
    NodePolicy {
        required_capability: capability,
        office_scope: OfficeScope::AuthorizedIntersection,
        max_sensitivity: Sensitivity::Pii,
        pii_required: false,
    }
}
fn output_mode(value: &str) -> OutputMode {
    match value {
        "scalar" => OutputMode::Scalar,
        "comparison" => OutputMode::Comparison,
        "grouped" => OutputMode::Grouped,
        "not_found" => OutputMode::NotFound,
        _ => OutputMode::Table,
    }
}
