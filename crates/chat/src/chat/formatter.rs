use serde_json::Value;

use crate::chat::planner::ExecutionPlan;

pub fn format_report_response(plan: &ExecutionPlan, result: &Value) -> Option<String> {
    match plan.capability.as_str() {
        "savings_deposit_total" => {
            format_total(result, "deposit", "total_deposit_amount", "deposit_count")
        }
        "savings_deposit_top_n" => format_top_n(result, "deposit"),
        "savings_withdrawal_total" => format_total(
            result,
            "withdrawal",
            "total_withdrawal_amount",
            "withdrawal_count",
        ),
        "savings_withdrawal_top_n" => format_top_n(result, "withdrawal"),
        "savings_deposit_monthly_breakdown" => {
            format_monthly_breakdown(result, "deposit", "total_deposit_amount", "deposit_count")
        }
        "savings_deposit_monthly_top_n" => format_monthly_top_n(result, "deposit"),
        "savings_withdrawal_monthly_breakdown" => format_monthly_breakdown(
            result,
            "withdrawal",
            "total_withdrawal_amount",
            "withdrawal_count",
        ),
        "savings_withdrawal_monthly_top_n" => format_monthly_top_n(result, "withdrawal"),
        "savings_balance_summary" => {
            let first_row = first_row(result)?;
            Some(format!(
                "Active client-owned savings portfolio: {} account(s). Total balance {}. Average {}. Largest {}.",
                first_row.get("account_count")?.as_i64()?,
                first_row.get("total_balance")?.as_str()?,
                first_row.get("average_balance")?.as_str()?,
                first_row.get("max_balance")?.as_str()?,
            ))
        }
        _ => None,
    }
}

fn first_row(result: &Value) -> Option<&Value> {
    result.get("rows")?.as_array()?.first()
}

fn format_total(
    result: &Value,
    activity: &str,
    amount_field: &str,
    count_field: &str,
) -> Option<String> {
    let first_row = first_row(result)?;
    Some(format!(
        "The total savings {activity} from {} to {} is {} across {} {activity} transaction(s).",
        first_row.get("from_date")?.as_str()?,
        first_row.get("to_date")?.as_str()?,
        first_row.get(amount_field)?.as_str()?,
        first_row.get(count_field)?.as_i64()?,
    ))
}

fn format_top_n(result: &Value, activity: &str) -> Option<String> {
    let first_row = first_row(result)?;
    Some(format!(
        "Found {} savings {activity} transaction(s). The largest amount is {} on {}.",
        result.get("row_count")?.as_u64()?,
        first_row.get("amount")?.as_str()?,
        first_row.get("transaction_date")?.as_str()?,
    ))
}

fn format_monthly_top_n(result: &Value, activity: &str) -> Option<String> {
    let rows = result.get("rows")?.as_array()?;
    if rows.is_empty() {
        return Some(format!(
            "No savings {activity} activity in the requested period."
        ));
    }
    // Group consecutive rows by month_start; SQL already ORDERs by month then amount DESC.
    let mut lines: Vec<String> = Vec::new();
    let mut last_month: Option<&str> = None;
    let mut month_count = 0usize;
    for row in rows.iter().take(120) {
        let month = row
            .get("month_start")
            .and_then(Value::as_str)
            .unwrap_or("?");
        if last_month != Some(month) {
            month_count += 1;
            lines.push(format!("{month}:"));
            last_month = Some(month);
        }
        let amount = row.get("amount").and_then(Value::as_str).unwrap_or("0");
        let date = row
            .get("transaction_date")
            .and_then(Value::as_str)
            .unwrap_or("?");
        lines.push(format!("  - {amount} on {date}"));
    }
    if rows.len() > 120 {
        lines.push(format!("... and {} more transaction(s).", rows.len() - 120));
    }
    let header = format!(
        "Top savings {activity}s per month ({month_count} month(s), {} transaction(s)):",
        rows.len()
    );
    let mut out = vec![header];
    out.extend(lines);
    Some(out.join("\n"))
}

fn format_monthly_breakdown(
    result: &Value,
    activity: &str,
    amount_field: &str,
    count_field: &str,
) -> Option<String> {
    let rows = result.get("rows")?.as_array()?;
    if rows.is_empty() {
        return Some(format!(
            "No savings {activity} activity in the requested period."
        ));
    }
    let mut lines = vec![format!(
        "Savings {activity} by month ({} month(s)):",
        rows.len()
    )];
    for row in rows.iter().take(24) {
        let month = row
            .get("month_start")
            .and_then(Value::as_str)
            .unwrap_or("?");
        let amount = row.get(amount_field).and_then(Value::as_str).unwrap_or("0");
        let count = row.get(count_field).and_then(Value::as_i64).unwrap_or(0);
        lines.push(format!(
            "- {month}: {amount} across {count} transaction(s)."
        ));
    }
    if rows.len() > 24 {
        lines.push(format!("... and {} more month(s).", rows.len() - 24));
    }
    Some(lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::chat::planner::{ExecutionPlan, ExecutionPlanType};

    #[test]
    fn formats_total_response() {
        let plan = ExecutionPlan {
            plan_type: ExecutionPlanType::Atomic,
            domain: "savings".to_string(),
            capability: "savings_deposit_total".to_string(),
            query_id: "savings.deposit_total".to_string(),
            output_mode: "total".to_string(),
            params: json!({}),
            requires_policy_check: true,
        };
        let result = json!({
            "rows": [{
                "from_date": "2026-06-01",
                "to_date": "2026-06-21",
                "total_deposit_amount": "200.000000",
                "deposit_count": 2
            }]
        });

        assert_eq!(
            format_report_response(&plan, &result).as_deref(),
            Some(
                "The total savings deposit from 2026-06-01 to 2026-06-21 is 200.000000 across 2 deposit transaction(s)."
            )
        );
    }

    #[test]
    fn formats_withdrawal_monthly_breakdown_empty_response() {
        let plan = ExecutionPlan {
            plan_type: ExecutionPlanType::Atomic,
            domain: "savings".to_string(),
            capability: "savings_withdrawal_monthly_breakdown".to_string(),
            query_id: "savings.withdrawal_monthly_breakdown".to_string(),
            output_mode: "monthly_breakdown".to_string(),
            params: json!({}),
            requires_policy_check: true,
        };

        assert_eq!(
            format_report_response(&plan, &json!({ "rows": [] })).as_deref(),
            Some("No savings withdrawal activity in the requested period.")
        );
    }
}
