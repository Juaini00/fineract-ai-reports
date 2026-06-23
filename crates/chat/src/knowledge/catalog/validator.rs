use std::{collections::HashSet, path::Path};

use anyhow::{Result, bail};

use crate::knowledge::model::{KnowledgeCatalog, QueryKnowledge};

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
const OUTPUT_MODES: &[&str] = &["total", "top_n"];
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
        validate_unique_ids(
            "data areas",
            catalog.data_areas.iter().map(|item| item.id.as_str()),
        )?;
        validate_unique_ids(
            "domain",
            catalog.domains.iter().map(|item| item.id.as_str()),
        )?;
        validate_unique_ids(
            "capability",
            catalog.capabilities.iter().map(|item| item.id.as_str()),
        )?;
        validate_unique_ids("query", catalog.queries.iter().map(|item| item.id.as_str()))?;

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

        for capability in &catalog.capabilities {
            if capability.status == "approved_mvp" && capability.required_parameters.is_empty() {
                bail!(
                    "approved capability {} must declare required parameters",
                    capability.id
                );
            }

            if capability.status == "approved_mvp" && capability.metrics.is_empty() {
                bail!("approved capability {} must declare metrics", capability.id);
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

            let sql_path = if query.sql_file.starts_with("queries/") {
                catalog
                    .query_path
                    .parent()
                    .unwrap_or(&catalog.query_path)
                    .join(&query.sql_file)
            } else {
                catalog.query_path.join(&query.sql_file)
            };

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

fn validate_sql_safety(query: &QueryKnowledge, sql_path: &Path) -> Result<()> {
    let sql = std::fs::read_to_string(sql_path)?;
    let trimmed = sql.trim();
    let upper = trimmed.to_ascii_uppercase();

    if !upper.starts_with("SELECT") {
        bail!("query {} SQL must start with SELECT", query.id);
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
        if !upper.contains("OFFICE_ID") || !upper.contains("ANY($") {
            bail!(
                "query {} SQL must constrain authorized office ids",
                query.id
            );
        }
    }

    if has_parameter(query, "from_date") && has_parameter(query, "to_date") {
        if !upper.contains("TRANSACTION_DATE") || !upper.contains("BETWEEN") {
            bail!(
                "query {} SQL must constrain transaction date range",
                query.id
            );
        }
    }

    if has_parameter(query, "limit") && !upper.contains("LIMIT") {
        bail!("query {} SQL must constrain result limit", query.id);
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

        if start != end {
            if let Ok(number) = sql[start..end].parse::<usize>() {
                placeholders.insert(number);
            }
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
