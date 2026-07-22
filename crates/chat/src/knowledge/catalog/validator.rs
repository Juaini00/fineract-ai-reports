use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

use anyhow::{Result, bail};
use sqlx::{AssertSqlSafe, Column, Executor, PgPool, SqlSafeStr, Statement};

use crate::assistant::ClarificationFieldType;
use crate::knowledge::model::{
    CapabilityKnowledge, GenericKnowledge, KnowledgeCatalog, ParameterInputKnowledge,
    QueryKnowledge,
};

const DATA_AREA_STATUSES: &[&str] = &[
    "included_mvp_foundation",
    "included_mvp_domain",
    "conditional",
    "secondary",
    "deferred",
    "deferred_group",
    "rejected",
    "rejected_group",
    "out_of_scope",
];
const DOMAIN_STATUSES: &[&str] = &["approved_mvp", "candidate", "deferred", "rejected"];
const CAPABILITY_STATUSES: &[&str] = &["approved_mvp", "candidate", "deferred", "rejected"];
const OUTPUT_MODES: &[&str] = &[
    "total",
    "top_n",
    "monthly_breakdown",
    "monthly_top_n",
    "list",
    "summary",
];
const QUERY_DATABASES: &[&str] = &["fineract", "app"];
const PARAMETER_TYPES: &[&str] = &["date", "integer", "string", "array_bigint"];
const SENSITIVITY_CLASSES: &[&str] = &[
    "public_business",
    "pii",
    "sensitive_business_identifier",
    "security_sensitive",
    "secret_never_expose",
    "free_text_sensitive",
];
const UNSAFE_SQL_COMMANDS: &[&str] = &[
    "INSERT", "UPDATE", "DELETE", "TRUNCATE", "DROP", "ALTER", "CREATE", "GRANT", "REVOKE", "COPY",
    "VACUUM", "ANALYZE",
];

pub struct KnowledgeValidator;

impl KnowledgeValidator {
    pub fn validate(catalog: &KnowledgeCatalog) -> Result<()> {
        let capability_ids: Vec<String> =
            catalog.capabilities.iter().map(|c| c.id.clone()).collect();
        validate_classification_policy_against_catalog(&catalog.classification, &capability_ids)?;

        validate_unique_ids(
            "data areas",
            catalog.data_areas.iter().map(|item| item.id.as_str()),
        )?;
        validate_unique_ids(
            "domain",
            catalog.domains.iter().map(|item| item.id.as_str()),
        )?;
        validate_unique_ids(
            "schema",
            catalog.schemas.iter().map(|item| item.id.as_str()),
        )?;
        validate_unique_ids(
            "metric",
            catalog.metrics.iter().map(|item| item.id.as_str()),
        )?;
        validate_unique_ids(
            "capability",
            catalog.capabilities.iter().map(|item| item.id.as_str()),
        )?;
        validate_unique_ids("query", catalog.queries.iter().map(|item| item.id.as_str()))?;
        validate_unique_ids(
            "policy",
            catalog.policies.iter().map(|item| item.id.as_str()),
        )?;
        validate_unique_ids(
            "response",
            catalog.responses.iter().map(|item| item.id.as_str()),
        )?;
        validate_unique_ids(
            "parameter input",
            catalog.parameter_inputs.iter().map(|item| item.id.as_str()),
        )?;
        validate_parameter_input_registry(&catalog.parameter_inputs, &catalog.queries)?;

        for area in &catalog.data_areas {
            validate_status("data area", &area.id, &area.status, DATA_AREA_STATUSES)?;
        }

        for domain in &catalog.domains {
            validate_status("domain", &domain.id, &domain.status, DOMAIN_STATUSES)?;
        }

        for capability in &catalog.capabilities {
            validate_status(
                "capability",
                &capability.id,
                &capability.status,
                CAPABILITY_STATUSES,
            )?;
        }

        for query in &catalog.queries {
            validate_status(
                "query database",
                &query.id,
                &query.database,
                QUERY_DATABASES,
            )?;
        }

        let data_area_ids = catalog
            .data_areas
            .iter()
            .map(|item| item.id.as_str())
            .collect::<HashSet<_>>();

        let domains_ids = catalog
            .domains
            .iter()
            .map(|item| item.id.as_str())
            .collect::<HashSet<_>>();

        let query_ids = catalog
            .queries
            .iter()
            .map(|item| item.id.as_str())
            .collect::<HashSet<_>>();

        let deferred_or_rejected_data_area_ids = catalog
            .data_areas
            .iter()
            .filter(|item| is_deferred_or_rejected_status(&item.status))
            .map(|item| item.id.as_str())
            .collect::<HashSet<_>>();

        for domain in &catalog.domains {
            validate_refs(
                "domain",
                &domain.id,
                "data area",
                &domain.data_areas,
                &data_area_ids,
            )?;
        }

        validate_generic_layer("schema", &catalog.schemas, &data_area_ids, &domains_ids)?;
        validate_generic_layer("metric", &catalog.metrics, &data_area_ids, &domains_ids)?;
        validate_generic_layer("policy", &catalog.policies, &data_area_ids, &domains_ids)?;
        validate_generic_layer("response", &catalog.responses, &data_area_ids, &domains_ids)?;

        for capability in &catalog.capabilities {
            // Snapshot output_modes ("summary") have no user-required params —
            // they aggregate current state filtered by authorized office scope only.
            if capability.status == "approved_mvp"
                && capability.output_mode != "summary"
                && capability.required_parameters.is_empty()
            {
                bail!(
                    "approved capability {} must declare required parameters",
                    capability.id
                );
            }

            if capability.status == "approved_mvp" && capability.metrics.is_empty() {
                bail!("approved capability {} must declare metrics", capability.id);
            }

            if capability.status == "approved_mvp"
                && (matches!(
                    capability.request_shape.operation,
                    crate::assistant::RequestOperation::Unknown
                ) || matches!(
                    capability.request_shape.subject,
                    crate::assistant::RequestSubject::Unknown
                ) || matches!(
                    capability.request_shape.grouping,
                    crate::assistant::RequestGrouping::Unknown
                ) || matches!(
                    capability.request_shape.output,
                    crate::assistant::RequestOutput::Unknown
                ) || matches!(
                    capability.request_shape.pii,
                    crate::assistant::RequestPii::Unknown
                ))
            {
                bail!(
                    "approved capability {} must declare a complete request_shape",
                    capability.id
                );
            }

            validate_status(
                "capability output mode",
                &capability.id,
                &capability.output_mode,
                OUTPUT_MODES,
            )?;

            if !domains_ids.contains(capability.domain.as_str()) {
                bail!(
                    "capability {} references unknown domain {}",
                    capability.id,
                    capability.domain
                );
            }

            if !query_ids.contains(capability.query_id.as_str()) {
                bail!(
                    "capability {} references unknown query {}",
                    capability.id,
                    capability.query_id
                );
            }

            if capability.status == "approved_mvp" {
                let query = catalog
                    .queries
                    .iter()
                    .find(|query| query.id == capability.query_id)
                    .expect("validated query reference");
                validate_capability_parameter_contract(
                    capability,
                    query,
                    &catalog.parameter_inputs,
                )?;
            }

            validate_refs(
                "capability",
                &capability.id,
                "data area",
                &capability.data_areas,
                &data_area_ids,
            )?;

            validate_no_deferred_or_rejected_data_areas(
                "capability",
                &capability.id,
                &capability.data_areas,
                &deferred_or_rejected_data_area_ids,
            )?;
        }

        for query in catalog.queries.iter() {
            if query.parameters.is_empty() {
                bail!("query {} must declare parameters", query.id);
            }

            for parameter in &query.parameters {
                if parameter.name.trim().is_empty() {
                    bail!("query {} has parameter with empty name", query.id);
                }

                validate_status(
                    "query parameter type",
                    &format!("{}.{}", query.id, parameter.name),
                    &parameter.kind,
                    PARAMETER_TYPES,
                )?;
            }

            validate_refs(
                "query",
                &query.id,
                "data area",
                &query.data_areas,
                &data_area_ids,
            )?;

            validate_no_deferred_or_rejected_data_areas(
                "query",
                &query.id,
                &query.data_areas,
                &deferred_or_rejected_data_area_ids,
            )?;

            if query.output_fields.is_empty() {
                bail!("query {} must have at least one output field", query.id);
            }

            for field in &query.output_fields {
                if field.name.trim().is_empty() {
                    bail!("query {} has output field with empty name", query.id);
                }

                validate_status(
                    "query output sensitivity",
                    &format!("{}.{}", query.id, field.name),
                    &field.sensitivity,
                    SENSITIVITY_CLASSES,
                )?;
            }

            let sql_path = resolve_sql_path(catalog, query);

            if !sql_path.exists() {
                bail!(
                    "query {} references non-existing sql file {}",
                    query.id,
                    sql_path.display()
                );
            }

            validate_sql_safety(query, &sql_path)?;
        }

        Ok(())
    }
}

pub(crate) fn validate_classification_policy(
    policy: &crate::knowledge::model::ClassificationPolicy,
) -> Result<()> {
    if !(policy.min_gap > 0.0 && policy.min_gap < 1.0) {
        anyhow::bail!(
            "classification_policy.min_gap must be in (0, 1); got {}",
            policy.min_gap
        );
    }
    if !(policy.min_floor > 0.0 && policy.min_floor < 1.0) {
        anyhow::bail!(
            "classification_policy.min_floor must be in (0, 1); got {}",
            policy.min_floor
        );
    }
    if policy.others_key.trim().is_empty() {
        anyhow::bail!("classification_policy.others_key must be non-empty");
    }
    if policy.others_label.trim().is_empty() {
        anyhow::bail!("classification_policy.others_label must be non-empty");
    }
    Ok(())
}

pub(crate) fn validate_classification_policy_against_catalog(
    policy: &crate::knowledge::model::ClassificationPolicy,
    capability_ids: &[String],
) -> Result<()> {
    validate_classification_policy(policy)?;
    if capability_ids.iter().any(|id| id == &policy.others_key) {
        anyhow::bail!(
            "classification_policy.others_key '{}' must not collide with any capability id",
            policy.others_key
        );
    }
    Ok(())
}

fn validate_generic_layer(
    label: &str,
    items: &[GenericKnowledge],
    data_area_ids: &HashSet<&str>,
    domain_ids: &HashSet<&str>,
) -> Result<()> {
    for item in items {
        if item.checks.is_empty() {
            bail!("{label} {} must declare checks", item.id);
        }

        validate_refs(
            label,
            &item.id,
            "data area",
            &item.data_areas,
            data_area_ids,
        )?;

        if let Some(domain) = item.domain.as_deref()
            && !domain_ids.contains(domain)
        {
            bail!("{label} {} references unknown domain {domain}", item.id);
        }
    }

    Ok(())
}

fn validate_parameter_input_registry(
    inputs: &[ParameterInputKnowledge],
    queries: &[QueryKnowledge],
) -> Result<()> {
    let mut covered = HashSet::new();
    for input in inputs {
        for parameter in &input.parameters {
            if !covered.insert(parameter.as_str()) {
                bail!("parameter {} is covered more than once", parameter);
            }
        }
    }

    for input in inputs {
        if input.parameters.is_empty() {
            bail!(
                "parameter input {} must cover at least one parameter",
                input.id
            );
        }

        for parameter in &input.parameters {
            let matches = queries
                .iter()
                .flat_map(|query| query.parameters.iter())
                .filter(|candidate| candidate.name == *parameter)
                .collect::<Vec<_>>();
            if matches.is_empty() {
                bail!(
                    "parameter input {} covers unknown parameter {}",
                    input.id,
                    parameter
                );
            }
            if matches
                .iter()
                .any(|candidate| candidate.source.as_deref() == Some("authorized_scope"))
            {
                bail!(
                    "parameter input {} must not cover authorized-scope parameter {}",
                    input.id,
                    parameter
                );
            }

            let expected_kind = match input.field_type {
                ClarificationFieldType::DateRange => "date",
                ClarificationFieldType::Integer => "integer",
                ClarificationFieldType::Text => "string",
            };
            if matches
                .iter()
                .any(|candidate| candidate.kind != expected_kind)
            {
                bail!("parameter input {} has an invalid type mapping", input.id);
            }
        }

        let valid_mapping = match input.field_type {
            ClarificationFieldType::DateRange => {
                let names = input
                    .parameters
                    .iter()
                    .map(String::as_str)
                    .collect::<HashSet<_>>();
                names.len() == 2 && names == HashSet::from(["from_date", "to_date"])
            }
            ClarificationFieldType::Integer | ClarificationFieldType::Text => {
                input.parameters.len() == 1
            }
        };

        if !valid_mapping {
            bail!("parameter input {} has an invalid type mapping", input.id);
        }
    }

    Ok(())
}

fn validate_capability_parameter_contract(
    capability: &CapabilityKnowledge,
    query: &QueryKnowledge,
    inputs: &[ParameterInputKnowledge],
) -> Result<()> {
    let required_user_parameters = query
        .parameters
        .iter()
        .filter(|parameter| {
            parameter.required && parameter.source.as_deref() != Some("authorized_scope")
        })
        .map(|parameter| parameter.name.as_str())
        .collect::<HashSet<_>>();
    let declared_required_parameters = capability
        .required_parameters
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();

    if declared_required_parameters != required_user_parameters {
        bail!(
            "approved capability {} required_parameters do not match required user query parameters",
            capability.id
        );
    }

    for parameter in &required_user_parameters {
        let coverage = inputs
            .iter()
            .filter(|input| {
                input
                    .parameters
                    .iter()
                    .any(|candidate| candidate == parameter)
            })
            .count();
        if coverage != 1 {
            bail!(
                "approved capability {} required user parameter {} must be covered exactly once",
                capability.id,
                parameter
            );
        }
    }

    if let Some(max_limit) = capability.guards.max_limit
        && max_limit <= 0
    {
        bail!("capability {} max_limit must be positive", capability.id);
    }
    if let Some(max_date_range_days) = capability.guards.max_date_range_days
        && max_date_range_days == 0
    {
        bail!(
            "capability {} max_date_range_days must be positive",
            capability.id
        );
    }
    if let Some(default_limit) = capability.defaults.default_limit {
        if default_limit <= 0 {
            bail!(
                "capability {} default_limit must be positive",
                capability.id
            );
        }
        if !capability
            .required_parameters
            .iter()
            .any(|parameter| parameter == "limit")
            && !capability
                .optional_parameters
                .iter()
                .any(|parameter| parameter == "limit")
        {
            bail!(
                "capability {} default_limit requires limit to be accepted",
                capability.id
            );
        }
        if !query
            .parameters
            .iter()
            .any(|parameter| parameter.name == "limit")
        {
            bail!(
                "capability {} default_limit requires query parameter limit",
                capability.id
            );
        }
        if let Some(max_limit) = capability.guards.max_limit
            && default_limit > max_limit
        {
            bail!(
                "capability {} default_limit exceeds max_limit",
                capability.id
            );
        }
    }

    Ok(())
}

fn validate_unique_ids<'a>(label: &str, ids: impl Iterator<Item = &'a str>) -> Result<()> {
    let mut seen = HashSet::new();
    for id in ids {
        if id.trim().is_empty() {
            bail!("{label} id must not be empty");
        }
        if !seen.insert(id) {
            bail!("duplicate {label} id {id}");
        }
    }

    Ok(())
}

fn validate_status(label: &str, id: &str, status: &str, allowed: &[&str]) -> Result<()> {
    if allowed.contains(&status) {
        Ok(())
    } else {
        bail!("{label} {id} has invalid status {status}");
    }
}

fn validate_refs(
    owner_label: &str,
    owner_id: &str,
    target_label: &str,
    refs: &[String],
    valid_ids: &HashSet<&str>,
) -> Result<()> {
    for reference in refs {
        if !valid_ids.contains(reference.as_str()) {
            bail!("{owner_label} {owner_id} references unknown {target_label} {reference}");
        }
    }

    Ok(())
}

fn validate_no_deferred_or_rejected_data_areas(
    owner_label: &str,
    owner_id: &str,
    data_areas: &[String],
    blocked_ids: &HashSet<&str>,
) -> Result<()> {
    for data_area in data_areas {
        if blocked_ids.contains(data_area.as_str()) {
            bail!("{owner_label} {owner_id} references deferred or rejected data area {data_area}");
        }
    }

    Ok(())
}

fn is_deferred_or_rejected_status(status: &str) -> bool {
    matches!(
        status,
        "deferred" | "deferred_group" | "rejected" | "rejected_group" | "out_of_scope"
    )
}

fn resolve_sql_path(catalog: &KnowledgeCatalog, query: &QueryKnowledge) -> PathBuf {
    if query.sql_file.starts_with("queries/") {
        catalog
            .query_path
            .parent()
            .unwrap_or(&catalog.query_path)
            .join(&query.sql_file)
    } else {
        catalog.query_path.join(&query.sql_file)
    }
}

/// Runtime SQL validation: prepares each approved Fineract query against the
/// Fineract pool. The prepare step parses the SQL (covers the "EXPLAIN succeeds"
/// requirement without executing rows) and returns column metadata, which we
/// compare to the declared `output_fields` contract.
///
/// ponytail: only column *names* are checked; type matching is left to the
/// executor's `try_get` at runtime — upgrade when output_fields gain typed PG
/// OIDs in YAML.
pub async fn validate_runtime(catalog: &KnowledgeCatalog, fineract_pool: &PgPool) -> Result<()> {
    for query in &catalog.queries {
        if query.database != "fineract" {
            continue;
        }

        let sql_path = resolve_sql_path(catalog, query);
        let sql = std::fs::read_to_string(&sql_path)?;
        let sql_for_prepare: String = sql.trim().trim_end_matches(';').to_string();

        let statement = fineract_pool
            .prepare(AssertSqlSafe(sql_for_prepare).into_sql_str())
            .await
            .map_err(|err| {
                anyhow::anyhow!("query {} failed prepare against fineract: {err}", query.id)
            })?;

        let actual: Vec<String> = statement
            .columns()
            .iter()
            .map(|column| column.name().to_string())
            .collect();
        let expected: Vec<String> = query
            .output_fields
            .iter()
            .map(|field| field.name.clone())
            .collect();

        if actual != expected {
            bail!(
                "query {} output columns {:?} do not match declared output_fields {:?}",
                query.id,
                actual,
                expected
            );
        }
    }

    Ok(())
}

fn validate_sql_safety(query: &QueryKnowledge, sql_path: &Path) -> Result<()> {
    let sql = std::fs::read_to_string(sql_path)?;
    let trimmed = sql.trim();
    let upper = trimmed.to_ascii_uppercase();

    // Allow SELECT or WITH ... SELECT (CTE). Both are read-only.
    if !(upper.starts_with("SELECT") || upper.starts_with("WITH")) {
        bail!(
            "query {} SQL must start with SELECT or WITH (CTE)",
            query.id
        );
    }

    let without_final_semicolon = trimmed.strip_suffix(';').unwrap_or(trimmed);
    if without_final_semicolon.contains(';') {
        bail!("query {} SQL must be a single statement", query.id);
    }

    let tokens = sql_tokens(&upper);
    for command in UNSAFE_SQL_COMMANDS {
        if tokens.contains(command) {
            bail!("query {} SQL contains unsafe command {}", query.id, command);
        }
    }

    validate_placeholders(query, trimmed)?;

    if has_parameter(query, "office_ids") {
        let office_pos = parameter_position(query, "office_ids");
        let expected = format!("ANY(${office_pos}::BIGINT[])");
        if !upper.contains(&expected) {
            bail!(
                "query {} SQL must constrain authorized office ids via `ANY(${}::bigint[])`",
                query.id,
                office_pos
            );
        }
    }

    if has_parameter(query, "from_date") && has_parameter(query, "to_date") {
        let from_pos = parameter_position(query, "from_date");
        let to_pos = parameter_position(query, "to_date");
        let expected = format!("BETWEEN ${from_pos}::DATE AND ${to_pos}::DATE");
        if !upper.contains(&expected) {
            bail!(
                "query {} SQL must constrain a date column with `BETWEEN ${}::date AND ${}::date`",
                query.id,
                from_pos,
                to_pos
            );
        }
    }

    // Bound result size via LIMIT (atomic top_n) or ROW_NUMBER()/RANK() over a
    // partition (per-group top_n, used by monthly_top_n et al). Both put a hard
    // cap on rows; either one is acceptable.
    if has_parameter(query, "limit")
        && !(upper.contains("LIMIT") || upper.contains("ROW_NUMBER(") || upper.contains("RANK("))
    {
        bail!(
            "query {} SQL must constrain result limit via LIMIT or a window function",
            query.id
        );
    }

    Ok(())
}

fn validate_placeholders(query: &QueryKnowledge, sql: &str) -> Result<()> {
    let placeholders = placeholder_numbers(sql);
    let expected_count = query.parameters.len();

    for index in 1..=expected_count {
        if !placeholders.contains(&index) {
            bail!("query {} SQL is missing placeholder ${index}", query.id);
        }
    }

    if placeholders
        .iter()
        .any(|placeholder| *placeholder > expected_count)
    {
        bail!(
            "query {} SQL has more placeholders than declared parameters",
            query.id
        );
    }

    for (index, parameter) in query.parameters.iter().enumerate() {
        let placeholder = index + 1;
        let cast = placeholder_cast(sql, placeholder);

        match parameter.kind.as_str() {
            "date" if cast.as_deref() != Some("date") => {
                bail!(
                    "query {} parameter {} must use ${placeholder}::date",
                    query.id,
                    parameter.name
                );
            }
            "string" if cast.as_deref() != Some("text") => {
                bail!(
                    "query {} parameter {} must use ${placeholder}::text",
                    query.id,
                    parameter.name
                );
            }
            "array_bigint" if cast.as_deref() != Some("bigint[]") => {
                bail!(
                    "query {} parameter {} must use ${placeholder}::bigint[]",
                    query.id,
                    parameter.name
                );
            }
            "integer"
                if cast.is_none()
                    && sql
                        .to_ascii_uppercase()
                        .contains(&format!("LIMIT ${placeholder}")) => {}
            "integer" if matches!(cast.as_deref(), Some("integer" | "int4" | "bigint")) => {}
            "integer" => {
                bail!(
                    "query {} parameter {} must use integer placeholder ${placeholder}",
                    query.id,
                    parameter.name
                );
            }
            _ => {}
        }
    }

    Ok(())
}

fn has_parameter(query: &QueryKnowledge, name: &str) -> bool {
    query
        .parameters
        .iter()
        .any(|parameter| parameter.name == name)
}

fn parameter_position(query: &QueryKnowledge, name: &str) -> usize {
    query
        .parameters
        .iter()
        .position(|parameter| parameter.name == name)
        .map(|index| index + 1)
        .unwrap_or(0)
}

fn placeholder_numbers(sql: &str) -> HashSet<usize> {
    let bytes = sql.as_bytes();
    let mut placeholders = HashSet::new();
    let mut index = 0;

    while index < bytes.len() {
        if bytes[index] != b'$' {
            index += 1;
            continue;
        }

        let start = index + 1;
        let mut end = start;
        while end < bytes.len() && bytes[end].is_ascii_digit() {
            end += 1;
        }

        if start != end
            && let Ok(number) = sql[start..end].parse::<usize>()
        {
            placeholders.insert(number);
        }

        index = end;
    }

    placeholders
}

fn placeholder_cast(sql: &str, placeholder: usize) -> Option<String> {
    let marker = format!("${placeholder}::");
    let start = sql.find(&marker)? + marker.len();
    let rest = &sql[start..];
    let end = rest
        .find(|character: char| {
            !(character.is_ascii_alphanumeric() || matches!(character, '_' | '[' | ']'))
        })
        .unwrap_or(rest.len());

    Some(rest[..end].to_ascii_lowercase())
}

fn sql_tokens(sql_upper: &str) -> HashSet<&str> {
    sql_upper
        .split(|character: char| !(character.is_ascii_alphanumeric() || character == '_'))
        .filter(|token| !token.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_invalid_status() {
        let error = validate_status("capability", "bad", "wrong", CAPABILITY_STATUSES)
            .expect_err("invalid status should fail");

        assert!(error.to_string().contains("invalid status wrong"));
    }

    #[test]
    fn detects_placeholder_numbers() {
        assert_eq!(
            placeholder_numbers("SELECT $1::date, $3::bigint[]").len(),
            2
        );
        assert!(placeholder_numbers("SELECT $1::date, $3::bigint[]").contains(&3));
    }

    #[test]
    fn detects_placeholder_cast() {
        assert_eq!(
            placeholder_cast("SELECT $3::bigint[]", 3).as_deref(),
            Some("bigint[]")
        );
    }
}
