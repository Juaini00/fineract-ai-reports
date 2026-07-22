mod guard;
mod parameters;
mod planning;

use anyhow::Result;
use app_core::auth::model::PrincipalContext;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    assistant::{
        AssistantIntent, DeterministicExtraction, EffectiveConstraints, PlannerInputSnapshot,
        execution::plan::{ExecutionPlan, PolicyDecision},
    },
    knowledge::model::KnowledgeCatalog,
};

#[cfg(test)]
use crate::{assistant::AssistantEntityType, knowledge::model::QueryKnowledge};
#[cfg(test)]
use parameters::params_from_verified;
#[cfg(test)]
use serde_json::json;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ToolRequest {
    pub tool_name: String,
    pub capability_id: Option<String>,
    pub query_id: Option<String>,
    #[serde(default)]
    pub params: Value,
    #[serde(default)]
    pub evidence_refs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ToolResult {
    pub tool_name: String,
    pub ok: bool,
    #[serde(default)]
    pub rows: Vec<Value>,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub error: Option<ToolValidationError>,
    #[serde(default)]
    pub evidence_refs: Vec<String>,
}

pub const APPROVED_SQL_TOOL: &str = "approved_catalog_sql";

pub fn tool_request_from_plan(plan: &ExecutionPlan, evidence_refs: Vec<String>) -> ToolRequest {
    ToolRequest {
        tool_name: APPROVED_SQL_TOOL.into(),
        capability_id: Some(plan.capability.clone()),
        query_id: Some(plan.query_id.clone()),
        params: plan.params.clone(),
        evidence_refs,
    }
}

pub fn tool_result_from_execution(request: &ToolRequest, execution_result: Value) -> ToolResult {
    ToolResult {
        tool_name: request.tool_name.clone(),
        ok: true,
        rows: execution_result
            .get("rows")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
        summary: execution_result
            .get("row_count")
            .and_then(Value::as_u64)
            .map(|count| format!("{count} row(s) returned")),
        error: None,
        evidence_refs: request.evidence_refs.clone(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ToolValidationError {
    pub code: String,
    pub message: String,
    #[serde(default)]
    pub field: Option<String>,
}

pub fn plan_selected_capability(
    catalog: &KnowledgeCatalog,
    capability_id: &str,
    intent: &AssistantIntent,
) -> Result<ExecutionPlan> {
    planning::plan_selected_capability(catalog, capability_id, intent)
}

pub fn plan_selected_capability_verified(
    catalog: &KnowledgeCatalog,
    capability_id: &str,
    intent: &AssistantIntent,
    deterministic_extraction: Option<&DeterministicExtraction>,
) -> Result<ExecutionPlan> {
    planning::plan_selected_capability_verified(
        catalog,
        capability_id,
        intent,
        deterministic_extraction,
    )
}

pub fn approved_default_patch(
    catalog: &KnowledgeCatalog,
    capability_id: &str,
) -> Result<crate::assistant::ConstraintPatch> {
    parameters::approved_default_patch(catalog, capability_id)
}

pub fn normalize_effective_parameters(
    catalog: &KnowledgeCatalog,
    capability_id: &str,
    effective: &EffectiveConstraints,
) -> Result<Value> {
    parameters::normalize_effective_parameters(catalog, capability_id, effective)
}

pub fn plan_from_snapshot(
    catalog: &KnowledgeCatalog,
    snapshot: &PlannerInputSnapshot,
) -> Result<ExecutionPlan> {
    planning::plan_from_snapshot(catalog, snapshot)
}

pub fn guard_selected_capability(
    client: &PrincipalContext,
    catalog: &KnowledgeCatalog,
    plan: &ExecutionPlan,
) -> PolicyDecision {
    guard::guard_selected_capability(client, catalog, plan)
}

#[cfg(test)]
mod tests;
