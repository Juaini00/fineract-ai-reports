use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, bail};
use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde_json::{Value, json};
use sqlx::{AssertSqlSafe, PgPool, Row, SqlSafeStr};

use crate::assistant::execution::plan::{ExecutionPlan, PolicyDecision, PolicyDecisionStatus};
use crate::knowledge::model::{KnowledgeCatalog, QueryParameter};

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
    let sql = read_approved_sql(catalog, query.sql_file.as_str())?;
    tracing::info!(
        query_id = %plan.query_id,
        capability = %plan.capability,
        sql_file = %query.sql_file,
        params = %plan.params,
        office_ids = ?policy.office_ids,
        sql = %sql,
        "executing approved SQL",
    );
    let mut sql_query = sqlx::query(AssertSqlSafe(sql).into_sql_str());

    for parameter in &query.parameters {
        match parameter.kind.as_str() {
            "date" => sql_query = sql_query.bind(date_param(plan, parameter)?),
            "integer" => sql_query = sql_query.bind(integer_param(plan, parameter)?),
            "string" => sql_query = sql_query.bind(string_param(plan, parameter)?),
            "array_bigint" => {
                sql_query = sql_query.bind(array_bigint_param(plan, policy, parameter)?)
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

fn date_param(plan: &ExecutionPlan, parameter: &QueryParameter) -> Result<Option<NaiveDate>> {
    let Some(value) = plan.params.get(&parameter.name).and_then(Value::as_str) else {
        return required_or_null(parameter);
    };

    Ok(Some(NaiveDate::parse_from_str(value, "%Y-%m-%d")?))
}

fn integer_param(plan: &ExecutionPlan, parameter: &QueryParameter) -> Result<Option<i64>> {
    let Some(value) = plan.params.get(&parameter.name).and_then(Value::as_i64) else {
        return required_or_null(parameter);
    };

    Ok(Some(value))
}

fn string_param(plan: &ExecutionPlan, parameter: &QueryParameter) -> Result<Option<String>> {
    let Some(value) = plan.params.get(&parameter.name).and_then(Value::as_str) else {
        return required_or_null(parameter);
    };

    Ok(Some(value.to_string()))
}

fn array_bigint_param(
    plan: &ExecutionPlan,
    policy: &PolicyDecision,
    parameter: &QueryParameter,
) -> Result<Option<Vec<i64>>> {
    if parameter.source.as_deref() == Some("authorized_scope") {
        return Ok(Some(policy.office_ids.clone()));
    }

    let Some(value) = plan.params.get(&parameter.name) else {
        return required_or_null(parameter);
    };
    let Some(items) = value.as_array() else {
        bail!("parameter {} must be an array", parameter.name);
    };

    let mut parsed = Vec::with_capacity(items.len());
    for item in items {
        let Some(value) = item.as_i64() else {
            bail!("parameter {} must contain only integers", parameter.name);
        };
        parsed.push(value);
    }

    Ok(Some(parsed))
}

fn required_or_null<T>(parameter: &QueryParameter) -> Result<Option<T>> {
    if parameter.required {
        bail!("missing parameter {}", parameter.name);
    }

    Ok(None)
}

fn read_approved_sql(catalog: &KnowledgeCatalog, sql_file: &str) -> Result<String> {
    let path = resolve_sql_path(&catalog.query_path, sql_file)?;
    std::fs::read_to_string(&path).with_context(|| format!("read approved SQL {}", path.display()))
}

fn resolve_sql_path(query_root: &Path, sql_file: &str) -> Result<PathBuf> {
    let relative = sql_file.strip_prefix("queries/").unwrap_or(sql_file);
    let path = Path::new(relative);
    if path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        bail!("invalid SQL file path");
    }

    Ok(query_root.join(path))
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{integer_param, resolve_sql_path};
    use crate::assistant::execution::plan::{
        AnswerPlan, EvidenceEvaluation, ExecutionPlan, ExecutionPlanType, RetrievalPlan,
    };
    use crate::knowledge::model::QueryParameter;

    #[test]
    fn optional_integer_param_returns_none_when_missing() {
        let plan = ExecutionPlan {
            plan_type: ExecutionPlanType::Atomic,
            domain: "savings".to_string(),
            capability: "savings_activity_list".to_string(),
            query_id: "savings.activity_list".to_string(),
            output_mode: "list".to_string(),
            params: serde_json::json!({}),
            retrieval_plan: RetrievalPlan::default(),
            evidence_evaluation: EvidenceEvaluation::default(),
            answer_plan: AnswerPlan::default(),
            requires_policy_check: true,
        };
        let parameter = QueryParameter {
            name: "limit".to_string(),
            kind: "integer".to_string(),
            required: false,
            source: None,
        };

        assert_eq!(integer_param(&plan, &parameter).unwrap(), None);
    }

    #[test]
    fn resolves_catalog_sql_path_under_query_root() {
        assert_eq!(
            resolve_sql_path(Path::new("/repo/queries"), "queries/savings/report.sql").unwrap(),
            Path::new("/repo/queries/savings/report.sql")
        );
    }

    #[test]
    fn rejects_sql_path_traversal() {
        assert!(resolve_sql_path(Path::new("/repo/queries"), "../secret.sql").is_err());
        assert!(resolve_sql_path(Path::new("/repo/queries"), "/tmp/secret.sql").is_err());
    }
}
