//! Enumerates every executable statement a dataset can produce.
//!
//! Filters deliberately do not appear here. Because they are emitted as
//! null-passthrough predicates, statement text is identical whether a filter is
//! active or not, so the cross product stays `shapes x order_by` rather than
//! `2^filters x shapes x order_by`.

use crate::knowledge::dataset::model::DatasetKnowledge;

pub fn executable_combinations(dataset: &DatasetKnowledge) -> Vec<(String, Option<String>)> {
    let mut combinations = Vec::new();
    for shape in &dataset.shapes {
        if shape.order_by.is_empty() {
            combinations.push((shape.id.clone(), None));
            continue;
        }
        for order_by_id in &shape.order_by {
            combinations.push((shape.id.clone(), Some(order_by_id.clone())));
        }
    }
    combinations
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assistant::{
        RequestGrouping, RequestOperation, RequestOutput, RequestPii, RequestShape, RequestSubject,
    };
    use crate::knowledge::dataset::model::{DatasetKnowledge, OrderByOption, ShapeOption};

    fn shape(id: &str, fragment: Option<&str>, order_by: Vec<&str>) -> ShapeOption {
        ShapeOption {
            id: id.into(),
            request_shape: RequestShape {
                operation: RequestOperation::List,
                subject: RequestSubject::SavingsAccountCharge,
                grouping: RequestGrouping::None,
                output: RequestOutput::List,
                pii: RequestPii::None,
            },
            fragment: fragment.map(str::to_string),
            order_by: order_by.into_iter().map(str::to_string).collect(),
        }
    }

    fn dataset(shapes: Vec<ShapeOption>, order_by: Vec<&str>) -> DatasetKnowledge {
        DatasetKnowledge {
            id: "test.dataset".into(),
            database: "fineract".into(),
            source_sql: "queries/test.sql".into(),
            tables: Vec::new(),
            filters: Vec::new(),
            shapes,
            order_by: order_by
                .into_iter()
                .map(|id| OrderByOption {
                    id: id.into(),
                    expr: format!("t.{id}"),
                })
                .collect(),
            output_fields: Vec::new(),
            parameters: Vec::new(),
            timeout_ms: None,
        }
    }

    #[test]
    fn degenerate_dataset_has_exactly_one_combination_with_no_ordering() {
        let data = dataset(vec![shape("legacy", None, Vec::new())], Vec::new());
        assert_eq!(
            executable_combinations(&data),
            vec![("legacy".to_string(), None)]
        );
    }

    #[test]
    fn expands_each_shape_across_its_declared_order_by_options() {
        let data = dataset(
            vec![
                shape("list", Some("f"), vec!["a", "b"]),
                shape("total", Some("f"), Vec::new()),
            ],
            vec!["a", "b"],
        );

        assert_eq!(
            executable_combinations(&data),
            vec![
                ("list".to_string(), Some("a".to_string())),
                ("list".to_string(), Some("b".to_string())),
                ("total".to_string(), None),
            ]
        );
    }

    #[test]
    fn filters_do_not_multiply_the_combination_count() {
        use crate::knowledge::dataset::model::{FilterOperator, FilterSlot};

        let mut data = dataset(vec![shape("list", Some("f"), vec!["a"])], vec!["a"]);
        data.filters = vec![
            FilterSlot {
                id: "due_date".into(),
                expr: "t.due_date".into(),
                kind: "date".into(),
                operators: vec![FilterOperator::Eq, FilterOperator::Lt],
            },
            FilterSlot {
                id: "is_paid".into(),
                expr: "t.is_paid".into(),
                kind: "boolean".into(),
                operators: vec![FilterOperator::Eq],
            },
        ];

        assert_eq!(
            executable_combinations(&data).len(),
            1,
            "null-passthrough keeps statement text identical regardless of filters"
        );
    }
}
