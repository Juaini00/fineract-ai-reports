use std::collections::{HashMap, HashSet};

use app_core::auth::model::PrincipalContext;

use crate::knowledge::{
    catalog::parameter_policy::ParameterType,
    model::{KnowledgeCatalog, Sensitivity},
};
use crate::policy::authorization::ensure_capability_allowed;

use super::{contract::*, graph::WorkflowGraph};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerifyError {
    Cycle,
    UnknownResource,
    TypeIncompatibleBinding,
    DataDependentSqlIdentifier,
    MissingOfficeScope,
    BudgetExceeded,
    PartialResultsNotPermitted,
    SensitivityWidening,
    UnreachableOrOrphanNode,
    DanglingResume,
    UnboundRequiredInput,
    CapabilityNotPermitted,
}
impl VerifyError {
    pub fn client_message(self) -> &'static str {
        match self {
            Self::Cycle => "The requested workflow is invalid.",
            Self::UnknownResource => "The requested report is unavailable.",
            Self::TypeIncompatibleBinding => "The requested workflow has incompatible inputs.",
            Self::DataDependentSqlIdentifier => {
                "The requested report contains an invalid identifier."
            }
            Self::MissingOfficeScope => "The requested report has no authorized office scope.",
            Self::BudgetExceeded => "The requested report exceeds its execution budget.",
            Self::PartialResultsNotPermitted => {
                "The requested report does not permit partial results."
            }
            Self::SensitivityWidening => "The requested report exceeds its data sensitivity.",
            Self::UnreachableOrOrphanNode => "The requested workflow is incomplete.",
            Self::DanglingResume => "The requested workflow cannot be resumed.",
            Self::UnboundRequiredInput => "The requested report is missing a required input.",
            Self::CapabilityNotPermitted => "The requested report is not permitted.",
        }
    }
    pub fn audit_reason_code(self) -> &'static str {
        match self {
            Self::Cycle => "V1_CYCLE",
            Self::UnknownResource => "V2_UNKNOWN_RESOURCE",
            Self::TypeIncompatibleBinding => "V3_TYPE_INCOMPATIBLE_BINDING",
            Self::DataDependentSqlIdentifier => "V4_DATA_DEPENDENT_SQL_IDENTIFIER",
            Self::MissingOfficeScope => "V5_MISSING_OFFICE_SCOPE",
            Self::BudgetExceeded => "V6_BUDGET_EXCEEDED",
            Self::PartialResultsNotPermitted => "V7_PARTIAL_RESULTS_NOT_PERMITTED",
            Self::SensitivityWidening => "V8_SENSITIVITY_WIDENING",
            Self::UnreachableOrOrphanNode => "V9_UNREACHABLE_OR_ORPHAN_NODE",
            Self::DanglingResume => "V10_DANGLING_RESUME",
            Self::UnboundRequiredInput => "V11_UNBOUND_REQUIRED_INPUT",
            Self::CapabilityNotPermitted => "V12_CAPABILITY_NOT_PERMITTED",
        }
    }
}
impl std::fmt::Display for VerifyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.client_message())
    }
}
impl std::error::Error for VerifyError {}

#[derive(Debug, Clone, PartialEq)]
pub struct VerifiedWorkflow(pub ExecutionWorkflow);

pub fn verify_before_execute<T>(
    workflow: ExecutionWorkflow,
    principal: &PrincipalContext,
    catalog: &KnowledgeCatalog,
    execute: impl FnOnce(&VerifiedWorkflow) -> T,
) -> Result<T, VerifyError> {
    let verified = verify(workflow, principal, catalog)?;
    Ok(execute(&verified))
}

pub fn verify(
    workflow: ExecutionWorkflow,
    principal: &PrincipalContext,
    catalog: &KnowledgeCatalog,
) -> Result<VerifiedWorkflow, VerifyError> {
    let graph = WorkflowGraph::new(&workflow);
    if graph.is_cyclic() {
        return Err(VerifyError::Cycle);
    }
    let ids: HashSet<_> = workflow.nodes.iter().map(|node| &node.id).collect();
    if ids.len() != workflow.nodes.len()
        || workflow
            .edges
            .iter()
            .any(|edge| !ids.contains(&edge.from) || !ids.contains(&edge.to))
        || graph.entry_nodes().len() != 1
        || !workflow
            .nodes
            .iter()
            .any(|node| matches!(node.kind, NodeKind::Complete(_)))
    {
        return Err(VerifyError::UnreachableOrOrphanNode);
    }
    let entry = graph.entry_nodes().pop().expect("entry count checked");
    if graph.reachable_from(&entry).len() != workflow.nodes.len() {
        return Err(VerifyError::UnreachableOrOrphanNode);
    }
    if workflow.fail_policy == FailPolicy::ContinueLabelled
        && !workflow.output_contract.allows_partial
    {
        return Err(VerifyError::PartialResultsNotPermitted);
    }
    let spent_queries: u16 = workflow
        .nodes
        .iter()
        .map(|node| u16::from(node.budget.query_cost))
        .sum();
    let spent_rows: u64 = workflow
        .nodes
        .iter()
        .map(|node| u64::from(node.budget.row_cap))
        .sum();
    let spent_time: u64 = workflow
        .nodes
        .iter()
        .map(|node| node.budget.timeout_ms)
        .sum();
    if spent_queries > u16::from(workflow.budgets.max_query_count)
        || spent_rows > u64::from(workflow.budgets.shared_row_cap)
        || spent_time > workflow.budgets.shared_timeout_ms
        || workflow.budgets.max_parallel_queries == 0
        || graph.max_runnable_width() > usize::from(workflow.budgets.max_parallel_queries)
    {
        return Err(VerifyError::BudgetExceeded);
    }
    let topo = graph.topological_order().map_err(|_| VerifyError::Cycle)?;
    let positions: HashMap<_, _> = topo
        .iter()
        .enumerate()
        .map(|(index, id)| (id, index))
        .collect();
    let nodes: HashMap<_, _> = workflow.nodes.iter().map(|node| (&node.id, node)).collect();
    for node in &workflow.nodes {
        verify_node_resources(node, catalog)?;
        let referenced_nodes = match &node.kind {
            NodeKind::CardinalityBranch(branch) => {
                vec![&branch.source, &branch.zero, &branch.one, &branch.many]
            }
            NodeKind::ComposeResult(compose) => compose.sources.iter().collect(),
            _ => Vec::new(),
        };
        if referenced_nodes
            .iter()
            .any(|reference| !nodes.contains_key(*reference))
            || matches!(&node.kind, NodeKind::ComposeResult(compose) if compose.sources.is_empty())
        {
            return Err(VerifyError::UnreachableOrOrphanNode);
        }
        if node.budget.query_cost > 0
            && !node.inputs.iter().any(|input| {
                input.parameter == "office_ids"
                    && matches!(input.source, BindingSource::AuthorizedScope)
            })
        {
            return Err(VerifyError::MissingOfficeScope);
        }
        if sensitivity_rank(node.policy.max_sensitivity)
            > sensitivity_rank(workflow.output_contract.max_sensitivity)
            || node.outputs.iter().any(|output| {
                sensitivity_rank(output.sensitivity)
                    > sensitivity_rank(workflow.output_contract.max_sensitivity)
            })
        {
            return Err(VerifyError::SensitivityWidening);
        }
        if node.policy.pii_required && !principal.can_view_pii {
            return Err(VerifyError::CapabilityNotPermitted);
        }
        if let Some(capability) = node.policy.required_capability.as_deref() {
            ensure_capability_allowed(principal, capability)
                .map_err(|_| VerifyError::CapabilityNotPermitted)?;
        }
        if let NodeKind::ExecuteQuery(exec) = &node.kind {
            if let Some(capability) = exec.capability_id.as_deref() {
                ensure_capability_allowed(principal, capability)
                    .map_err(|_| VerifyError::CapabilityNotPermitted)?;
            }
            if let Some(query) = exec
                .query_id
                .as_deref()
                .and_then(|id| catalog.queries.iter().find(|query| query.id == id))
                && query.parameters.iter().any(|parameter| {
                    parameter.required
                        && !node
                            .inputs
                            .iter()
                            .any(|input| input.parameter == parameter.name)
                })
            {
                return Err(VerifyError::UnboundRequiredInput);
            }
        }
        for input in &node.inputs {
            verify_input(input, node, &nodes, &positions, catalog)?;
        }
        if let NodeKind::ClarificationInterrupt(interrupt) = &node.kind
            && (!graph.contains(&interrupt.resume)
                || !graph.reachable_from(&node.id).contains(&interrupt.resume))
        {
            return Err(VerifyError::DanglingResume);
        }
    }
    Ok(VerifiedWorkflow(workflow))
}

fn verify_node_resources(
    node: &WorkflowNode,
    catalog: &KnowledgeCatalog,
) -> Result<(), VerifyError> {
    match &node.kind {
        NodeKind::ExecuteQuery(exec) => {
            if let Some(id) = &exec.capability_id {
                let cap = catalog
                    .capabilities
                    .iter()
                    .find(|cap| &cap.id == id)
                    .filter(|cap| cap.status == "approved_mvp")
                    .ok_or(VerifyError::UnknownResource)?;
                if exec
                    .query_id
                    .as_deref()
                    .is_some_and(|query| query != cap.query_id)
                {
                    return Err(VerifyError::UnknownResource);
                }
            }
            if let Some(id) = &exec.query_id
                && !catalog.queries.iter().any(|query| &query.id == id)
            {
                return Err(VerifyError::UnknownResource);
            }
            if exec.dataset_id.is_some() != exec.shape_id.is_some() {
                return Err(VerifyError::UnknownResource);
            }
            if let (Some(dataset), Some(shape)) = (&exec.dataset_id, &exec.shape_id) {
                let dataset = catalog
                    .datasets
                    .iter()
                    .find(|value| &value.id == dataset)
                    .ok_or(VerifyError::UnknownResource)?;
                if dataset.shape(shape).is_none() {
                    return Err(VerifyError::UnknownResource);
                }
            }
        }
        NodeKind::ResolveEntity(node) => {
            let dataset = catalog
                .datasets
                .iter()
                .find(|value| value.id == node.dataset_id)
                .ok_or(VerifyError::UnknownResource)?;
            if !matches!(
                dataset
                    .shape(&node.resolver_shape_id)
                    .map(|shape| shape.role),
                Some(crate::knowledge::dataset::model::ShapeRole::Resolver)
            ) {
                return Err(VerifyError::UnknownResource);
            }
        }
        _ => {}
    };
    Ok(())
}
fn verify_input(
    input: &NodeInput,
    consumer: &WorkflowNode,
    nodes: &HashMap<&NodeId, &WorkflowNode>,
    positions: &HashMap<&NodeId, usize>,
    catalog: &KnowledgeCatalog,
) -> Result<(), VerifyError> {
    if looks_like_sql_identifier(&input.parameter) {
        return Err(VerifyError::DataDependentSqlIdentifier);
    }
    if matches!(input.source, BindingSource::AuthorizedScope)
        && (input.parameter != "office_ids" || input.kind != ParameterType::IntegerArray)
    {
        return Err(VerifyError::TypeIncompatibleBinding);
    }
    match &input.source {
        BindingSource::SafePriorSelection { clarification } => {
            let Some(producer) = nodes.get(clarification) else {
                return Err(VerifyError::TypeIncompatibleBinding);
            };
            if positions.get(clarification) >= positions.get(&consumer.id)
                || !matches!(producer.kind, NodeKind::ClarificationInterrupt(_))
            {
                return Err(VerifyError::TypeIncompatibleBinding);
            }
        }
        BindingSource::PriorStep { node, slot }
        | BindingSource::AuthorizedDataProbe { node, slot } => {
            let producer = nodes
                .get(node)
                .ok_or(VerifyError::TypeIncompatibleBinding)?;
            if positions.get(node) >= positions.get(&consumer.id) {
                return Err(VerifyError::TypeIncompatibleBinding);
            }
            let output = producer
                .outputs
                .iter()
                .find(|output| &output.name == slot)
                .ok_or(VerifyError::TypeIncompatibleBinding)?;
            if output.kind != input.kind
                || sensitivity_rank(output.sensitivity)
                    > sensitivity_rank(consumer.policy.max_sensitivity)
            {
                return Err(VerifyError::SensitivityWidening);
            }
        }
        _ => {}
    }
    if let NodeKind::ExecuteQuery(exec) = &consumer.kind
        && let Some(parameters) = exec
            .query_id
            .as_deref()
            .and_then(|id| catalog.queries.iter().find(|query| query.id == id))
            .map(|query| &query.parameters)
        && let Some(parameter) = parameters
            .iter()
            .find(|parameter| parameter.name == input.parameter)
    {
        if parameter_type(&parameter.kind) != Some(input.kind) {
            return Err(VerifyError::TypeIncompatibleBinding);
        }
        if parameter.required
            && matches!(input.source, BindingSource::ExactSensitiveInput)
            && input.kind != ParameterType::String
        {
            return Err(VerifyError::UnboundRequiredInput);
        }
    }
    Ok(())
}
fn parameter_type(value: &str) -> Option<ParameterType> {
    match value {
        "date" => Some(ParameterType::Date),
        "integer" => Some(ParameterType::Integer),
        "integer_array" => Some(ParameterType::IntegerArray),
        "string" => Some(ParameterType::String),
        "currency" => Some(ParameterType::Currency),
        _ => None,
    }
}
fn looks_like_sql_identifier(value: &str) -> bool {
    value.contains([' ', ';', '"', '\'', '(', ')', '.']) || value.contains("--")
}
fn sensitivity_rank(value: Sensitivity) -> u8 {
    match value {
        Sensitivity::PublicBusiness => 0,
        Sensitivity::MaskedOutput => 1,
        Sensitivity::FilterOnly => 2,
        Sensitivity::Pii => 3,
        Sensitivity::NeverUse => 4,
    }
}
