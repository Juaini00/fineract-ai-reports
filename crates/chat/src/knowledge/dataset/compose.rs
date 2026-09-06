//! Assembles one statement from authored SQL plus declared expressions.
//!
//! Inputs are file *contents*, never paths, so composition stays pure. Every
//! character of the result originates in an authored file or a declared `expr`
//! that has passed `grammar::validate_sql_expr`.

use anyhow::{Result, bail};

use crate::knowledge::dataset::grammar::validate_sql_expr;
use crate::knowledge::dataset::model::{DatasetKnowledge, FilterOperator};

/// One `$n` placeholder reserved for a declared filter slot + operator pair.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilterBind {
    pub filter_id: String,
    pub operator: FilterOperator,
    /// 1-based positional placeholder index in the composed statement.
    pub placeholder: usize,
    /// Maximum values accepted for an `in` bind, inherited from the selected
    /// shape's row cap. Other operators have no array bound.
    pub max_array_len: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComposedSql {
    pub sql: String,
    pub filter_binds: Vec<FilterBind>,
}

pub fn compose(
    dataset: &DatasetKnowledge,
    shape_id: &str,
    order_by_id: Option<&str>,
    source_sql: &str,
    fragment_sql: Option<&str>,
) -> Result<ComposedSql> {
    let Some(shape) = dataset.shape(shape_id) else {
        bail!("dataset {} has no shape {shape_id}", dataset.id);
    };

    // Degenerate dataset: the source SQL is already a complete statement.
    // Returning it verbatim is what keeps Phase A byte-identical.
    if shape.fragment.is_none() && dataset.filters.is_empty() && order_by_id.is_none() {
        return Ok(ComposedSql {
            sql: source_sql.to_string(),
            filter_binds: Vec::new(),
        });
    }

    let Some(fragment_sql) = fragment_sql else {
        bail!(
            "shape {shape_id} of dataset {} requires a fragment",
            dataset.id
        );
    };

    let mut placeholder = shape.parameters(dataset).len();
    let mut predicates = String::new();
    let mut filter_binds = Vec::new();
    for filter in &dataset.filters {
        validate_sql_expr(&filter.expr).map_err(|reason| {
            anyhow::anyhow!("dataset {} filter {}: {reason}", dataset.id, filter.id)
        })?;
        for operator in &filter.operators {
            placeholder += 1;
            predicates.push_str(&predicate(
                &filter.expr,
                *operator,
                &filter.kind,
                filter.case_insensitive,
                placeholder,
            )?);
            filter_binds.push(FilterBind {
                filter_id: filter.id.clone(),
                operator: *operator,
                placeholder,
                max_array_len: (operator == &FilterOperator::In)
                    .then_some(shape.row_cap)
                    .flatten(),
            });
            if matches!(operator, FilterOperator::Between) {
                // BETWEEN consumes a second placeholder for the upper bound.
                placeholder += 1;
            }
        }
    }

    let order_by_clause = match order_by_id {
        Some(id) => {
            let Some(expr) = dataset.order_by_expr(id) else {
                bail!("dataset {} has no order_by {id}", dataset.id);
            };
            validate_sql_expr(expr).map_err(|reason| {
                anyhow::anyhow!("dataset {} order_by {id}: {reason}", dataset.id)
            })?;
            format!("\nORDER BY {expr}")
        }
        None => String::new(),
    };

    let sql = format!(
        "WITH source AS (\n{source_sql}\n),\nbase AS (\n  SELECT *\n  FROM source\n  WHERE TRUE{predicates}\n)\n{fragment_sql}{order_by_clause}"
    );

    Ok(ComposedSql { sql, filter_binds })
}

/// Null-passthrough predicate. An inactive filter binds NULL and the predicate
/// short-circuits, so the statement text is identical whether or not a filter
/// is used. That property is what keeps the validator's cross product at
/// `shapes x order_by` instead of `2^filters x shapes x order_by`.
fn predicate(
    expr: &str,
    operator: FilterOperator,
    kind: &str,
    case_insensitive: bool,
    placeholder: usize,
) -> Result<String> {
    let Some(cast) = filter_cast(kind) else {
        bail!("unsupported dataset filter type {kind}");
    };
    if case_insensitive {
        if kind != "string" || operator != FilterOperator::Eq {
            bail!("case_insensitive filters require string equality");
        }
        return Ok(format!(
            "\n    AND (${placeholder}{cast} IS NULL OR lower({expr}) = lower(${placeholder}{cast}))"
        ));
    }
    Ok(match operator {
        FilterOperator::In => format!(
            "\n  AND (${placeholder}{cast}[] IS NULL OR {expr} = ANY(${placeholder}{cast}[]))"
        ),
        FilterOperator::Between => format!(
            "\n  AND (${placeholder}{cast} IS NULL OR ${next}{cast} IS NULL OR {expr} BETWEEN ${placeholder} AND ${next})",
            next = placeholder + 1
        ),
        _ => format!(
            "\n  AND (${placeholder}{cast} IS NULL OR {expr} {op} ${placeholder})",
            op = operator.as_sql()
        ),
    })
}

fn filter_cast(kind: &str) -> Option<&'static str> {
    match kind {
        "date" => Some("::date"),
        "integer" => Some("::bigint"),
        "boolean" => Some("::bool"),
        "string" => Some("::text"),
        "decimal" => Some("::numeric"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assistant::{
        RequestGrouping, RequestOperation, RequestOutput, RequestPii, RequestShape, RequestSubject,
    };
    use crate::knowledge::dataset::model::{
        DatasetKnowledge, FilterInputPolicy, FilterOperator, FilterSlot, OrderByOption, ShapeOption,
    };
    use crate::knowledge::model::QueryParameter;

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
            role: Default::default(),
            expected_cardinality: None,
            row_cap: None,
            grouped_by: None,
            produces: Vec::new(),
            fragment: fragment.map(str::to_string),
            order_by: order_by.into_iter().map(str::to_string).collect(),
            output_fields: Vec::new(),
            parameters: Vec::new(),
        }
    }

    fn dataset(filters: Vec<FilterSlot>, shapes: Vec<ShapeOption>) -> DatasetKnowledge {
        DatasetKnowledge {
            id: "test.dataset".into(),
            database: "fineract".into(),
            source_sql: "queries/test.sql".into(),
            tables: vec!["m_client".into()],
            filters,
            entity: None,
            filters_exempt: Vec::new(),
            shapes,
            order_by: vec![OrderByOption {
                id: "created_desc".into(),
                expr: "sac.created_on_utc DESC, sac.id DESC".into(),
            }],
            output_fields: Vec::new(),
            // Every fixture's source SQL embeds exactly one `$1` (office scope),
            // so `parameters` must declare exactly that one placeholder — this
            // is what sizes the base the composer's filter placeholders start
            // counting from.
            parameters: vec![QueryParameter {
                name: "office_ids".into(),
                kind: "array_bigint".into(),
                required: true,
                source: Some("authorized_scope".into()),
            }],
            timeout_ms: None,
        }
    }

    #[test]
    fn degenerate_dataset_returns_source_sql_verbatim() {
        let source = "SELECT a, b\nFROM m_client c\nWHERE c.office_id = ANY($1::bigint[])\nORDER BY c.id\nLIMIT $2;";
        let data = dataset(Vec::new(), vec![shape("legacy", None, Vec::new())]);

        let composed = compose(&data, "legacy", None, source, None).unwrap();

        assert_eq!(composed.sql, source, "Phase A must not alter legacy SQL");
        assert!(composed.filter_binds.is_empty());
    }

    #[test]
    fn wraps_source_in_a_cte_and_appends_fragment_and_order_by() {
        let source = "SELECT a FROM m_client c WHERE c.office_id = ANY($1::bigint[])";
        let data = dataset(
            Vec::new(),
            vec![shape("list", Some("unused"), vec!["created_desc"])],
        );

        let composed = compose(
            &data,
            "list",
            Some("created_desc"),
            source,
            Some("SELECT * FROM base"),
        )
        .unwrap();

        assert_eq!(
            composed.sql,
            "WITH source AS (\nSELECT a FROM m_client c WHERE c.office_id = ANY($1::bigint[])\n),\nbase AS (\n  SELECT *\n  FROM source\n  WHERE TRUE\n)\nSELECT * FROM base\nORDER BY sac.created_on_utc DESC, sac.id DESC"
        );
    }

    #[test]
    fn applies_computed_and_case_insensitive_filters_outside_source() {
        let filters = vec![FilterSlot {
            id: "client_name".into(),
            expr: "client_display_name".into(),
            kind: "string".into(),
            case_insensitive: true,
            input_policy: FilterInputPolicy::Ordinary,
            operators: vec![FilterOperator::Eq],
        }];
        let data = dataset(filters, vec![shape("lookup", Some("f"), Vec::new())]);

        let composed = compose(
            &data,
            "lookup",
            None,
            "SELECT row_number() OVER () AS latest_rank, c.display_name AS client_display_name FROM m_client c WHERE c.office_id = ANY($1::bigint[])",
            Some("SELECT * FROM base WHERE latest_rank = 1"),
        )
        .unwrap();

        assert!(composed.sql.contains("WITH source AS"));
        assert!(composed.sql.contains("FROM source"));
        assert!(
            composed
                .sql
                .contains("lower(client_display_name) = lower($2::text)")
        );
        assert!(
            !composed
                .sql
                .contains("WHERE c.office_id = ANY($1::bigint[])\n    AND")
        );
    }

    #[test]
    fn emits_one_null_passthrough_predicate_per_filter_operator() {
        let filters = vec![FilterSlot {
            id: "due_date".into(),
            expr: "sac.charge_due_date".into(),
            kind: "date".into(),
            case_insensitive: false,
            input_policy: FilterInputPolicy::Ordinary,
            operators: vec![FilterOperator::Eq, FilterOperator::Lt],
        }];
        let data = dataset(filters, vec![shape("list", Some("f"), Vec::new())]);

        let composed = compose(
            &data,
            "list",
            None,
            "SELECT a FROM t WHERE x = $1",
            Some("SELECT * FROM base"),
        )
        .unwrap();

        assert!(
            composed
                .sql
                .contains("($2::date IS NULL OR sac.charge_due_date = $2)")
        );
        assert!(
            composed
                .sql
                .contains("($3::date IS NULL OR sac.charge_due_date < $3)")
        );
        assert_eq!(composed.filter_binds.len(), 2);
        assert_eq!(composed.filter_binds[0].filter_id, "due_date");
        assert_eq!(composed.filter_binds[0].operator, FilterOperator::Eq);
        assert_eq!(composed.filter_binds[0].placeholder, 2);
        assert_eq!(composed.filter_binds[1].placeholder, 3);
    }

    #[test]
    fn in_filter_uses_a_bound_array_without_interpolating_values() {
        let filters = vec![FilterSlot {
            id: "client_ids".into(),
            expr: "client_id".into(),
            kind: "integer".into(),
            case_insensitive: false,
            input_policy: FilterInputPolicy::Ordinary,
            operators: vec![FilterOperator::In],
        }];
        let mut data = dataset(filters, vec![shape("list", Some("f"), Vec::new())]);
        data.shapes[0].row_cap = Some(25);

        let composed = compose(
            &data,
            "list",
            None,
            "SELECT a FROM t WHERE x = $1",
            Some("SELECT * FROM base"),
        )
        .unwrap();

        assert!(composed.sql.contains("client_id = ANY($2::bigint[])"));
        assert!(!composed.sql.contains("[42, 99]"));
        assert_eq!(composed.filter_binds[0].operator, FilterOperator::In);
        assert_eq!(composed.filter_binds[0].placeholder, 2);
        assert_eq!(composed.filter_binds[0].max_array_len, Some(25));
    }

    #[test]
    fn between_reserves_two_placeholders_so_later_operators_do_not_collide() {
        let filters = vec![FilterSlot {
            id: "due_date".into(),
            expr: "sac.charge_due_date".into(),
            kind: "date".into(),
            case_insensitive: false,
            input_policy: FilterInputPolicy::Ordinary,
            operators: vec![FilterOperator::Between, FilterOperator::Eq],
        }];
        let data = dataset(filters, vec![shape("list", Some("f"), Vec::new())]);

        let composed = compose(
            &data,
            "list",
            None,
            "SELECT a FROM t WHERE x = $1",
            Some("SELECT * FROM base"),
        )
        .unwrap();

        assert!(
            composed
                .sql
                .contains("sac.charge_due_date BETWEEN $2 AND $3")
        );
        // Eq must take $4, not $3, or it would reuse BETWEEN's upper bound.
        assert!(
            composed
                .sql
                .contains("($4::date IS NULL OR sac.charge_due_date = $4)")
        );
        assert_eq!(composed.filter_binds[0].placeholder, 2);
        assert_eq!(composed.filter_binds[1].placeholder, 4);
    }

    #[test]
    fn rejects_unknown_filter_type_instead_of_defaulting_to_text() {
        let filters = vec![FilterSlot {
            id: "unknown".into(),
            expr: "sac.charge_due_date".into(),
            kind: "jsonb".into(),
            case_insensitive: false,
            input_policy: FilterInputPolicy::Ordinary,
            operators: vec![FilterOperator::Eq],
        }];
        let data = dataset(filters, vec![shape("list", Some("f"), Vec::new())]);

        let error = compose(
            &data,
            "list",
            None,
            "SELECT a FROM t WHERE x = $1",
            Some("SELECT * FROM base"),
        )
        .unwrap_err()
        .to_string();

        assert!(
            error.contains("unsupported dataset filter type"),
            "got: {error}"
        );
    }

    #[test]
    fn decimal_filter_uses_numeric_cast() {
        let filters = vec![FilterSlot {
            id: "amount".into(),
            expr: "t.amount".into(),
            kind: "decimal".into(),
            case_insensitive: false,
            input_policy: FilterInputPolicy::Ordinary,
            operators: vec![FilterOperator::Eq],
        }];
        let data = dataset(filters, vec![shape("list", Some("f"), Vec::new())]);

        let composed = compose(
            &data,
            "list",
            None,
            "SELECT a FROM t WHERE x = $1",
            Some("SELECT * FROM base"),
        )
        .unwrap();

        assert!(composed.sql.contains("$2::numeric"));
    }

    #[test]
    fn rejects_an_order_by_expression_that_fails_the_grammar() {
        let mut data = dataset(Vec::new(), vec![shape("list", Some("f"), vec!["evil"])]);
        data.order_by.push(OrderByOption {
            id: "evil".into(),
            expr: "sac.id; DROP TABLE m_client".into(),
        });

        let error = compose(
            &data,
            "list",
            Some("evil"),
            "SELECT a FROM t",
            Some("SELECT * FROM base"),
        )
        .unwrap_err()
        .to_string();

        assert!(error.contains("forbidden token"), "got: {error}");
    }

    #[test]
    fn rejects_unknown_shape_and_unknown_order_by() {
        let data = dataset(Vec::new(), vec![shape("list", Some("f"), Vec::new())]);

        assert!(compose(&data, "nope", None, "SELECT a", Some("SELECT * FROM base")).is_err());
        assert!(
            compose(
                &data,
                "list",
                Some("nope"),
                "SELECT a",
                Some("SELECT * FROM base")
            )
            .is_err()
        );
    }
}
