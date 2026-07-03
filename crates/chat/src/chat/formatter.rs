use serde_json::Value;

use crate::chat::planner::ExecutionPlan;

pub fn format_report_response(plan: &ExecutionPlan, result: &Value) -> Option<String> {
    match plan.capability.as_str() {
        "savings_deposit_total" => format_total(
            plan,
            result,
            "deposit",
            "total_deposit_amount",
            "deposit_count",
        ),
        "savings_deposit_top_n" => format_top_n(result, "deposit"),
        "savings_withdrawal_total" => format_total(
            plan,
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
                format_amount(
                    first_row.get("total_balance")?.as_str()?,
                    plan_currency(plan)
                ),
                format_amount(
                    first_row.get("average_balance")?.as_str()?,
                    plan_currency(plan)
                ),
                format_amount(first_row.get("max_balance")?.as_str()?, plan_currency(plan)),
            ))
        }
        _ => None,
    }
}

fn first_row(result: &Value) -> Option<&Value> {
    result.get("rows")?.as_array()?.first()
}

fn plan_currency(plan: &ExecutionPlan) -> Option<&str> {
    plan.params.get("currency_code").and_then(Value::as_str)
}

fn row_currency(row: &Value) -> Option<&str> {
    row.get("currency_code").and_then(Value::as_str)
}

fn format_amount(value: &str, currency_code: Option<&str>) -> String {
    match currency_code.filter(|currency| !currency.trim().is_empty()) {
        Some(currency) => format!("{currency} {value}"),
        None => value.to_string(),
    }
}

fn client_suffix(row: &Value) -> String {
    row.get("client_display_name")
        .and_then(Value::as_str)
        .filter(|name| !name.trim().is_empty())
        .map(|name| format!(" for {name}"))
        .unwrap_or_default()
}

fn format_total(
    plan: &ExecutionPlan,
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
    let first_row = rows.first()?;
    Some(format!(
        "The total savings {activity} from {} to {} is {} across {} {activity} transaction(s).",
        first_row.get("from_date")?.as_str()?,
        first_row.get("to_date")?.as_str()?,
        format_amount(first_row.get(amount_field)?.as_str()?, plan_currency(plan)).as_str(),
        first_row.get(count_field)?.as_i64()?,
    ))
}

fn format_top_n(result: &Value, activity: &str) -> Option<String> {
    let rows = result.get("rows")?.as_array()?;
    if rows.is_empty() {
        return Some(format!(
            "No savings {activity} activity in the requested period."
        ));
    }
    let first_row = rows.first()?;
    Some(format!(
        "Found {} savings {activity} transaction(s). The largest amount is {} on {}{}.",
        result.get("row_count")?.as_u64()?,
        format_amount(first_row.get("amount")?.as_str()?, row_currency(first_row)).as_str(),
        first_row.get("transaction_date")?.as_str()?,
        client_suffix(first_row),
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
        let amount = format_amount(
            row.get("amount").and_then(Value::as_str).unwrap_or("0"),
            row_currency(row),
        );
        let date = row
            .get("transaction_date")
            .and_then(Value::as_str)
            .unwrap_or("?");
        lines.push(format!("  - {amount} on {date}{}", client_suffix(row)));
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
        let amount = format_amount(
            row.get(amount_field).and_then(Value::as_str).unwrap_or("0"),
            row_currency(row),
        );
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
    fn formats_top_n_empty_response() {
        let plan = ExecutionPlan {
            plan_type: ExecutionPlanType::Atomic,
            domain: "savings".to_string(),
            capability: "savings_deposit_top_n".to_string(),
            query_id: "savings.deposit_top_n".to_string(),
            output_mode: "top_n".to_string(),
            params: json!({}),
            requires_policy_check: true,
        };

        assert_eq!(
            format_report_response(&plan, &json!({ "row_count": 0, "rows": [] })).as_deref(),
            Some("No savings deposit activity in the requested period.")
        );
    }

    #[test]
    fn formats_top_n_with_client_name_when_available() {
        let plan = ExecutionPlan {
            plan_type: ExecutionPlanType::Atomic,
            domain: "savings".to_string(),
            capability: "savings_deposit_top_n".to_string(),
            query_id: "savings.deposit_top_n".to_string(),
            output_mode: "top_n".to_string(),
            params: json!({}),
            requires_policy_check: true,
        };
        let result = json!({
            "row_count": 1,
            "rows": [{
                "amount": "25000000.000000",
                "currency_code": "USD",
                "transaction_date": "2026-06-21",
                "client_display_name": "Amina"
            }]
        });

        assert_eq!(
            format_report_response(&plan, &result).as_deref(),
            Some(
                "Found 1 savings deposit transaction(s). The largest amount is USD 25000000.000000 on 2026-06-21 for Amina."
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
