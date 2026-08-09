use std::collections::BTreeMap;
use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::assistant::intent::RequestShape;
use crate::assistant::{ClarificationFieldType, ClarificationValidation, ConstraintField};
use crate::knowledge::dataset::model::{DatasetKnowledge, DatasetRecipe};

#[derive(Debug, Clone)]
pub struct KnowledgeCatalog {
    pub root_path: PathBuf,
    pub query_path: PathBuf,
    pub data_areas: Vec<DataAreasKnowledge>,
    pub domains: Vec<DomainKnowledge>,
    pub schemas: Vec<GenericKnowledge>,
    pub metrics: Vec<GenericKnowledge>,
    pub capabilities: Vec<CapabilityKnowledge>,
    pub queries: Vec<QueryKnowledge>,
    pub policies: Vec<GenericKnowledge>,
    pub responses: Vec<GenericKnowledge>,
    pub parameter_inputs: Vec<ParameterInputKnowledge>,
    /// Query parameter name -> the canonical facts that may fill it, in
    /// precedence order. Declared once in `knowledge/parameter-bindings/`.
    pub parameter_bindings: BTreeMap<String, Vec<ConstraintField>>,
    pub classification: ClassificationPolicy,
    pub datasets: Vec<DatasetKnowledge>,
}

/// The catalog's answer to "what fills this query parameter?".
///
/// Before this existed, three unrelated Rust match arms each answered it by
/// parameter name — `input_satisfied`, `effective_parameter` and
/// `params_from_verified` — and they disagreed. Six of the nine declared
/// clarification inputs fell through `input_satisfied`'s `_ => false`, so they
/// were reported missing on every turn no matter what the user had typed.
///
/// It is a single flat map rather than a field on `ParameterInputKnowledge`
/// because a clarification input can only describe an *askable* parameter:
/// the registry rejects anything that is not a single date/integer/text field,
/// which excludes `product_ids` (array) and `as_of_date`. Binding must cover
/// every parameter, askable or not, so it gets its own declaration.
#[derive(Debug, Clone, Deserialize)]
pub struct ParameterBindingKnowledge {
    pub bindings: BTreeMap<String, Vec<ConstraintField>>,
}

impl KnowledgeCatalog {
    /// Canonical facts that may fill `parameter`, in declared precedence order.
    /// Empty means "no fact binds this" — the validator refuses to load a
    /// catalog where a query parameter has no entry at all, so an empty slice
    /// here is a deliberate declaration (a parameter only a policy default
    /// fills), never a forgotten one.
    pub fn binding_fields(&self, parameter: &str) -> &[ConstraintField] {
        self.parameter_bindings
            .get(parameter)
            .map_or(&[], Vec::as_slice)
    }

    /// Resolve any spelling of a metric — canonical id, legacy id, or a
    /// natural-language surface form — to the canonical id of the metric that
    /// declares it.
    ///
    /// The alias list lives in `knowledge/metrics/*.yaml` beside the metric it
    /// describes, so adding a spelling is a catalog edit. This replaces a
    /// hand-written Rust match that had drifted far enough to normalize three
    /// phrases onto metric ids no definition file declared, which made
    /// `verify_capability_metric` reject the very capabilities those phrases
    /// name.
    pub fn resolve_metric_id(&self, raw: &str) -> Option<&str> {
        let needle = normalize_metric_key(raw);
        self.metrics
            .iter()
            .find(|metric| {
                normalize_metric_key(&metric.id) == needle
                    || metric_aliases(metric).any(|alias| normalize_metric_key(alias) == needle)
            })
            .map(|metric| metric.id.as_str())
    }
}

/// Aliases ride in `GenericKnowledge::content` via `#[serde(flatten)]`, so no
/// model change is needed to declare them.
pub fn metric_aliases(metric: &GenericKnowledge) -> impl Iterator<Item = &str> {
    metric
        .content
        .get("aliases")
        .and_then(|value| value.as_array())
        .into_iter()
        .flatten()
        .filter_map(|value| value.as_str())
}

/// Compare metric spellings without caring about `.` vs `_` vs spaces or case,
/// so `savings.deposit_total`, `savings_deposit_total` and `deposit volume`
/// all reduce to a comparable key.
fn normalize_metric_key(value: &str) -> String {
    value
        .chars()
        .filter_map(|character| match character {
            '.' | '_' | '-' | ' ' => None,
            other => Some(other.to_ascii_lowercase()),
        })
        .collect()
}

#[derive(Debug, Clone, Deserialize)]
pub struct ClassificationPolicy {
    pub min_gap: f32,
    pub min_floor: f32,
    pub others_key: String,
    pub others_label: String,
    #[serde(default)]
    pub lqr: LqrPolicy,
}

impl Default for ClassificationPolicy {
    fn default() -> Self {
        Self {
            min_gap: 0.05,
            min_floor: 0.40,
            others_key: "other_activity".to_string(),
            others_label: "Others — let me describe it in my own words".to_string(),
            lqr: LqrPolicy::default(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct LqrPolicy {
    #[serde(default = "default_domain_floor")]
    pub domain_min_floor: f32,
    #[serde(default = "default_domain_gap")]
    pub domain_min_gap: f32,
    #[serde(default = "default_cap_floor")]
    pub capability_min_floor: f32,
    #[serde(default = "default_cap_gap")]
    pub capability_min_gap: f32,
    #[serde(default = "default_retry_budget")]
    pub retry_budget: u8,
    #[serde(default)]
    pub score_aggregation: ScoreAggregation,
}

impl Default for LqrPolicy {
    fn default() -> Self {
        Self {
            domain_min_floor: default_domain_floor(),
            domain_min_gap: default_domain_gap(),
            capability_min_floor: default_cap_floor(),
            capability_min_gap: default_cap_gap(),
            retry_budget: default_retry_budget(),
            score_aggregation: ScoreAggregation::default(),
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScoreAggregation {
    #[default]
    Min,
    Mean,
    Product,
}

fn default_domain_floor() -> f32 {
    0.55
}

fn default_domain_gap() -> f32 {
    0.10
}

fn default_cap_floor() -> f32 {
    0.40
}

fn default_cap_gap() -> f32 {
    0.05
}

fn default_retry_budget() -> u8 {
    2
}

#[derive(Debug, Clone, Deserialize)]
pub struct GenericKnowledge {
    pub id: String,

    #[serde(default)]
    pub status: Option<String>,

    #[serde(default)]
    pub domain: Option<String>,

    #[serde(default)]
    pub data_areas: Vec<String>,

    #[serde(default)]
    pub checks: Vec<serde_json::Value>,

    #[serde(flatten)]
    pub content: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DataAreasKnowledge {
    pub id: String,
    pub status: String,

    #[serde(default)]
    pub included_tables: Vec<String>,

    #[serde(default)]
    pub conditional_tables: Vec<String>,

    #[serde(default)]
    pub excluded_tables: Vec<String>,

    #[serde(default)]
    pub allowed_domains: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DomainKnowledge {
    pub id: String,
    pub status: String,

    #[serde(default)]
    pub display_name: Option<String>,

    #[serde(default)]
    pub description: Option<String>,

    #[serde(default)]
    pub data_areas: Vec<String>,

    #[serde(default)]
    pub concepts: Vec<DomainConcept>,

    #[serde(default)]
    pub supported_intents: Vec<String>,

    #[serde(default)]
    pub unsupported_intents: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DomainConcept {
    pub id: String,

    #[serde(default)]
    pub meaning: Option<String>,

    #[serde(default)]
    pub synonyms: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ParameterInputKnowledge {
    pub id: String,
    pub parameters: Vec<String>,
    #[serde(rename = "type")]
    pub field_type: ClarificationFieldType,
    pub label: String,
    #[serde(default)]
    pub help_text: Option<String>,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub validation: ClarificationValidation,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct CapabilityDefaults {
    #[serde(default)]
    pub default_limit: Option<i64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct CapabilityGuards {
    #[serde(default)]
    pub max_limit: Option<i64>,
    #[serde(default)]
    pub max_date_range_days: Option<u32>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityKind {
    #[default]
    Terminal,
    Resolver,
    Probe,
    Composite,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CapabilityKnowledge {
    pub id: String,
    pub status: String,
    pub domain: String,
    pub query_id: String,
    pub output_mode: String,
    pub request_shape: RequestShape,

    #[serde(default)]
    pub kind: CapabilityKind,

    /// Component capability ids for a composite workflow. Ignored for all
    /// non-composite kinds.
    #[serde(default, rename = "members")]
    pub member_capability_ids: Vec<String>,

    #[serde(default)]
    pub display_name: Option<String>,

    #[serde(default)]
    pub description: Option<String>,

    #[serde(default)]
    pub data_areas: Vec<String>,

    #[serde(default)]
    pub metrics: Vec<String>,

    #[serde(default)]
    pub examples: Vec<String>,

    /// What this capability is for, in the user's terms. Authored in every
    /// capability YAML since the catalog was written, but until now dropped on
    /// the floor by serde — the field simply did not exist here, so the
    /// reranker judged coverage from `display_name`/`description` alone. That
    /// is how `client_list_recent` ("Recently Activated Clients", whose SQL has
    /// no recency predicate at all) came to refuse "all clients from <office>".
    #[serde(default)]
    pub supported_intents: Vec<String>,

    /// Intents this capability must never be selected for. Same history as
    /// `supported_intents`: authored, ignored, now load-bearing.
    #[serde(default)]
    pub unsupported_intents: Vec<String>,

    /// True when this capability can only be entered from a clarification the
    /// assistant itself raised, never from a first-turn message. Its examples
    /// are continuation phrasings ("Continue with the selected client."), so
    /// the first-turn reachability check must skip it rather than pretend the
    /// phrase should bind a parameter no user could have supplied yet.
    #[serde(default)]
    pub continuation: bool,

    #[serde(default)]
    pub required_parameters: Vec<String>,

    #[serde(default)]
    pub optional_parameters: Vec<String>,

    #[serde(default)]
    pub defaults: CapabilityDefaults,

    #[serde(default)]
    pub guards: CapabilityGuards,

    #[serde(default)]
    pub dataset_recipe: Option<DatasetRecipe>,

    /// Per-parameter policy replacing the legacy `required_parameters` /
    /// `optional_parameters` / `clarification.missing_parameters` triad.
    /// Populated by the loader after YAML deserialization; empty until
    /// migration Phase 3 rewrites each capability YAML.
    #[serde(default, skip)]
    pub parameter_policies: Vec<crate::knowledge::catalog::parameter_policy::ParameterPolicy>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct QueryKnowledge {
    pub id: String,
    pub database: String,
    pub sql_file: String,

    #[serde(default)]
    pub data_areas: Vec<String>,

    #[serde(default)]
    pub tables: Vec<String>,

    #[serde(default)]
    pub metrics: Vec<String>,

    #[serde(default)]
    pub parameters: Vec<QueryParameter>,

    #[serde(default)]
    pub output_fields: Vec<QueryOutputField>,

    /// Per-query Postgres statement-timeout budget in milliseconds. Absent falls
    /// back to `QueryConfig.default_timeout_ms` at execution.
    #[serde(default)]
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct QueryParameter {
    pub name: String,

    #[serde(rename = "type")]
    pub kind: String,

    pub required: bool,

    #[serde(default)]
    pub source: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Sensitivity {
    PublicBusiness,
    Pii,
    FilterOnly,
    MaskedOutput,
    NeverUse,
}

impl Sensitivity {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PublicBusiness => "public_business",
            Self::Pii => "pii",
            Self::FilterOnly => "filter_only",
            Self::MaskedOutput => "masked_output",
            Self::NeverUse => "never_use",
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct QueryOutputField {
    pub name: String,

    #[serde(rename = "type")]
    pub kind: String,

    pub sensitivity: Sensitivity,
}
