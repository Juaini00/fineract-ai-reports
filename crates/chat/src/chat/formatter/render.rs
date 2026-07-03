use serde_json::Value;

use crate::chat::formatter::labels::ResponseText;
use crate::chat::planner::{ExecutionPlan, PolicyDecision};
use crate::knowledge::model::{QueryKnowledge, QueryOutputField};

pub fn summary(
    query: &QueryKnowledge,
    row: &Value,
    text: &ResponseText,
    plan: &ExecutionPlan,
    policy: &PolicyDecision,
) -> Option<String> {
    let parts = visible_fields(query, policy)
        .filter_map(|field| field_sentence(field, row, text, plan))
        .collect::<Vec<_>>();

    (!parts.is_empty()).then(|| format!("{}.", parts.join(". ")))
}

pub fn rows(
    query: &QueryKnowledge,
    rows: &[Value],
    text: &ResponseText,
    plan: &ExecutionPlan,
    policy: &PolicyDecision,
) -> Option<String> {
    let visible = visible_fields(query, policy).collect::<Vec<_>>();
    if visible.is_empty() {
        return None;
    }

    let mut lines = Vec::with_capacity(rows.len() + 1);
    lines.push(format!("Report returned {} row(s).", rows.len()));
    for (index, row) in rows.iter().take(50).enumerate() {
        let values = visible
            .iter()
            .filter_map(|field| field_sentence(field, row, text, plan))
            .collect::<Vec<_>>();
        if !values.is_empty() {
            lines.push(format!("{}. {}.", index + 1, values.join("; ")));
        }
    }
    if rows.len() > 50 {
        lines.push(format!("... and {} more row(s).", rows.len() - 50));
    }

    Some(lines.join("\n"))
}

fn visible_fields<'a>(
    query: &'a QueryKnowledge,
    policy: &'a PolicyDecision,
) -> impl Iterator<Item = &'a QueryOutputField> {
    query.output_fields.iter().filter(move |field| {
        !matches!(field.sensitivity.as_str(), "secret")
            && (field.sensitivity != "pii" || policy.can_view_pii)
    })
}

fn field_sentence(
    field: &QueryOutputField,
    row: &Value,
    text: &ResponseText,
    plan: &ExecutionPlan,
) -> Option<String> {
    let value = row.get(&field.name)?;
    if value.is_null() {
        return None;
    }

    Some(format!(
        "{}: {}",
        text.field_label(&field.name),
        render_value(field, value, row, plan)
    ))
}

fn render_value(
    field: &QueryOutputField,
    value: &Value,
    row: &Value,
    plan: &ExecutionPlan,
) -> String {
    let rendered = value
        .as_str()
        .map(str::to_string)
        .unwrap_or_else(|| value.to_string());

    if field.kind == "decimal"
        && let Some(currency) = row_currency(row).or_else(|| plan_currency(plan))
    {
        return format!("{currency} {rendered}");
    }

    rendered
}

fn row_currency(row: &Value) -> Option<&str> {
    row.get("currency_code")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
}

fn plan_currency(plan: &ExecutionPlan) -> Option<&str> {
    plan.params
        .get("currency_code")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
}
