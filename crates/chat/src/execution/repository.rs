use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, bail};
use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde_json::{Value, json};
use sqlx::{
    AssertSqlSafe, PgPool, Postgres, Row, SqlSafeStr,
    postgres::{PgArguments, PgRow},
};

use crate::assistant::execution::plan::{ExecutionPlan, PolicyDecision, PolicyDecisionStatus};
use crate::knowledge::dataset::compose::compose;
use crate::knowledge::dataset::model::FilterOperator;
use crate::knowledge::model::{KnowledgeCatalog, QueryOutputField, QueryParameter};

/// Execution ceilings resolved from config and carried into the SQL layer.
#[derive(Debug, Clone, Copy)]
pub struct ExecutionLimits {
    pub default_timeout_ms: u64,
    pub global_max_rows: i64,
}

impl Default for ExecutionLimits {
    // Fallback for canonical-absent execution; real requests carry the configured
    // values via CanonicalRuntimeContext.
    // ponytail: mirrors the QueryConfig env defaults.
    fn default() -> Self {
        Self {
            default_timeout_ms: 3_000,
            global_max_rows: 50_000,
        }
    }
}

/// Row ceiling for a capability's limit/top_n parameter: its declared hard_cap
/// if present, else the configured global backstop.
pub(crate) fn effective_row_cap(declared_hard_cap: Option<i64>, global_max_rows: i64) -> i64 {
    declared_hard_cap.unwrap_or(global_max_rows)
}

/// Return the bound to probe only when the cap replaces a missing or over-cap
/// request. A within-cap `limit` may be a per-group rank rather than a global
/// row count (for example, monthly top-N), so it must retain its SQL semantics.
fn truncation_limit(requested: Option<i64>, row_cap: i64) -> Option<i64> {
    match requested {
        Some(requested) if requested <= row_cap => None,
        _ => Some(row_cap),
    }
}

/// PostgreSQL SQLSTATE 57014 means `query_canceled` (statement_timeout fired).
fn is_statement_timeout(code: Option<&str>) -> bool {
    code == Some("57014")
}

fn is_supported_output_field_type(kind: &str) -> bool {
    matches!(
        kind,
        "date" | "decimal" | "integer" | "bigint" | "string" | "boolean"
    )
}

/// On SQLSTATE 57014 (statement_timeout), return a sanitized error and no rows.
async fn fetch_all_with_timeout<'q>(
    pool: &PgPool,
    query: sqlx::query::Query<'q, Postgres, PgArguments>,
    timeout_ms: u64,
) -> Result<Vec<PgRow>> {
    let mut tx = pool.begin().await?;
    // timeout_ms is a trusted integer from config/YAML, never user input.
    sqlx::query(
        AssertSqlSafe(format!("SET LOCAL statement_timeout = {timeout_ms}")).into_sql_str(),
    )
    .execute(&mut *tx)
    .await?;
    let outcome = query.fetch_all(&mut *tx).await;
    let _ = tx.rollback().await; // Read-only: never commit the execution transaction.
    match outcome {
        Ok(rows) => Ok(rows),
        Err(error) => {
            let code = error.as_database_error().and_then(|db| db.code());
            if is_statement_timeout(code.as_deref()) {
                // Sanitized: no SQL, parameters, or SQLSTATE leak to the client.
                bail!("execution_timed_out");
            }
            Err(error.into())
        }
    }
}

pub async fn execute_plan(
    pool: &PgPool,
    catalog: &KnowledgeCatalog,
    plan: &ExecutionPlan,
    policy: &PolicyDecision,
    limits: ExecutionLimits,
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
    let (sql, parameters, output_fields, timeout_ms, applied_filters) =
        resolve_statement(catalog, plan, query, limits.default_timeout_ms)?;
    tracing::info!(
        query_id = %plan.query_id,
        capability = %plan.capability,
        dataset_id = plan.dataset_selection.as_ref().map(|selection| selection.dataset_id.as_str()),
        bind_count = parameters.len(),
        "executing approved statement",
    );
    let declared_hard_cap = catalog
        .capabilities
        .iter()
        .find(|capability| capability.id == plan.capability)
        .and_then(|capability| {
            capability
                .parameter_policies
                .iter()
                .find(|policy| matches!(policy.name.as_str(), "limit" | "top_n"))
                .and_then(|policy| policy.hard_cap)
        });
    let row_cap = effective_row_cap(declared_hard_cap, limits.global_max_rows);
    let limit_param = parameters
        .iter()
        .find(|parameter| matches!(parameter.name.as_str(), "limit" | "top_n"))
        .map(|parameter| parameter.name.as_str());
    // Fetch one extra row only when the cap replaces the requested bind. A
    // within-cap limit can be a per-group rank, so probing it as a global row
    // ceiling would change approved-query semantics.
    let fetch_limit = limit_param
        .and_then(|name| truncation_limit(plan.params.get(name).and_then(Value::as_i64), row_cap));

    let mut sql_query = sqlx::query(AssertSqlSafe(sql).into_sql_str());
    for parameter in &parameters {
        match parameter.kind.as_str() {
            "date" => sql_query = sql_query.bind(date_param(plan, parameter)?),
            "integer" => {
                let value = if Some(parameter.name.as_str()) == limit_param {
                    match fetch_limit {
                        Some(limit) => Some(limit.saturating_add(1)),
                        None => integer_param(plan, parameter)?,
                    }
                } else {
                    integer_param(plan, parameter)?
                };
                sql_query = sql_query.bind(value);
            }
            "string" => sql_query = sql_query.bind(string_param(plan, parameter)?),
            "decimal" => sql_query = sql_query.bind(decimal_param(plan, parameter)?),
            "array_bigint" => {
                sql_query = sql_query.bind(array_bigint_param(plan, policy, parameter)?)
            }
            other => bail!("unsupported query parameter {other}"),
        }
    }

    if let Some(selection) = plan.dataset_selection.as_ref() {
        let dataset = catalog
            .datasets
            .iter()
            .find(|dataset| dataset.id == selection.dataset_id)
            .context("dataset selection not found in catalog")?;
        for bind in compose_dataset_binds(dataset, selection)? {
            sql_query = bind_dataset_value(sql_query, &bind)?;
        }
    }

    let mut rows = fetch_all_with_timeout(pool, sql_query, timeout_ms).await?;
    let (truncated, shown) = match fetch_limit {
        Some(limit) if rows.len() as i64 > limit => (true, limit),
        _ => (false, rows.len() as i64),
    };
    if truncated {
        rows.truncate(shown as usize);
    }
    let mut result_rows = Vec::with_capacity(rows.len());

    for row in rows {
        let mut result_row = serde_json::Map::new();
        for field in &output_fields {
            if !is_supported_output_field_type(field.kind.as_str()) {
                bail!("unsupported output field type {}", field.kind);
            }
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
                "boolean" => row
                    .try_get::<Option<bool>, _>(field.name.as_str())?
                    .map(Value::Bool)
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
        "truncated": truncated,
        "shown": shown,
        "applied_filters": applied_filters,
    }))
}

fn resolve_statement(
    catalog: &KnowledgeCatalog,
    plan: &ExecutionPlan,
    query: &crate::knowledge::model::QueryKnowledge,
    default_timeout_ms: u64,
) -> Result<(
    String,
    Vec<QueryParameter>,
    Vec<QueryOutputField>,
    u64,
    Vec<String>,
)> {
    let Some(selection) = plan.dataset_selection.as_ref() else {
        return Ok((
            read_approved_sql(catalog, query.sql_file.as_str())?,
            query.parameters.clone(),
            query.output_fields.clone(),
            query.timeout_ms.unwrap_or(default_timeout_ms),
            Vec::new(),
        ));
    };
    let dataset = catalog
        .datasets
        .iter()
        .find(|dataset| dataset.id == selection.dataset_id)
        .context("dataset selection not found in catalog")?;
    let shape = dataset
        .shape(&selection.shape_id)
        .context("dataset shape not found in catalog")?;
    let source = read_approved_sql(catalog, &dataset.source_sql)?;
    let fragment = shape
        .fragment
        .as_deref()
        .map(|path| read_approved_sql(catalog, path))
        .transpose()?;
    let composed = compose(
        dataset,
        &selection.shape_id,
        selection.order_by_id.as_deref(),
        &source,
        fragment.as_deref(),
    )?;
    let output_fields = shape
        .output_fields(dataset)
        .iter()
        .map(|field| QueryOutputField {
            name: field.name.clone(),
            kind: field.kind.clone(),
            sensitivity: field.sensitivity,
        })
        .collect();
    Ok((
        composed.sql,
        shape.parameters(dataset).to_vec(),
        output_fields,
        dataset.timeout_ms.unwrap_or(default_timeout_ms),
        selection
            .filters
            .iter()
            .map(|filter| filter.filter_id.clone())
            .collect(),
    ))
}

#[derive(Debug)]
struct DatasetBindValue {
    kind: String,
    value: Option<Value>,
}

fn compose_dataset_binds(
    dataset: &crate::knowledge::dataset::model::DatasetKnowledge,
    selection: &crate::knowledge::dataset::model::DatasetSelection,
) -> Result<Vec<DatasetBindValue>> {
    let mut binds = Vec::new();
    for slot in &dataset.filters {
        for operator in &slot.operators {
            let selected = selection
                .filters
                .iter()
                .find(|filter| filter.filter_id == slot.id && filter.operator == *operator);
            if *operator == FilterOperator::Between {
                let values = selected.and_then(|filter| filter.value.as_array());
                for index in 0..2 {
                    binds.push(DatasetBindValue {
                        kind: slot.kind.clone(),
                        value: values.and_then(|values| values.get(index)).cloned(),
                    });
                }
            } else {
                binds.push(DatasetBindValue {
                    kind: slot.kind.clone(),
                    value: selected.map(|filter| filter.value.clone()),
                });
            }
        }
    }
    Ok(binds)
}

fn bind_dataset_value<'q>(
    query: sqlx::query::Query<'q, Postgres, PgArguments>,
    bind: &DatasetBindValue,
) -> Result<sqlx::query::Query<'q, Postgres, PgArguments>> {
    match bind.kind.as_str() {
        "date" => Ok(query.bind(
            bind.value
                .as_ref()
                .and_then(Value::as_str)
                .map(|value| NaiveDate::parse_from_str(value, "%Y-%m-%d"))
                .transpose()?,
        )),
        "integer" => Ok(query.bind(bind.value.as_ref().and_then(Value::as_i64))),
        "boolean" => Ok(query.bind(bind.value.as_ref().and_then(Value::as_bool))),
        "string" => Ok(query.bind(
            bind.value
                .as_ref()
                .and_then(Value::as_str)
                .map(str::to_owned),
        )),
        "decimal" => Ok(query.bind(
            bind.value
                .as_ref()
                .and_then(Value::as_str)
                .map(str::parse::<Decimal>)
                .transpose()?,
        )),
        other => bail!("unsupported dataset filter bind type {other}"),
    }
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

fn decimal_param(plan: &ExecutionPlan, parameter: &QueryParameter) -> Result<Option<Decimal>> {
    let Some(value) = plan.params.get(&parameter.name).and_then(Value::as_str) else {
        return required_or_null(parameter);
    };
    Ok(Some(value.parse()?))
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
        EvidenceEvaluation, ExecutionPlan, ExecutionPlanType, RetrievalPlan,
    };
    use crate::knowledge::model::{QueryOutputField, QueryParameter, Sensitivity};

    #[test]
    fn effective_row_cap_prefers_declared_hard_cap() {
        assert_eq!(super::effective_row_cap(Some(100), 50_000), 100);
    }

    #[test]
    fn effective_row_cap_falls_back_to_backstop() {
        assert_eq!(super::effective_row_cap(None, 50_000), 50_000);
    }

    #[test]
    fn truncation_limit_only_applies_when_the_cap_replaces_the_request() {
        assert_eq!(super::truncation_limit(Some(2), 100), None);
        assert_eq!(super::truncation_limit(Some(i64::MAX), 100), Some(100));
        assert_eq!(super::truncation_limit(None, 100), Some(100));
    }

    #[test]
    fn statement_timeout_sqlstate_is_recognized() {
        assert!(super::is_statement_timeout(Some("57014")));
        assert!(!super::is_statement_timeout(Some("42P01")));
        assert!(!super::is_statement_timeout(None));
    }

    #[tokio::test]
    async fn statement_timeout_cancels_slow_query() {
        let Ok(url) = std::env::var("FINERACT_DATABASE_URL") else {
            eprintln!("skipping: FINERACT_DATABASE_URL unset");
            return;
        };
        let pool = sqlx::PgPool::connect(&url).await.expect("connect fineract");
        let error = super::fetch_all_with_timeout(&pool, sqlx::query("SELECT pg_sleep(0.2)"), 1)
            .await
            .expect_err("1ms budget must trip on a 200ms sleep");
        let message = error.to_string();
        assert_eq!(message, "execution_timed_out");
        assert!(!message.contains("pg_sleep"), "error must not leak SQL");
    }

    #[test]
    fn approved_boolean_output_field_is_supported() {
        let field = QueryOutputField {
            name: "is_penalty".to_string(),
            kind: "boolean".to_string(),
            sensitivity: Sensitivity::PublicBusiness,
        };

        assert!(super::is_supported_output_field_type(field.kind.as_str()));
    }

    #[test]
    fn optional_integer_param_returns_none_when_missing() {
        let plan = ExecutionPlan {
            plan_type: ExecutionPlanType::Atomic,
            domain: "savings".to_string(),
            capability: "savings_activity_list".to_string(),
            query_id: "savings.activity_list".to_string(),
            dataset_selection: None,
            output_mode: "list".to_string(),
            params: serde_json::json!({}),
            retrieval_plan: RetrievalPlan::default(),
            evidence_evaluation: EvidenceEvaluation::default(),
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
