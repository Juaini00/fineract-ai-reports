use std::collections::BTreeMap;

use uuid::Uuid;

use crate::assistant::context::canonical_state::ConstraintField;
use crate::knowledge::{
    catalog::parameter_policy::{ParameterPolicy, ParameterType, ResolutionStrategy},
    dataset::model::{FilterInputPolicy, FilterOperator, ShapeRole},
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
    ComparisonFactsDiverge(NodeId),
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
            Self::ComparisonFactsDiverge(node) => {
                write!(
                    f,
                    "comparison node {node} has sources with differing scope or temporal facts"
                )
            }
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
        let workflow = expand_iteration(workflow, catalog)?;
        check_comparison_facts(&workflow)?;
        return Ok(workflow);
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
    let workflow = expand_iteration(
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
    )?;
    check_comparison_facts(&workflow)?;
    Ok(workflow)
}

/// `comparison` composition requires every source to share identical scope and temporal
/// facts; a differing binding source on a scope/temporal parameter is a compile error,
/// never a runtime warning, because it would otherwise compare rows drawn from different
/// authorized populations or time windows.
fn check_comparison_facts(workflow: &ExecutionWorkflow) -> Result<(), CompileError> {
    for node in &workflow.nodes {
        let NodeKind::ComposeResult(compose) = &node.kind else {
            continue;
        };
        if compose.composition != Composition::Comparison {
            continue;
        }
        let mut reference: Option<(&NodeId, BTreeMap<&str, &BindingSource>)> = None;
        for source_id in &compose.sources {
            let source_node = workflow
                .nodes
                .iter()
                .find(|candidate| &candidate.id == source_id)
                .ok_or(CompileError::InvalidProposal)?;
            let facts: BTreeMap<&str, &BindingSource> = source_node
                .inputs
                .iter()
                .filter(|input| is_scope_or_temporal(&input.parameter))
                .map(|input| (input.parameter.as_str(), &input.source))
                .collect();
            match &reference {
                None => reference = Some((source_id, facts)),
                Some((_, reference_facts)) if *reference_facts != facts => {
                    return Err(CompileError::ComparisonFactsDiverge(node.id.clone()));
                }
                Some(_) => {}
            }
        }
    }
    Ok(())
}
fn is_scope_or_temporal(parameter: &str) -> bool {
    matches!(
        constraint_for(parameter),
        Some(
            ConstraintField::ClientId
                | ConstraintField::OfficeIds
                | ConstraintField::ProductIds
                | ConstraintField::FromDate
                | ConstraintField::ToDate
        )
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
    let query_required: std::collections::HashSet<&str> = catalog
        .queries
        .iter()
        .find(|query| query.id == capability.query_id)
        .map(|query| {
            query
                .parameters
                .iter()
                .filter(|parameter| parameter.required)
                .map(|parameter| parameter.name.as_str())
                .collect()
        })
        .unwrap_or_default();
    for policy_item in capability.parameter_policies.iter().filter(|policy| {
        policy.name != "office_ids"
            && (policy.required || query_required.contains(policy.name.as_str()))
    }) {
        let source = acquisition_source(
            capability,
            policy_item,
            facts,
            nodes,
            edges,
            execute,
            catalog,
        )?;
        inputs.push(NodeInput {
            parameter: policy_item.name.clone(),
            kind: policy_item.kind,
            source,
        });
    }
    Ok(inputs)
}
fn acquisition_source(
    capability: &CapabilityKnowledge,
    parameter_policy: &ParameterPolicy,
    facts: &AcquisitionFacts,
    nodes: &mut Vec<WorkflowNode>,
    edges: &mut Vec<WorkflowEdge>,
    execute: &NodeId,
    catalog: &KnowledgeCatalog,
) -> Result<BindingSource, CompileError> {
    // Exact-identifier dataset filters (e.g. savings `account_number`) are
    // transient, equality-only, and never a normal SQL bind: the value reaches
    // approved SQL out-of-band via `FineractDataExecutor`'s sensitive
    // identifier field (`run.rs::bindings_for` skips `ExactSensitiveInput`
    // inputs entirely — no `Null` placeholder, no `plan.params` entry) and
    // `state::persisted_output` redacts it from the durable node-run ledger
    // (SI-7). Emit a satisfied (non-clarify) binding unconditionally — the
    // caller's verified plan (`plan_selected_capability_verified`, which bails
    // on a missing required parameter) already guaranteed the value is present
    // by the time compilation runs.
    if is_exact_identifier_param(capability, &parameter_policy.name, catalog) {
        return Ok(BindingSource::ExactSensitiveInput);
    }
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
                // The authoritative parameter->constraint mapping lives in
                // `knowledge/parameter-bindings/` (catalog `binding_fields`),
                // not a Rust match — a param like `search` binds `person_name`,
                // which the old local `constraint_for` never knew, so a
                // provided value fell through to the `Clarify` arm.
                if let Some(field) = catalog
                    .binding_fields(&parameter_policy.name)
                    .iter()
                    .find(|field| facts.deterministic.contains(field))
                    .cloned()
                {
                    return Ok(BindingSource::DeterministicExtraction { field });
                }
            }
            ResolutionStrategy::VerifiedUserText => {
                if let Some(field) = catalog
                    .binding_fields(&parameter_policy.name)
                    .iter()
                    .find(|field| facts.verified_user.contains(field))
                    .cloned()
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
                    // The guard/executor drive resolver SQL off an approved
                    // capability's `dataset_recipe` (`data.rs`/`data_executor.rs`),
                    // so a resolver node must carry the id of the approved
                    // capability backing this shape — otherwise `node_executor`
                    // has no capability id to declare and the guarded probe is
                    // rejected. Match the capability whose recipe selects exactly
                    // this dataset+shape.
                    let backing = catalog
                        .capabilities
                        .iter()
                        .find(|capability| {
                            capability.status == "approved_mvp"
                                && capability.dataset_recipe.as_ref().is_some_and(|recipe| {
                                    recipe.dataset_id == probe.dataset_id
                                        && recipe.shape_id == probe.shape_id
                                })
                        })
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
                        policy: policy(Some(backing.id.clone())),
                        budget: NodeBudget {
                            timeout_ms: dataset.timeout_ms.unwrap_or(5_000),
                            row_cap: shape.row_cap.unwrap_or(25),
                            query_cost: 1,
                        },
                        idempotency: Idempotency::ExecuteOnce,
                        retry: RetryPolicy { max_attempts: 0 },
                    });
                    // Branch on the resolved cardinality instead of resolving
                    // straight into the execute node (which binds correctly
                    // only for exactly one candidate): zero -> not-found
                    // terminal, one -> execute (id bound from the resolver
                    // slot), many -> SelectEntity clarification over the probe's
                    // safe labels. ponytail: one probe per compile — a second
                    // probe-backed param would add a second branch whose `one`
                    // arm also targets `execute`, and `runnable` fires on ANY
                    // satisfied incoming edge; no capability declares two today.
                    let branch_id = NodeId::new(format!("branch_{}", parameter_policy.name))
                        .map_err(|_| CompileError::InvalidProposal)?;
                    let not_found_id = NodeId::new(format!("not_found_{}", parameter_policy.name))
                        .map_err(|_| CompileError::InvalidProposal)?;
                    let select_id = NodeId::new(format!("select_{}", parameter_policy.name))
                        .map_err(|_| CompileError::InvalidProposal)?;
                    let terminal_budget = NodeBudget {
                        timeout_ms: 0,
                        row_cap: 0,
                        query_cost: 0,
                    };
                    nodes.push(WorkflowNode {
                        id: branch_id.clone(),
                        kind: NodeKind::CardinalityBranch(CardinalityBranchNode {
                            source: id.clone(),
                            zero: not_found_id.clone(),
                            one: execute.clone(),
                            many: select_id.clone(),
                        }),
                        inputs: vec![],
                        outputs: vec![],
                        policy: policy(None),
                        budget: terminal_budget.clone(),
                        idempotency: Idempotency::Pure,
                        retry: RetryPolicy { max_attempts: 0 },
                    });
                    nodes.push(WorkflowNode {
                        id: not_found_id.clone(),
                        kind: NodeKind::Complete(CompleteNode {
                            terminal: TerminalState::NotFound,
                        }),
                        inputs: vec![],
                        outputs: vec![],
                        policy: policy(None),
                        budget: terminal_budget.clone(),
                        idempotency: Idempotency::Pure,
                        retry: RetryPolicy { max_attempts: 0 },
                    });
                    nodes.push(WorkflowNode {
                        id: select_id.clone(),
                        kind: NodeKind::ClarificationInterrupt(ClarificationInterruptNode {
                            clarification_kind: "select_entity".into(),
                            option_source: id.clone(),
                            resume: execute.clone(),
                        }),
                        inputs: vec![],
                        outputs: vec![],
                        policy: policy(None),
                        budget: terminal_budget,
                        idempotency: Idempotency::Replayable,
                        retry: RetryPolicy { max_attempts: 0 },
                    });
                    edges.push(WorkflowEdge {
                        from: id.clone(),
                        to: branch_id.clone(),
                        condition: EdgeCondition::Always,
                    });
                    edges.push(WorkflowEdge {
                        from: branch_id.clone(),
                        to: not_found_id,
                        condition: EdgeCondition::Cardinality(Cardinality::Zero),
                    });
                    edges.push(WorkflowEdge {
                        from: branch_id.clone(),
                        to: execute.clone(),
                        condition: EdgeCondition::Cardinality(Cardinality::One),
                    });
                    edges.push(WorkflowEdge {
                        from: branch_id,
                        to: select_id.clone(),
                        condition: EdgeCondition::Cardinality(Cardinality::Many),
                    });
                    edges.push(WorkflowEdge {
                        from: select_id,
                        to: execute.clone(),
                        condition: EdgeCondition::ClarificationAnswered,
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
/// True when `name` maps to a dataset-recipe filter whose dataset filter slot
/// declares `input_policy: exact_identifier` (a transient sensitive identifier,
/// e.g. savings `account_number`).
fn is_exact_identifier_param(
    capability: &CapabilityKnowledge,
    name: &str,
    catalog: &KnowledgeCatalog,
) -> bool {
    let Some(recipe) = capability.dataset_recipe.as_ref() else {
        return false;
    };
    let Some(dataset) = catalog
        .datasets
        .iter()
        .find(|dataset| dataset.id == recipe.dataset_id)
    else {
        return false;
    };
    recipe.filters.iter().any(|mapping| {
        mapping.parameter.as_deref() == Some(name)
            && dataset.filters.iter().any(|slot| {
                slot.id == mapping.filter_id
                    && slot.input_policy == FilterInputPolicy::ExactIdentifier
            })
    })
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::knowledge::catalog::loader::KnowledgeLoader;

    fn catalog() -> KnowledgeCatalog {
        KnowledgeLoader::new("../../knowledge", "../../queries")
            .load()
            .unwrap()
    }
    fn budgets() -> WorkflowBudgets {
        WorkflowBudgets {
            shared_timeout_ms: 30_000,
            shared_row_cap: 1_000,
            max_query_count: 10,
            max_parallel_queries: 2,
            max_model_turns: 2,
            max_node_retries: 0,
        }
    }
    fn budget() -> NodeBudget {
        NodeBudget {
            timeout_ms: 1,
            row_cap: 1,
            query_cost: 1,
        }
    }
    fn policy() -> NodePolicy {
        NodePolicy {
            required_capability: None,
            office_scope: OfficeScope::AuthorizedIntersection,
            max_sensitivity: Sensitivity::Pii,
            pii_required: false,
        }
    }
    fn execute_node(id_value: &str, from_date_source: BindingSource) -> WorkflowNode {
        WorkflowNode {
            id: NodeId::new(id_value).unwrap(),
            kind: NodeKind::ExecuteQuery(ExecuteQueryNode {
                capability_id: None,
                dataset_id: None,
                shape_id: None,
                query_id: None,
                iterate_over: None,
            }),
            inputs: vec![
                NodeInput {
                    parameter: "office_ids".into(),
                    kind: ParameterType::IntegerArray,
                    source: BindingSource::AuthorizedScope,
                },
                NodeInput {
                    parameter: "from_date".into(),
                    kind: ParameterType::Date,
                    source: from_date_source,
                },
            ],
            outputs: vec![],
            policy: policy(),
            budget: budget(),
            idempotency: Idempotency::ExecuteOnce,
            retry: RetryPolicy { max_attempts: 0 },
        }
    }
    fn compose_node(sources: &[&str]) -> WorkflowNode {
        WorkflowNode {
            id: NodeId::new("compose").unwrap(),
            kind: NodeKind::ComposeResult(ComposeResultNode {
                sources: sources
                    .iter()
                    .map(|value| NodeId::new(*value).unwrap())
                    .collect(),
                composition: Composition::Comparison,
            }),
            inputs: vec![],
            outputs: vec![],
            policy: policy(),
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
    fn comparison_compile_rejects_diverging_temporal_facts() {
        let proposal = WorkflowProposal {
            capability_ids: vec![],
            nodes: vec![
                execute_node(
                    "deposits",
                    BindingSource::DeterministicExtraction {
                        field: ConstraintField::FromDate,
                    },
                ),
                execute_node(
                    "withdrawals",
                    BindingSource::VerifiedUserText {
                        field: ConstraintField::FromDate,
                    },
                ),
                compose_node(&["deposits", "withdrawals"]),
            ],
            edges: vec![],
        };
        let error =
            compile(proposal, &catalog(), Uuid::nil(), budgets()).expect_err("must be rejected");
        assert!(matches!(error, CompileError::ComparisonFactsDiverge(_)));
    }

    #[test]
    fn comparison_compile_accepts_identical_scope_and_temporal_facts() {
        let proposal = WorkflowProposal {
            capability_ids: vec![],
            nodes: vec![
                execute_node(
                    "deposits",
                    BindingSource::DeterministicExtraction {
                        field: ConstraintField::FromDate,
                    },
                ),
                execute_node(
                    "withdrawals",
                    BindingSource::DeterministicExtraction {
                        field: ConstraintField::FromDate,
                    },
                ),
                compose_node(&["deposits", "withdrawals"]),
            ],
            edges: vec![],
        };
        compile(proposal, &catalog(), Uuid::nil(), budgets()).expect("must compile");
    }
}
