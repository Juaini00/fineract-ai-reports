use anyhow::{Result, bail};
use serde_json::Value;

use crate::knowledge::dataset::model::{
    DatasetFilterSelection, DatasetKnowledge, DatasetRecipe, DatasetSelection, FilterOperator,
};

/// Resolves a catalog-owned recipe against normalized parameters. The result
/// contains only declared IDs and typed values; SQL remains catalog-owned.
pub fn resolve_recipe(
    dataset: &DatasetKnowledge,
    recipe: &DatasetRecipe,
    params: &Value,
) -> Result<DatasetSelection> {
    if dataset.id != recipe.dataset_id {
        bail!(
            "dataset recipe references unknown dataset {}",
            recipe.dataset_id
        );
    }
    let shape = dataset.shape(&recipe.shape_id).ok_or_else(|| {
        anyhow::anyhow!("dataset {} has no shape {}", dataset.id, recipe.shape_id)
    })?;
    if let Some(order_by_id) = recipe.order_by_id.as_deref()
        && !shape.order_by.iter().any(|id| id == order_by_id)
    {
        bail!(
            "dataset {} shape {} does not allow order_by {}",
            dataset.id,
            shape.id,
            order_by_id
        );
    }

    let params = params
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("dataset recipe parameters must be an object"))?;
    let mut filters = Vec::with_capacity(recipe.filters.len());
    for mapping in &recipe.filters {
        let slot = dataset
            .filters
            .iter()
            .find(|slot| slot.id == mapping.filter_id)
            .ok_or_else(|| {
                anyhow::anyhow!("dataset {} has no filter {}", dataset.id, mapping.filter_id)
            })?;
        if !slot.operators.contains(&mapping.operator) {
            bail!(
                "dataset {} filter {} does not allow operator {:?}",
                dataset.id,
                slot.id,
                mapping.operator
            );
        }
        if mapping.parameter.is_some() == mapping.value.is_some() {
            bail!(
                "dataset filter {} must declare exactly one of parameter or value",
                slot.id
            );
        }
        let value = match (&mapping.parameter, &mapping.value) {
            (Some(parameter), None) => params
                .get(parameter)
                .ok_or_else(|| anyhow::anyhow!("missing dataset recipe parameter {parameter}"))?,
            (None, Some(value)) => value,
            _ => unreachable!("validated exactly one recipe value source"),
        };
        if !valid_filter_value(&slot.kind, mapping.operator, value) {
            bail!("dataset filter {} has invalid value type", slot.id);
        }
        filters.push(DatasetFilterSelection {
            filter_id: slot.id.clone(),
            operator: mapping.operator,
            value: value.clone(),
        });
    }

    for field in &recipe.projection {
        if !shape
            .output_fields(dataset)
            .iter()
            .any(|candidate| candidate.name == *field)
        {
            bail!("dataset {} has no output field {field}", dataset.id);
        }
    }

    Ok(DatasetSelection {
        dataset_id: dataset.id.clone(),
        shape_id: shape.id.clone(),
        order_by_id: recipe.order_by_id.clone(),
        filters,
        projection: recipe.projection.clone(),
    })
}

fn valid_filter_value(kind: &str, operator: FilterOperator, value: &Value) -> bool {
    if operator == FilterOperator::Between {
        return value.as_array().is_some_and(|values| {
            values.len() == 2 && values.iter().all(|value| valid_scalar(kind, value))
        });
    }
    valid_scalar(kind, value)
}

fn valid_scalar(kind: &str, value: &Value) -> bool {
    match kind {
        "date" | "string" => value.is_string(),
        "integer" => value.as_i64().is_some(),
        "boolean" => value.is_boolean(),
        "decimal" => value
            .as_str()
            .is_some_and(|value| value.parse::<rust_decimal::Decimal>().is_ok()),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assistant::{
        RequestGrouping, RequestOperation, RequestOutput, RequestPii, RequestShape, RequestSubject,
    };
    use crate::knowledge::dataset::model::{
        DatasetOutputField, DatasetRecipeFilter, FilterSlot, ShapeOption,
    };
    use crate::knowledge::model::Sensitivity;

    fn dataset() -> DatasetKnowledge {
        DatasetKnowledge {
            id: "savings.account_activity".into(),
            database: "fineract".into(),
            source_sql: "queries/datasets/savings/account_activity.source.sql".into(),
            tables: vec!["m_savings_account_transaction".into()],
            filters: vec![FilterSlot {
                id: "latest_amount".into(),
                expr: "latest_transaction_amount".into(),
                kind: "decimal".into(),
                case_insensitive: false,
                operators: vec![FilterOperator::Eq],
            }],
            shapes: vec![ShapeOption {
                id: "account_match".into(),
                request_shape: RequestShape {
                    operation: RequestOperation::Lookup,
                    subject: RequestSubject::SavingsTransaction,
                    grouping: RequestGrouping::None,
                    output: RequestOutput::Lookup,
                    pii: RequestPii::ClientIdentity,
                },
                fragment: Some("queries/datasets/savings/account_activity.frag.sql".into()),
                order_by: vec![],
                output_fields: Vec::new(),
                parameters: Vec::new(),
            }],
            order_by: vec![],
            output_fields: vec![DatasetOutputField {
                name: "savings_account_id".into(),
                kind: "bigint".into(),
                sensitivity: Sensitivity::PublicBusiness,
                core: true,
            }],
            parameters: vec![],
            timeout_ms: Some(3_000),
        }
    }

    fn recipe() -> DatasetRecipe {
        DatasetRecipe {
            dataset_id: "savings.account_activity".into(),
            shape_id: "account_match".into(),
            order_by_id: None,
            filters: vec![DatasetRecipeFilter {
                filter_id: "latest_amount".into(),
                operator: FilterOperator::Eq,
                parameter: Some("latest_transaction_amount".into()),
                value: None,
            }],
            projection: vec!["savings_account_id".into()],
        }
    }

    #[test]
    fn resolves_declared_ids_and_exact_decimal_value() {
        let selection = resolve_recipe(
            &dataset(),
            &recipe(),
            &serde_json::json!({"latest_transaction_amount": "0.130000"}),
        )
        .unwrap();

        assert_eq!(selection.dataset_id, "savings.account_activity");
        assert_eq!(selection.filters[0].value, "0.130000");
    }

    #[test]
    fn rejects_unknown_filter_operator_projection_and_invalid_value() {
        let mut invalid_recipe = recipe();
        invalid_recipe.filters[0].filter_id = "unknown".into();
        assert!(resolve_recipe(&dataset(), &invalid_recipe, &serde_json::json!({})).is_err());

        let mut invalid_recipe = recipe();
        invalid_recipe.filters[0].operator = FilterOperator::Gt;
        assert!(resolve_recipe(&dataset(), &invalid_recipe, &serde_json::json!({})).is_err());

        let mut invalid_recipe = recipe();
        invalid_recipe.projection = vec!["account_no".into()];
        assert!(
            resolve_recipe(
                &dataset(),
                &invalid_recipe,
                &serde_json::json!({"latest_transaction_amount": "0.130000"})
            )
            .is_err()
        );

        assert!(
            resolve_recipe(
                &dataset(),
                &recipe(),
                &serde_json::json!({"latest_transaction_amount": 0.13})
            )
            .is_err()
        );
    }
}
