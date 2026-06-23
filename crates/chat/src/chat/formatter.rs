use serde_json::Value;

use crate::chat::planner::ExecutionPlan;

pub fn format_report_response(plan: &ExecutionPlan, result: &Value) -> Option<String> {
    let first_row = result.get("rows")?.as_array()?.first()?;

    match plan.capability.as_str() {
        "savings_deposit_total" => Some(format!(
            "The total savings deposit from {} to {} is {} across {} deposit transaction(s).",
            first_row.get("from_date")?.as_str()?,
            first_row.get("to_date")?.as_str()?,
            first_row.get("total_deposit_amount")?.as_str()?,
            first_row.get("deposit_count")?.as_i64()?,
        )),
        "savings_deposit_top_n" => Some(format!(
            "Found {} savings deposit transaction(s). The largest amount is {} on {}.",
            result.get("row_count")?.as_u64()?,
            first_row.get("amount")?.as_str()?,
            first_row.get("transaction_date")?.as_str()?,
        )),
        _ => None,
    }
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
}
