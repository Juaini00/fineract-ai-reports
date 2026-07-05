mod activity;
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

    // Special-case the savings activity list: users want the flat list
    // buckets into deposits/withdrawals/charges plus weekly and 2-day
    // aggregations, all rendered inline in the same response.
    if plan.query_id == "savings.activity_list"
        && let Some(rendered) = activity::render(rows)
    {
        return Some(rendered);
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
