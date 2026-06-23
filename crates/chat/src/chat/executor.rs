use anyhow::{Context, Result, bail};
use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde_json::{Value, json};
use sqlx::{PgPool, Row};

use crate::chat::planner::{ExecutionPlan, PolicyDecision, PolicyDecisionStatus};
use crate::knowledge::model::KnowledgeCatalog;

pub async fn execute_plan(
    pool: &PgPool,
    catalog: &KnowledgeCatalog,
    plan: &ExecutionPlan,
    policy: &PolicyDecision,
) -> Result<Value> {
    if policy.status != PolicyDecisionStatus::Allowed {
        bail!(
            "policy blocked execution: {}",
            policy.reason.as_deref().unwrap_or("unknown policy error")
        );
    }

    let query = catalog
        .queries
        .iter()
        .find(|query| query.id == plan.query_id)
        .with_context(|| format!("query {} not found in catalog", plan.query_id))?;
    let sql = approved_sql(&query.id)?;
    let mut sql_query = sqlx::query(sql);

    for parameter in &query.parameters {
        match parameter.name.as_str() {
            "from_date" | "to_date" => {
                let value = plan
                    .params
                    .get(&parameter.name)
                    .and_then(Value::as_str)
                    .with_context(|| format!("missing parameter {}", parameter.name))?;
                sql_query = sql_query.bind(NaiveDate::parse_from_str(value, "%Y-%m-%d")?);
            }
            "office_ids" => {
                sql_query = sql_query.bind(policy.office_ids.clone());
            }
            "limit" => {
                let value = plan
                    .params
                    .get("limit")
                    .and_then(Value::as_u64)
                    .context("missing parameter limit")?;
                sql_query = sql_query.bind(value as i64);
            }
            "currency_code" => {
                let value = plan.params.get("currency_code").and_then(Value::as_str);
                sql_query = sql_query.bind(value);
            }
            "product_ids" => {
                let value = plan.params.get("product_ids").and_then(|value| {
                    value
                        .as_array()
                        .map(|items| items.iter().filter_map(Value::as_i64).collect::<Vec<_>>())
                });
                sql_query = sql_query.bind(value);
            }
            other => bail!("unsupported query parameter {other}"),
        }
    }

    let rows = sql_query.fetch_all(pool).await?;
    let mut result_rows = Vec::with_capacity(rows.len());

    for row in rows {
        let mut result_row = serde_json::Map::new();
        for field in &query.output_fields {
            let value = match field.kind.as_str() {
                "date" => row
                    .try_get::<Option<NaiveDate>, _>(field.name.as_str())?
                    .map(|value| json!(value.to_string()))
                    .unwrap_or(Value::Null),
                "decimal" => row
                    .try_get::<Option<Decimal>, _>(field.name.as_str())?
                    .map(|value| json!(value.to_string()))
                    .unwrap_or(Value::Null),
                "integer" | "bigint" => row
                    .try_get::<Option<i64>, _>(field.name.as_str())?
                    .map(|value| json!(value))
                    .unwrap_or(Value::Null),
                "string" => row
                    .try_get::<Option<String>, _>(field.name.as_str())?
                    .map(Value::String)
                    .unwrap_or(Value::Null),
                other => bail!("unsupported output field type {other}"),
            };
            result_row.insert(field.name.clone(), value);
        }
        result_rows.push(Value::Object(result_row));
    }

    Ok(json!({
        "query_id": query.id,
        "row_count": result_rows.len(),
        "rows": result_rows,
    }))
}

fn approved_sql(query_id: &str) -> Result<&'static str> {
    match query_id {
        "savings.deposit_total" => Ok(include_str!(
            "../../../../queries/savings/deposit_total.sql"
        )),
        "savings.deposit_top_n" => Ok(include_str!(
            "../../../../queries/savings/deposit_top_n.sql"
        )),
        other => bail!("query {other} has no approved SQL binding"),
    }
}
