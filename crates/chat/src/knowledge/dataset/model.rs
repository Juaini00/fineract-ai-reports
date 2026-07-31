//! The dataset contract: one source, plus whitelists for filters, shapes and
//! ordering. See docs/superpowers/specs/2026-07-31-dataset-model-design.md.

use serde::Deserialize;

use crate::assistant::RequestShape;
use crate::knowledge::model::{QueryParameter, Sensitivity};

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct DatasetKnowledge {
    pub id: String,
    pub database: String,
    /// Path to the authored source SQL (joins + office scope), relative to the
    /// repository root when it starts with `queries/`.
    pub source_sql: String,

    #[serde(default)]
    pub tables: Vec<String>,

    #[serde(default)]
    pub filters: Vec<FilterSlot>,

    pub shapes: Vec<ShapeOption>,

    #[serde(default)]
    pub order_by: Vec<OrderByOption>,

    #[serde(default)]
    pub output_fields: Vec<DatasetOutputField>,

    #[serde(default)]
    pub parameters: Vec<QueryParameter>,

    #[serde(default)]
    pub timeout_ms: Option<u64>,
}

impl DatasetKnowledge {
    /// Fields rendered for every request, regardless of projection hints.
    pub fn core_field_names(&self) -> Vec<String> {
        self.output_fields
            .iter()
            .filter(|field| field.core)
            .map(|field| field.name.clone())
            .collect()
    }

    pub fn shape(&self, shape_id: &str) -> Option<&ShapeOption> {
        self.shapes.iter().find(|shape| shape.id == shape_id)
    }

    pub fn order_by_expr(&self, order_by_id: &str) -> Option<&str> {
        self.order_by
            .iter()
            .find(|option| option.id == order_by_id)
            .map(|option| option.expr.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct FilterSlot {
    /// The id the LLM refers to. Never a SQL identifier.
    pub id: String,
    /// The SQL column expression. Validated by `grammar::validate_sql_expr`.
    pub expr: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub operators: Vec<FilterOperator>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FilterOperator {
    Eq,
    Lt,
    Lte,
    Gt,
    Gte,
    Between,
}

impl FilterOperator {
    /// SQL operator text. `Between` is expanded by the composer, not here.
    pub fn as_sql(self) -> &'static str {
        match self {
            Self::Eq => "=",
            Self::Lt => "<",
            Self::Lte => "<=",
            Self::Gt => ">",
            Self::Gte => ">=",
            Self::Between => "BETWEEN",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ShapeOption {
    pub id: String,
    pub request_shape: RequestShape,
    /// Path to the authored SQL fragment applied over the `base` CTE. `None`
    /// means degenerate passthrough: the source SQL is already complete.
    #[serde(default)]
    pub fragment: Option<String>,
    #[serde(default)]
    pub order_by: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct OrderByOption {
    pub id: String,
    pub expr: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct DatasetOutputField {
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub sensitivity: Sensitivity,
    /// Rendered for every request. Non-core fields are opt-in via projection.
    #[serde(default)]
    pub core: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
id: savings.account_charges
database: fineract
source_sql: queries/savings/account_charges.source.sql
tables: [m_savings_account_charge, m_client]
filters:
  - id: due_date
    expr: sac.charge_due_date
    type: date
    operators: [eq, lt, between]
shapes:
  - id: list
    request_shape:
      operation: list
      subject: savings_account_charge
      grouping: none
      output: list
    order_by: [created_desc]
order_by:
  - id: created_desc
    expr: sac.created_on_utc DESC, sac.id DESC
output_fields:
  - name: savings_account_charge_id
    type: bigint
    sensitivity: public_business
    core: true
  - name: client_display_name
    type: string
    sensitivity: pii
parameters:
  - name: office_ids
    type: array_bigint
    required: true
    source: authorized_scope
"#;

    #[test]
    fn parses_dataset_yaml() {
        let dataset: DatasetKnowledge = serde_yaml::from_str(SAMPLE).unwrap();
        assert_eq!(dataset.id, "savings.account_charges");
        assert_eq!(dataset.filters.len(), 1);
        assert_eq!(dataset.filters[0].id, "due_date");
        assert_eq!(
            dataset.filters[0].operators,
            vec![
                FilterOperator::Eq,
                FilterOperator::Lt,
                FilterOperator::Between
            ]
        );
        assert_eq!(dataset.shapes.len(), 1);
        assert_eq!(dataset.shapes[0].order_by, vec!["created_desc".to_string()]);
        assert!(dataset.shapes[0].fragment.is_none());
        assert_eq!(
            dataset.order_by[0].expr,
            "sac.created_on_utc DESC, sac.id DESC"
        );
    }

    #[test]
    fn core_defaults_to_false_and_is_read_when_present() {
        let dataset: DatasetKnowledge = serde_yaml::from_str(SAMPLE).unwrap();
        assert!(dataset.output_fields[0].core);
        assert!(!dataset.output_fields[1].core);
    }

    #[test]
    fn core_field_names_returns_only_core_fields() {
        let dataset: DatasetKnowledge = serde_yaml::from_str(SAMPLE).unwrap();
        assert_eq!(
            dataset.core_field_names(),
            vec!["savings_account_charge_id"]
        );
    }
}
