use std::collections::HashSet;

use anyhow::{Result, bail};

use crate::knowledge::model::KnowledgeCatalog;

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
