use std::fmt;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::assistant::context::canonical_state::ConstraintField;
use crate::knowledge::catalog::parameter_policy::ParameterType;
use crate::knowledge::model::Sensitivity;

pub const WORKFLOW_CONTRACT_VERSION: u16 = 1;

#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
pub struct NodeId(String);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeIdError;

impl fmt::Display for NodeIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("node id must match ^[a-z][a-z0-9_]{0,47}$")
    }
}
impl std::error::Error for NodeIdError {}

impl NodeId {
    pub fn new(value: impl Into<String>) -> Result<Self, NodeIdError> {
        let value = value.into();
        let valid = (1..=48).contains(&value.len())
            && value.as_bytes().first().is_some_and(u8::is_ascii_lowercase)
            && value
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_');
        valid.then_some(Self(value)).ok_or(NodeIdError)
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl TryFrom<String> for NodeId {
    type Error = NodeIdError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}
impl From<NodeId> for String {
    fn from(value: NodeId) -> Self {
        value.0
    }
}
impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExecutionWorkflow {
    #[schemars(with = "String")]
    pub id: Uuid,
    pub contract_version: u16,
    #[schemars(with = "String")]
    pub catalog_version: Uuid,
    pub nodes: Vec<WorkflowNode>,
    pub edges: Vec<WorkflowEdge>,
    pub budgets: WorkflowBudgets,
    pub fail_policy: FailPolicy,
    pub output_contract: OutputContract,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WorkflowBudgets {
    pub shared_timeout_ms: u64,
    pub shared_row_cap: u32,
    pub max_query_count: u8,
    pub max_parallel_queries: u8,
    pub max_model_turns: u8,
    pub max_node_retries: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FailPolicy {
    FailFast,
    ContinueLabelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OutputMode {
    Table,
    Scalar,
    Comparison,
    Grouped,
    NotFound,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OutputContract {
    pub mode: OutputMode,
    #[serde(default)]
    pub allows_partial: bool,
    pub max_sensitivity: Sensitivity,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WorkflowNode {
    pub id: NodeId,
    pub kind: NodeKind,
    #[serde(default)]
    pub inputs: Vec<NodeInput>,
    #[serde(default)]
    pub outputs: Vec<NodeOutputSlot>,
    pub policy: NodePolicy,
    pub budget: NodeBudget,
    pub idempotency: Idempotency,
    pub retry: RetryPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "type", content = "config")]
pub enum NodeKind {
    ResolveEntity(ResolveEntityNode),
    ExecuteQuery(ExecuteQueryNode),
    CardinalityBranch(CardinalityBranchNode),
    ClarificationInterrupt(ClarificationInterruptNode),
    ComposeResult(ComposeResultNode),
    Complete(CompleteNode),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ResolveEntityNode {
    pub dataset_id: String,
    pub resolver_shape_id: String,
    pub entity_kind: String,
    pub probe_row_cap: u32,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExecuteQueryNode {
    pub capability_id: Option<String>,
    pub dataset_id: Option<String>,
    pub shape_id: Option<String>,
    pub query_id: Option<String>,
    pub iterate_over: Option<IterateOver>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct IterateOver {
    pub source: NodeId,
    pub slot: String,
    pub max: u8,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CardinalityBranchNode {
    pub source: NodeId,
    pub zero: NodeId,
    pub one: NodeId,
    pub many: NodeId,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ClarificationInterruptNode {
    pub clarification_kind: String,
    pub option_source: NodeId,
    pub resume: NodeId,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ComposeResultNode {
    pub sources: Vec<NodeId>,
    pub composition: Composition,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Composition {
    Single,
    Comparison,
    Grouped,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CompleteNode {
    pub terminal: TerminalState,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TerminalState {
    Success,
    Unsupported,
    NotFound,
    FailedOperational,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NodePolicy {
    pub required_capability: Option<String>,
    pub office_scope: OfficeScope,
    pub max_sensitivity: Sensitivity,
    #[serde(default)]
    pub pii_required: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OfficeScope {
    AuthorizedIntersection,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NodeBudget {
    pub timeout_ms: u64,
    pub row_cap: u32,
    pub query_cost: u8,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Idempotency {
    Pure,
    Replayable,
    ExecuteOnce,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RetryPolicy {
    pub max_attempts: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WorkflowEdge {
    pub from: NodeId,
    pub to: NodeId,
    pub condition: EdgeCondition,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EdgeCondition {
    Always,
    Cardinality(Cardinality),
    ClarificationAnswered,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Cardinality {
    Zero,
    One,
    Many,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NodeInput {
    pub parameter: String,
    pub kind: ParameterType,
    pub source: BindingSource,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "type", content = "config")]
pub enum BindingSource {
    AuthorizedScope,
    CatalogDefault,
    DeterministicExtraction { field: ConstraintField },
    VerifiedUserText { field: ConstraintField },
    ExactSensitiveInput,
    SafePriorSelection { clarification: NodeId },
    PriorStep { node: NodeId, slot: String },
    AuthorizedDataProbe { node: NodeId, slot: String },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NodeOutputSlot {
    pub name: String,
    pub kind: ParameterType,
    pub sensitivity: Sensitivity,
    pub cardinality: Cardinality,
}

/// Planner output is deliberately IDs, bindings, and values only. It has no SQL field.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WorkflowProposal {
    pub capability_ids: Vec<String>,
    #[serde(default)]
    pub nodes: Vec<WorkflowNode>,
    #[serde(default)]
    pub edges: Vec<WorkflowEdge>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkflowProposalWire {
    capability_ids: Vec<String>,
    #[serde(default)]
    nodes: Vec<WorkflowNode>,
    #[serde(default)]
    edges: Vec<WorkflowEdge>,
}

impl<'de> Deserialize<'de> for WorkflowProposal {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let wire = WorkflowProposalWire::deserialize(deserializer)?;
        let proposal = Self {
            capability_ids: wire.capability_ids,
            nodes: wire.nodes,
            edges: wire.edges,
        };
        let encoded = serde_json::to_value(&proposal).map_err(serde::de::Error::custom)?;
        if contains_sql(&encoded) {
            return Err(serde::de::Error::custom(
                "workflow proposal cannot contain SQL",
            ));
        }
        Ok(proposal)
    }
}

fn contains_sql(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::String(value) => {
            let value = value.to_ascii_lowercase();
            ["select ", "insert ", "update ", "delete ", " from ", ";--"]
                .iter()
                .any(|needle| value.contains(needle))
        }
        serde_json::Value::Array(values) => values.iter().any(contains_sql),
        serde_json::Value::Object(values) => values.values().any(contains_sql),
        _ => false,
    }
}
