//! Static dataset rules, enforced at catalog load before any SQL is prepared.

use std::collections::HashSet;

use anyhow::{Result, bail};

use crate::knowledge::dataset::grammar::validate_sql_expr;
use crate::knowledge::dataset::model::DatasetKnowledge;

pub fn validate_dataset(dataset: &DatasetKnowledge) -> Result<()> {
    let mut filter_ids = HashSet::new();
    for filter in &dataset.filters {
        if !filter_ids.insert(filter.id.as_str()) {
            bail!("dataset {} declares filter {} twice", dataset.id, filter.id);
        }
        if filter.operators.is_empty() {
            bail!(
                "dataset {} filter {} declares no operators",
                dataset.id,
                filter.id
            );
        }
        validate_sql_expr(&filter.expr).map_err(|reason| {
            anyhow::anyhow!("dataset {} filter {}: {reason}", dataset.id, filter.id)
        })?;
    }

    let mut order_by_ids = HashSet::new();
    for option in &dataset.order_by {
        if !order_by_ids.insert(option.id.as_str()) {
            bail!(
                "dataset {} declares order_by {} twice",
                dataset.id,
                option.id
            );
        }
        validate_sql_expr(&option.expr).map_err(|reason| {
            anyhow::anyhow!("dataset {} order_by {}: {reason}", dataset.id, option.id)
        })?;
    }

    if dataset.shapes.is_empty() {
        bail!("dataset {} declares no shapes", dataset.id);
    }
    let mut shape_ids = HashSet::new();
    for shape in &dataset.shapes {
        if !shape_ids.insert(shape.id.as_str()) {
            bail!("dataset {} declares shape {} twice", dataset.id, shape.id);
        }
        for reference in &shape.order_by {
            if !order_by_ids.contains(reference.as_str()) {
                bail!(
                    "dataset {} shape {} references undeclared order_by {reference}",
                    dataset.id,
                    shape.id
                );
            }
        }
        // A dataset with filter slots always composes a CTE, which needs a
        // fragment to select from it. Only a fully degenerate dataset may omit one.
        if shape.fragment.is_none() && !dataset.filters.is_empty() {
            bail!(
                "dataset {} shape {} must declare a fragment because the dataset declares filters",
                dataset.id,
                shape.id
            );
        }
    }

    if !dataset.output_fields.is_empty() && !dataset.output_fields.iter().any(|field| field.core) {
        bail!(
            "dataset {} declares no core output field, so projection would render an empty table",
            dataset.id
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assistant::{
        RequestGrouping, RequestOperation, RequestOutput, RequestPii, RequestShape, RequestSubject,
    };
    use crate::knowledge::dataset::model::{
        DatasetOutputField, FilterOperator, FilterSlot, OrderByOption, ShapeOption,
    };
    use crate::knowledge::model::Sensitivity;

    fn valid() -> DatasetKnowledge {
        DatasetKnowledge {
            id: "savings.account_charges".into(),
            database: "fineract".into(),
            source_sql: "queries/savings/account_charges.source.sql".into(),
            tables: vec!["m_savings_account_charge".into()],
            filters: vec![FilterSlot {
                id: "due_date".into(),
                expr: "sac.charge_due_date".into(),
                kind: "date".into(),
                operators: vec![FilterOperator::Eq],
            }],
            shapes: vec![ShapeOption {
                id: "list".into(),
                request_shape: RequestShape {
                    operation: RequestOperation::List,
                    subject: RequestSubject::SavingsAccountCharge,
                    grouping: RequestGrouping::None,
                    output: RequestOutput::List,
                    pii: RequestPii::None,
                },
                fragment: Some("queries/savings/account_charges.list.frag.sql".into()),
                order_by: vec!["created_desc".into()],
            }],
            order_by: vec![OrderByOption {
                id: "created_desc".into(),
                expr: "sac.created_on_utc DESC".into(),
            }],
            output_fields: vec![DatasetOutputField {
                name: "savings_account_charge_id".into(),
                kind: "bigint".into(),
                sensitivity: Sensitivity::PublicBusiness,
                core: true,
            }],
            parameters: Vec::new(),
            timeout_ms: None,
        }
    }

    #[test]
    fn accepts_a_well_formed_dataset() {
        assert!(validate_dataset(&valid()).is_ok());
    }

    #[test]
    fn rejects_a_shape_referencing_an_undeclared_order_by() {
        let mut dataset = valid();
        dataset.shapes[0].order_by = vec!["nope".into()];
        let error = validate_dataset(&dataset).unwrap_err().to_string();
        assert!(error.contains("order_by"), "got: {error}");
    }

    #[test]
    fn rejects_a_dataset_with_no_core_output_field() {
        let mut dataset = valid();
        dataset.output_fields[0].core = false;
        let error = validate_dataset(&dataset).unwrap_err().to_string();
        assert!(error.contains("core"), "got: {error}");
    }

    #[test]
    fn rejects_duplicate_filter_and_shape_ids() {
        let mut dataset = valid();
        let duplicate = dataset.filters[0].clone();
        dataset.filters.push(duplicate);
        assert!(validate_dataset(&dataset).is_err());

        let mut dataset = valid();
        let duplicate = dataset.shapes[0].clone();
        dataset.shapes.push(duplicate);
        assert!(validate_dataset(&dataset).is_err());
    }

    #[test]
    fn rejects_expressions_that_fail_the_grammar() {
        let mut dataset = valid();
        dataset.filters[0].expr = "sac.id; DROP TABLE m_client".into();
        assert!(validate_dataset(&dataset).is_err());

        let mut dataset = valid();
        dataset.order_by[0].expr = "sac.id /* x */".into();
        assert!(validate_dataset(&dataset).is_err());
    }

    #[test]
    fn rejects_a_shape_without_a_fragment_when_the_dataset_declares_filters() {
        let mut dataset = valid();
        dataset.shapes[0].fragment = None;
        let error = validate_dataset(&dataset).unwrap_err().to_string();
        assert!(error.contains("fragment"), "got: {error}");
    }
}
