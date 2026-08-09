//! The dataset contract: one source, plus whitelists for filters, shapes and
//! ordering. See docs/superpowers/specs/2026-07-31-dataset-model-design.md.

use serde::{Deserialize, Serialize};
use serde_json::Value;

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

    /// Entity identity and safe labels for resolver datasets.
    #[serde(default)]
    pub entity: Option<EntityMetadata>,

    /// Output field names that deliberately have no filter slot. Every string
    /// column a shape returns is narrowable by definition, so authoring one
    /// without a filter must be a stated decision rather than an oversight —
    /// see `validate::validate_dataset`.
    #[serde(default)]
    pub filters_exempt: Vec<String>,

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

/// Catalog-owned recipe attached to an authorized capability. Values are read
/// from normalized capability parameters; no SQL identifiers come from the LLM.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct DatasetRecipe {
    pub dataset_id: String,
    pub shape_id: String,
    #[serde(default)]
    pub order_by_id: Option<String>,
    #[serde(default)]
    pub filters: Vec<DatasetRecipeFilter>,
    #[serde(default)]
    pub projection: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct DatasetRecipeFilter {
    pub filter_id: String,
    pub operator: FilterOperator,
    #[serde(default)]
    pub parameter: Option<String>,
    #[serde(default)]
    pub value: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DatasetSelection {
    pub dataset_id: String,
    pub shape_id: String,
    #[serde(default)]
    pub order_by_id: Option<String>,
    #[serde(default)]
    pub filters: Vec<DatasetFilterSelection>,
    #[serde(default)]
    pub projection: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DatasetFilterSelection {
    pub filter_id: String,
    pub operator: FilterOperator,
    pub value: Value,
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
    #[serde(default)]
    pub case_insensitive: bool,
    #[serde(default)]
    pub input_policy: FilterInputPolicy,
    pub operators: Vec<FilterOperator>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FilterInputPolicy {
    #[default]
    Ordinary,
    ExactIdentifier,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FilterOperator {
    Eq,
    In,
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
            Self::In => "= ANY",
            Self::Lt => "<",
            Self::Lte => "<=",
            Self::Gt => ">",
            Self::Gte => ">=",
            Self::Between => "BETWEEN",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct EntityMetadata {
    pub kind: String,
    pub id_field: String,
    #[serde(default)]
    pub label_fields: Vec<String>,
    pub label_fallback: String,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShapeRole {
    #[default]
    Terminal,
    Resolver,
    Probe,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Cardinality {
    Zero,
    One,
    Many,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ProducedSlot {
    pub slot: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub sensitivity: Sensitivity,
    pub cardinality: Cardinality,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ShapeOption {
    pub id: String,
    pub request_shape: RequestShape,
    #[serde(default)]
    pub role: ShapeRole,
    #[serde(default)]
    pub expected_cardinality: Option<Cardinality>,
    #[serde(default)]
    pub row_cap: Option<u32>,
    /// Declared grouping key used by the workflow compiler to reject an N+1
    /// expansion when this one reviewed shape can produce all groups at once.
    #[serde(default)]
    pub grouped_by: Option<String>,
    #[serde(default)]
    pub produces: Vec<ProducedSlot>,
    /// Path to the authored SQL fragment applied over the `base` CTE. `None`
    /// means degenerate passthrough: the source SQL is already complete.
    #[serde(default)]
    pub fragment: Option<String>,
    #[serde(default)]
    pub order_by: Vec<String>,
    #[serde(default)]
    pub output_fields: Vec<DatasetOutputField>,
    #[serde(default)]
    pub parameters: Vec<QueryParameter>,
}

impl ShapeOption {
    pub fn output_fields<'a>(&'a self, dataset: &'a DatasetKnowledge) -> &'a [DatasetOutputField] {
        if self.output_fields.is_empty() {
            &dataset.output_fields
        } else {
            &self.output_fields
        }
    }

    pub fn parameters<'a>(&'a self, dataset: &'a DatasetKnowledge) -> &'a [QueryParameter] {
        if self.parameters.is_empty() {
            &dataset.parameters
        } else {
            &self.parameters
        }
    }
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
    fn parses_resolver_entity_and_shape_metadata_defaults() {
        let dataset: DatasetKnowledge = serde_yaml::from_str(
            r#"
id: client.identity
database: fineract
source_sql: queries/datasets/client/identity.source.sql
entity:
  kind: client
  id_field: client_id
  label_fields: [display_name, office_name]
  label_fallback: "Client {client_id}"
shapes:
  - id: identity_candidates
    role: resolver
    expected_cardinality: many
    row_cap: 25
    grouped_by: client_id
    produces:
      - slot: client_id
        type: integer
        sensitivity: public_business
        cardinality: many
    request_shape:
      operation: lookup
      subject: client
      grouping: none
      output: lookup
"#,
        )
        .unwrap();

        let entity = dataset.entity.unwrap();
        assert_eq!(entity.kind, "client");
        assert_eq!(entity.id_field, "client_id");
        assert_eq!(entity.label_fields, ["display_name", "office_name"]);
        assert_eq!(entity.label_fallback, "Client {client_id}");
        assert_eq!(dataset.shapes[0].role, ShapeRole::Resolver);
        assert_eq!(
            dataset.shapes[0].expected_cardinality,
            Some(Cardinality::Many)
        );
        assert_eq!(dataset.shapes[0].row_cap, Some(25));
        assert_eq!(dataset.shapes[0].grouped_by.as_deref(), Some("client_id"));
        assert_eq!(dataset.shapes[0].produces[0].slot, "client_id");

        let legacy: DatasetKnowledge = serde_yaml::from_str(SAMPLE).unwrap();
        assert!(legacy.entity.is_none());
        assert_eq!(legacy.shapes[0].role, ShapeRole::Terminal);
        assert!(legacy.shapes[0].expected_cardinality.is_none());
        assert!(legacy.shapes[0].row_cap.is_none());
        assert!(legacy.shapes[0].grouped_by.is_none());
        assert!(legacy.shapes[0].produces.is_empty());
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
