use std::collections::BTreeMap;

use chrono::{Datelike, NaiveDate};
use rust_decimal::Decimal;
use serde_json::{Map, Value, json};

use crate::chat::planner::ExecutionPlan;
use crate::knowledge::model::KnowledgeCatalog;

#[derive(Debug, Clone)]
struct Bucket {
    id: String,
    label: String,
}

#[derive(Debug, Default, Clone)]
struct BucketTotals {
    count: usize,
    amount: Decimal,
}

impl BucketTotals {
    fn add(&mut self, amount: Decimal) {
        self.count += 1;
        self.amount += amount;
    }
}

pub fn render(catalog: &KnowledgeCatalog, plan: &ExecutionPlan, rows: &[Value]) -> Option<String> {
    if rows.is_empty() {
        return None;
    }

    let bucket_config = BucketConfig::from_catalog(catalog);
    let mut labels = BTreeMap::new();
    let mut by_currency: BTreeMap<String, BTreeMap<String, BucketTotals>> = BTreeMap::new();
    let mut bucket_rows: BTreeMap<String, BTreeMap<String, Vec<&Value>>> = BTreeMap::new();
    let mut offices = BTreeMap::<i64, String>::new();
    let mut per_day: BTreeMap<NaiveDate, BTreeMap<String, BTreeMap<String, Decimal>>> =
        BTreeMap::new();

    for row in rows {
        let bucket = bucket_config.bucket_for(row);
        let currency = row_currency(row).to_string();
        let amount = amount(row);
        labels
            .entry(bucket.id.clone())
            .or_insert(bucket.label.clone());
        by_currency
            .entry(currency.clone())
            .or_default()
            .entry(bucket.id.clone())
            .or_default()
            .add(amount);
        bucket_rows
            .entry(bucket.id.clone())
            .or_default()
            .entry(currency.clone())
            .or_default()
            .push(row);
        if let Some(date) = row_date(row) {
            *per_day
                .entry(date)
                .or_default()
                .entry(currency)
                .or_default()
                .entry(bucket.id)
                .or_default() += amount;
        }
        if let Some(office_id) = row.get("office_id").and_then(Value::as_i64) {
            offices.insert(
                office_id,
                row.get("office_name")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown")
                    .to_string(),
            );
        }
    }

    let message = render_message(plan, rows, &bucket_rows, &by_currency, &labels, &offices);

    let mut per_week: BTreeMap<NaiveDate, BTreeMap<String, BTreeMap<String, Decimal>>> =
        BTreeMap::new();
    for (day, per_currency) in &per_day {
        let days_from_monday = day.weekday().num_days_from_monday() as i64;
        let week_start = *day - chrono::Duration::days(days_from_monday);
        let entry = per_week.entry(week_start).or_default();
        for (currency, per_bucket) in per_currency {
            for (bucket, amount) in per_bucket {
                *entry
                    .entry(currency.clone())
                    .or_default()
                    .entry(bucket.clone())
                    .or_default() += *amount;
            }
        }
    }

    serde_json::to_string(&json!({
        "answer_plan": {
            "capability": plan.capability,
            "sections": plan.answer_plan.sections,
            "coverage": coverage(plan, rows, &by_currency, &offices),
        },
        "structured": {
            "by_currency": structured_by_currency(&by_currency),
            "rows": rows,
            "weekly_aggregation": weekly_aggregation(&per_week),
            "period_aggregation": { "buckets": [] },
        },
        "message": message,
    }))
    .ok()
}

#[derive(Debug)]
struct BucketConfig {
    by_enum: BTreeMap<i64, Bucket>,
    fallback: Bucket,
}

impl BucketConfig {
    fn from_catalog(catalog: &KnowledgeCatalog) -> Self {
        let mut by_enum = BTreeMap::new();
        let mut fallback = Bucket {
            id: "other".to_string(),
            label: "Other activity".to_string(),
        };
        let Some(schema) = catalog
            .schemas
            .iter()
            .find(|item| item.id == "fineract.enums.savings_transaction_type")
        else {
            return Self { by_enum, fallback };
        };
        if let Some(item) = schema.content.get("fallback").and_then(Value::as_object) {
            fallback = Bucket {
                id: item
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or("other")
                    .to_string(),
                label: item
                    .get("label")
                    .and_then(Value::as_str)
                    .unwrap_or("Other activity")
                    .to_string(),
            };
        }
        if let Some(buckets) = schema.content.get("buckets").and_then(Value::as_object) {
            for (id, bucket) in buckets {
                let label = bucket
                    .get("label")
                    .and_then(Value::as_str)
                    .unwrap_or(id)
                    .to_string();
                if let Some(enums) = bucket.get("enums").and_then(Value::as_array) {
                    for enum_value in enums.iter().filter_map(Value::as_i64) {
                        by_enum.insert(
                            enum_value,
                            Bucket {
                                id: id.clone(),
                                label: label.clone(),
                            },
                        );
                    }
                }
            }
        }
        Self { by_enum, fallback }
    }

    fn bucket_for(&self, row: &Value) -> Bucket {
        row.get("transaction_type_enum")
            .and_then(Value::as_i64)
            .and_then(|value| self.by_enum.get(&value))
            .cloned()
            .unwrap_or_else(|| self.fallback.clone())
    }
}

fn coverage(
    plan: &ExecutionPlan,
    rows: &[Value],
    by_currency: &BTreeMap<String, BTreeMap<String, BucketTotals>>,
    offices: &BTreeMap<i64, String>,
) -> Value {
    let limit = plan.params.get("limit").and_then(Value::as_i64);
    json!({
        "requested_range": {
            "from": plan.params.get("from_date").and_then(Value::as_str),
            "to": plan.params.get("to_date").and_then(Value::as_str),
        },
        "returned_rows": rows.len(),
        "limit_applied": limit,
        "truncated": limit.is_some_and(|limit| rows.len() as i64 == limit),
        "known_total_rows": null,
        "currencies_returned": by_currency.keys().collect::<Vec<_>>(),
        "offices_returned": offices.keys().collect::<Vec<_>>(),
    })
}

fn structured_by_currency(by_currency: &BTreeMap<String, BTreeMap<String, BucketTotals>>) -> Value {
    let mut root = Map::new();
    for (currency, buckets) in by_currency {
        let mut bucket_values = Map::new();
        for (bucket, total) in buckets {
            bucket_values.insert(
                bucket.clone(),
                json!({ "count": total.count, "total": total.amount.to_string() }),
            );
        }
        root.insert(currency.clone(), Value::Object(bucket_values));
    }
    Value::Object(root)
}

fn weekly_aggregation(
    per_week: &BTreeMap<NaiveDate, BTreeMap<String, BTreeMap<String, Decimal>>>,
) -> Value {
    Value::Array(
        per_week
            .iter()
            .map(|(week_start, by_currency)| {
                json!({
                    "week_start": week_start.to_string(),
                    "week_end": (*week_start + chrono::Duration::days(6)).to_string(),
                    "by_currency": decimal_map(by_currency),
                })
            })
            .collect(),
    )
}

fn decimal_map(value: &BTreeMap<String, BTreeMap<String, Decimal>>) -> Value {
    let mut root = Map::new();
    for (currency, buckets) in value {
        let mut bucket_values = Map::new();
        for (bucket, amount) in buckets {
            bucket_values.insert(bucket.clone(), json!(amount.to_string()));
        }
        root.insert(currency.clone(), Value::Object(bucket_values));
    }
    Value::Object(root)
}

fn render_message(
    plan: &ExecutionPlan,
    rows: &[Value],
    bucket_rows: &BTreeMap<String, BTreeMap<String, Vec<&Value>>>,
    by_currency: &BTreeMap<String, BTreeMap<String, BucketTotals>>,
    labels: &BTreeMap<String, String>,
    offices: &BTreeMap<i64, String>,
) -> String {
    let mut sections = vec![header(plan, rows, by_currency, offices)];
    for (bucket, rows_by_currency) in bucket_rows {
        let label = labels.get(bucket).map(String::as_str).unwrap_or(bucket);
        let mut section = format!("\n### {label}");
        for (currency, rows) in rows_by_currency {
            let total = by_currency
                .get(currency)
                .and_then(|buckets| buckets.get(bucket))
                .cloned()
                .unwrap_or_default();
            section.push_str(&format!(
                "\n\n#### {} ({} transactions, total {})",
                currency, total.count, total.amount
            ));
            for (index, row) in rows.iter().take(50).enumerate() {
                section.push_str(&format!("\n{}. {}", index + 1, format_row_line(row)));
            }
        }
        sections.push(section);
    }
    sections.join("\n")
}

fn header(
    plan: &ExecutionPlan,
    rows: &[Value],
    by_currency: &BTreeMap<String, BTreeMap<String, BucketTotals>>,
    offices: &BTreeMap<i64, String>,
) -> String {
    let from = plan
        .params
        .get("from_date")
        .and_then(Value::as_str)
        .unwrap_or("");
    let to = plan
        .params
        .get("to_date")
        .and_then(Value::as_str)
        .unwrap_or("");
    let office_names = offices.values().cloned().collect::<Vec<_>>().join(", ");
    let currencies = by_currency.keys().cloned().collect::<Vec<_>>().join(", ");
    let mut message = format!(
        "Savings activity from **{from}** to **{to}**, across **{} office(s)** ({}), in currencies **{}**. Showing **{}** transactions.",
        offices.len(),
        office_names,
        currencies,
        rows.len()
    );
    if plan
        .params
        .get("limit")
        .and_then(Value::as_i64)
        .is_some_and(|limit| rows.len() as i64 == limit)
    {
        message.push_str(&format!(
            " Result limited by `limit={}`; narrow the date range or raise the limit to see more.",
            rows.len()
        ));
    }
    message
}

fn row_currency(row: &Value) -> &str {
    row.get("currency_code")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("unknown")
}

fn row_date(row: &Value) -> Option<NaiveDate> {
    row.get("transaction_date")
        .and_then(Value::as_str)
        .and_then(|date| NaiveDate::parse_from_str(date, "%Y-%m-%d").ok())
}

fn amount(row: &Value) -> Decimal {
    row.get("amount")
        .and_then(|value| match value {
            Value::String(s) => s.parse::<Decimal>().ok(),
            Value::Number(n) => n.as_f64().and_then(|f| Decimal::try_from(f).ok()),
            _ => None,
        })
        .unwrap_or(Decimal::ZERO)
}

fn format_currency(currency: &str) -> String {
    if currency == "unknown" {
        String::new()
    } else {
        format!("{currency} ")
    }
}

fn format_row_line(row: &Value) -> String {
    let date = row
        .get("transaction_date")
        .and_then(Value::as_str)
        .unwrap_or("");
    let amount = row
        .get("amount")
        .map(|v| {
            v.as_str()
                .map(String::from)
                .unwrap_or_else(|| v.to_string())
        })
        .unwrap_or_default();
    let currency = row
        .get("currency_code")
        .and_then(Value::as_str)
        .unwrap_or("");
    let office = row.get("office_name").and_then(Value::as_str).unwrap_or("");
    let product = row
        .get("product_name")
        .and_then(Value::as_str)
        .unwrap_or("");
    let mut line = format!("{date} — {}{amount}", format_currency(currency));
    if !product.is_empty() {
        line.push_str(&format!(" ({product}"));
        if !office.is_empty() {
            line.push_str(&format!(", office: {office}"));
        }
        line.push(')');
    } else if !office.is_empty() {
        line.push_str(&format!(" (office: {office})"));
    }
    line
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn empty_rows_returns_none() {
        assert!(render(&test_catalog(), &test_plan(), &[]).is_none());
    }

    #[test]
    fn buckets_deposits_withdrawals_charges_and_computes_totals() {
        let catalog = test_catalog();
        let plan = test_plan();
        let rows = vec![
            json!({
                "transaction_date": "2026-05-05",
                "transaction_type_enum": 1,
                "amount": "100.00",
                "currency_code": "IDR",
                "office_name": "Head Office",
                "product_name": "Basic Savings",
            }),
            json!({
                "transaction_date": "2026-05-05",
                "transaction_type_enum": 2,
                "amount": "40.00",
                "currency_code": "IDR",
                "office_name": "Head Office",
                "product_name": "Basic Savings",
            }),
            json!({
                "transaction_date": "2026-05-07",
                "transaction_type_enum": 4,
                "amount": "1.50",
                "currency_code": "IDR",
                "office_name": "Head Office",
                "product_name": "Basic Savings",
            }),
        ];
        let out = render(&catalog, &plan, &rows).expect("render");
        let payload: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(
            payload["structured"]["by_currency"]["IDR"]["deposits"]["total"],
            "100.00"
        );
        assert_eq!(
            payload["structured"]["by_currency"]["IDR"]["withdrawals"]["total"],
            "40.00"
        );
        assert_eq!(
            payload["structured"]["by_currency"]["IDR"]["charges_paid"]["total"],
            "1.50"
        );
        assert!(
            payload["message"]
                .as_str()
                .unwrap()
                .contains("### Charges paid")
        );
    }

    fn test_catalog() -> KnowledgeCatalog {
        KnowledgeCatalog {
            root_path: Default::default(),
            query_path: Default::default(),
            data_areas: Vec::new(),
            domains: Vec::new(),
            schemas: vec![crate::knowledge::model::GenericKnowledge {
                id: "fineract.enums.savings_transaction_type".to_string(),
                status: None,
                domain: None,
                data_areas: Vec::new(),
                checks: Vec::new(),
                content: serde_json::from_value(json!({
                    "buckets": {
                        "deposits": { "enums": [1], "label": "Deposits" },
                        "withdrawals": { "enums": [2], "label": "Withdrawals" },
                        "charges_paid": { "enums": [4], "label": "Charges paid" }
                    },
                    "fallback": { "id": "other", "label": "Other activity" }
                }))
                .unwrap(),
            }],
            metrics: Vec::new(),
            capabilities: Vec::new(),
            queries: Vec::new(),
            policies: Vec::new(),
            responses: Vec::new(),
            classification: Default::default(),
        }
    }

    fn test_plan() -> ExecutionPlan {
        ExecutionPlan {
            plan_type: crate::chat::planner::ExecutionPlanType::Atomic,
            domain: "savings".to_string(),
            capability: "savings_activity_list".to_string(),
            query_id: "savings.activity_list".to_string(),
            output_mode: "list".to_string(),
            params: json!({ "from_date": "2026-05-05", "to_date": "2026-05-07", "limit": 10 }),
            retrieval_plan: Default::default(),
            evidence_evaluation: Default::default(),
            answer_plan: crate::chat::planner::AnswerPlan::default(),
            requires_policy_check: true,
        }
    }
}
