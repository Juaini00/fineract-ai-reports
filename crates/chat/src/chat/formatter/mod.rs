mod labels;
mod render;

use serde_json::Value;

use crate::chat::planner::{ExecutionPlan, PolicyDecision};
use crate::knowledge::model::KnowledgeCatalog;

use labels::ResponseText;

pub fn format_report_response(
    catalog: &KnowledgeCatalog,
    plan: &ExecutionPlan,
    policy: &PolicyDecision,
    result: &Value,
) -> Option<String> {
    let query = catalog
        .queries
        .iter()
        .find(|query| query.id == plan.query_id)?;
    let text = ResponseText::from_catalog(catalog);
    let rows = result.get("rows")?.as_array()?;

    if rows.is_empty() {
        return Some(text.empty_result());
    }

    match plan.output_mode.as_str() {
        "total" | "summary" => render::summary(query, rows.first()?, &text, plan, policy),
        "top_n" | "monthly_breakdown" | "monthly_top_n" => {
            render::rows(query, rows, &text, plan, policy)
        }
        _ => render::rows(query, rows, &text, plan, policy),
    }
}

#[cfg(test)]
mod tests;
