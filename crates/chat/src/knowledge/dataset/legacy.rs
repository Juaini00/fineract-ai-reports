//! Bridges the existing capability model onto the dataset model.
//!
//! A capability today freezes source, filter, shape and projection together.
//! That is exactly a dataset with one shape, no fragment, and no filter slots —
//! so the conversion is mechanical and needs no authored YAML. Composition of
//! such a dataset returns the source SQL verbatim, which is what makes Phase A
//! equivalence provable by string comparison.

use crate::knowledge::dataset::model::{DatasetKnowledge, DatasetOutputField, ShapeOption};
use crate::knowledge::model::{CapabilityKnowledge, QueryKnowledge};

/// The shape id used for every legacy-derived dataset.
pub const LEGACY_SHAPE_ID: &str = "legacy";

pub fn degenerate_dataset(
    capability: &CapabilityKnowledge,
    query: &QueryKnowledge,
) -> DatasetKnowledge {
    DatasetKnowledge {
        id: query.id.clone(),
        database: query.database.clone(),
        source_sql: query.sql_file.clone(),
        tables: query.tables.clone(),
        // No filter slots: the legacy WHERE clause is baked into the source SQL.
        filters: Vec::new(),
        filters_exempt: Vec::new(),
        shapes: vec![ShapeOption {
            id: LEGACY_SHAPE_ID.to_string(),
            request_shape: capability.request_shape.clone(),
            // No fragment: the source SQL already selects, orders and limits.
            fragment: None,
            order_by: Vec::new(),
            output_fields: Vec::new(),
            parameters: Vec::new(),
        }],
        // Ordering stays inside the source SQL for the degenerate case.
        order_by: Vec::new(),
        output_fields: query
            .output_fields
            .iter()
            .map(|field| DatasetOutputField {
                name: field.name.clone(),
                kind: field.kind.clone(),
                sensitivity: field.sensitivity,
                // Phase A must not change which columns render, so every field
                // is core and projection is a no-op until a dataset is merged.
                core: true,
            })
            .collect(),
        parameters: query.parameters.clone(),
        timeout_ms: query.timeout_ms,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assistant::{
        RequestGrouping, RequestOperation, RequestOutput, RequestPii, RequestShape, RequestSubject,
    };
    use crate::knowledge::model::{
        CapabilityDefaults, CapabilityGuards, CapabilityKnowledge, QueryKnowledge,
        QueryOutputField, QueryParameter, Sensitivity,
    };

    fn capability() -> CapabilityKnowledge {
        CapabilityKnowledge {
            id: "savings_deposit_total".into(),
            status: "approved_mvp".into(),
            domain: "savings".into(),
            query_id: "savings.deposit_total".into(),
            dataset_recipe: None,
            output_mode: "summary".into(),
            request_shape: RequestShape {
                operation: RequestOperation::Total,
                subject: RequestSubject::SavingsTransaction,
                grouping: RequestGrouping::None,
                output: RequestOutput::Scalar,
                pii: RequestPii::None,
            },
            display_name: None,
            description: None,
            data_areas: Vec::new(),
            metrics: Vec::new(),
            examples: Vec::new(),
            continuation: false,
            required_parameters: Vec::new(),
            optional_parameters: Vec::new(),
            defaults: CapabilityDefaults::default(),
            guards: CapabilityGuards::default(),
            supported_intents: Vec::new(),
            unsupported_intents: Vec::new(),
            parameter_policies: Vec::new(),
        }
    }

    fn query() -> QueryKnowledge {
        QueryKnowledge {
            id: "savings.deposit_total".into(),
            database: "fineract".into(),
            sql_file: "queries/savings/deposit_total.sql".into(),
            data_areas: Vec::new(),
            tables: vec!["m_savings_account_transaction".into()],
            metrics: Vec::new(),
            parameters: vec![QueryParameter {
                name: "office_ids".into(),
                kind: "array_bigint".into(),
                required: true,
                source: Some("authorized_scope".into()),
            }],
            output_fields: vec![QueryOutputField {
                name: "total_deposit_amount".into(),
                kind: "decimal".into(),
                sensitivity: Sensitivity::PublicBusiness,
            }],
            timeout_ms: Some(3000),
        }
    }

    #[test]
    fn derives_a_single_shape_with_no_fragment_and_no_filters() {
        let dataset = degenerate_dataset(&capability(), &query());
        assert_eq!(dataset.id, "savings.deposit_total");
        assert!(
            dataset.filters.is_empty(),
            "degenerate dataset has no filter slots"
        );
        assert!(
            dataset.order_by.is_empty(),
            "ordering stays inside the source SQL"
        );
        assert_eq!(dataset.shapes.len(), 1);
        assert!(
            dataset.shapes[0].fragment.is_none(),
            "no fragment means the source SQL is already complete"
        );
        assert_eq!(dataset.shapes[0].request_shape, capability().request_shape);
    }

    #[test]
    fn carries_source_parameters_and_timeout_unchanged() {
        let dataset = degenerate_dataset(&capability(), &query());
        assert_eq!(dataset.source_sql, "queries/savings/deposit_total.sql");
        assert_eq!(dataset.database, "fineract");
        assert_eq!(dataset.parameters, query().parameters);
        assert_eq!(dataset.timeout_ms, Some(3000));
        assert_eq!(dataset.tables, query().tables);
    }

    #[test]
    fn every_output_field_is_core_so_projection_is_a_no_op() {
        let dataset = degenerate_dataset(&capability(), &query());
        assert_eq!(dataset.output_fields.len(), 1);
        assert!(
            dataset.output_fields.iter().all(|field| field.core),
            "Phase A must not change which columns render"
        );
        assert_eq!(dataset.core_field_names(), vec!["total_deposit_amount"]);
    }
}
