use std::collections::BTreeMap;

use serde_json::json;
use uuid::Uuid;

use crate::{
    assistant::{
        CLARIFICATION_VERSION_1, ClarificationField, ClarificationKind, ClarificationOption,
        ClarificationPayload, ClarificationValidation, ConstraintField, ConstraintPatch, LimitMode,
        OTHER_CLARIFICATION_OPTION_ID, TypedFactValue,
    },
    knowledge::model::{
        CapabilityKnowledge, KnowledgeCatalog, ParameterInputKnowledge, QueryKnowledge,
    },
};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ClarificationFacts {
    pub values: BTreeMap<ConstraintField, TypedFactValue>,
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq)]
pub enum ClarificationPlanResult {
    Complete {
        capability_id: String,
        approved_defaults: ConstraintPatch,
    },
    Clarify {
        payload: ClarificationPayload,
        approved_defaults: ConstraintPatch,
    },
}

pub struct ClarificationPlanner<'a> {
    catalog: &'a KnowledgeCatalog,
}

impl<'a> ClarificationPlanner<'a> {
    pub fn new(catalog: &'a KnowledgeCatalog) -> Self {
        Self { catalog }
    }

    pub fn plan(
        &self,
        candidate_ids: &[String],
        facts: &ClarificationFacts,
        payload_id: Uuid,
    ) -> ClarificationPlanResult {
        let candidates: Vec<_> = candidate_ids
            .iter()
            .filter_map(|id| self.candidate(id, facts))
            .collect();
        let approved_defaults = common_defaults(&candidates);
        if candidates.len() == 1 && candidates[0].missing.is_empty() {
            return ClarificationPlanResult::Complete {
                capability_id: candidates[0].capability.id.clone(),
                approved_defaults,
            };
        }
        if candidates.len() == 1 {
            return ClarificationPlanResult::Clarify {
                payload: payload(
                    payload_id,
                    ClarificationKind::CollectFields,
                    "What details should I use for this report?",
                    Vec::new(),
                    candidates[0].missing.clone(),
                ),
                approved_defaults,
            };
        }
        let common = common_fields(&candidates);
        let options = candidates
            .iter()
            .map(|candidate| ClarificationOption {
                id: candidate.capability.id.clone(),
                label: candidate
                    .capability
                    .display_name
                    .clone()
                    .unwrap_or_else(|| {
                        crate::assistant::clarification::humanize_id(&candidate.capability.id)
                    }),
                description: option_description(candidate.capability),
                fields: candidate
                    .missing
                    .iter()
                    .filter(|field| !common.iter().any(|shared| same_input(shared, field)))
                    .cloned()
                    .collect(),
            })
            .chain(std::iter::once(ClarificationOption {
                id: OTHER_CLARIFICATION_OPTION_ID.to_string(),
                label: "Others".to_string(),
                description: Some("Let me describe it in my own words".to_string()),
                fields: Vec::new(),
            }))
            .collect();
        ClarificationPlanResult::Clarify {
            payload: payload(
                payload_id,
                ClarificationKind::SelectOption,
                "Which report would you like?",
                options,
                common,
            ),
            approved_defaults,
        }
    }

    fn candidate(&self, id: &str, facts: &ClarificationFacts) -> Option<Candidate<'a>> {
        let capability = self
            .catalog
            .capabilities
            .iter()
            .find(|capability| capability.id == id && capability.status == "approved_mvp")?;
        let query = self
            .catalog
            .queries
            .iter()
            .find(|query| query.id == capability.query_id)?;
        let inputs = required_inputs(query, &self.catalog.parameter_inputs);
        let defaults = limit_default(capability, query, &inputs);
        let missing = inputs
            .into_iter()
            .filter(|input| !input_satisfied(self.catalog, input, query, facts, &defaults))
            .map(|input| field_for(input, facts, capability))
            .collect();
        Some(Candidate {
            capability,
            missing,
            defaults,
        })
    }
}

struct Candidate<'a> {
    capability: &'a CapabilityKnowledge,
    missing: Vec<ClarificationField>,
    defaults: ConstraintPatch,
}

fn required_inputs<'a>(
    query: &QueryKnowledge,
    inputs: &'a [ParameterInputKnowledge],
) -> Vec<&'a ParameterInputKnowledge> {
    let mut result = Vec::new();
    for parameter in query.parameters.iter().filter(|p| {
        p.required
            && !matches!(
                p.source.as_deref(),
                Some("authorized_scope" | "transient_sensitive_input")
            )
    }) {
        if let Some(input) = inputs
            .iter()
            .find(|input| input.parameters.iter().any(|name| name == &parameter.name))
            && !result
                .iter()
                .any(|existing: &&ParameterInputKnowledge| existing.id == input.id)
        {
            result.push(input);
        }
    }
    result
}

fn limit_default(
    capability: &CapabilityKnowledge,
    query: &QueryKnowledge,
    inputs: &[&ParameterInputKnowledge],
) -> ConstraintPatch {
    let Some(limit) = capability.defaults.default_limit else {
        return ConstraintPatch::new();
    };
    let Some(input) = inputs.iter().find(|input| input.id == "limit") else {
        return ConstraintPatch::new();
    };
    if limit < input.validation.min_integer.unwrap_or(1)
        || capability.guards.max_limit.is_some_and(|max| limit > max)
        || input.validation.max_integer.is_some_and(|max| limit > max)
    {
        return ConstraintPatch::new();
    }
    let mode = if query
        .parameters
        .iter()
        .any(|parameter| parameter.name == "top_n")
    {
        LimitMode::TopN
    } else {
        LimitMode::Limit
    };
    [
        (ConstraintField::LimitMode, TypedFactValue::LimitMode(mode)),
        (ConstraintField::LimitValue, TypedFactValue::Integer(limit)),
    ]
    .into_iter()
    .collect()
}

/// An input is satisfied once every parameter this query actually takes from it
/// has a value.
///
/// This used to be a match on the input id ending in `_ => false`, which meant
/// six of the nine declared inputs — office_name, product_name, charge_name,
/// client_id, account_number, latest_transaction_amount — were reported missing
/// on every turn no matter what the user had typed, and the clarification could
/// never be answered away. The catalog's binding declaration answers it now, so
/// a new input needs no code here at all.
fn input_satisfied(
    catalog: &KnowledgeCatalog,
    input: &ParameterInputKnowledge,
    query: &QueryKnowledge,
    facts: &ClarificationFacts,
    defaults: &ConstraintPatch,
) -> bool {
    let mut relevant = input
        .parameters
        .iter()
        .filter(|name| query.parameters.iter().any(|p| p.name == **name))
        .peekable();
    if relevant.peek().is_none() {
        return false;
    }
    relevant.all(|name| {
        catalog
            .binding_fields(name)
            .iter()
            .any(|field| facts.values.contains_key(field) || defaults.contains_key(field))
    })
}

fn field_for(
    input: &ParameterInputKnowledge,
    facts: &ClarificationFacts,
    capability: &CapabilityKnowledge,
) -> ClarificationField {
    let value = (input.id == "date_range").then(|| json!({ "from": date_value(facts, ConstraintField::FromDate), "to": date_value(facts, ConstraintField::ToDate) }));
    ClarificationField {
        key: input.id.clone(),
        label: input.label.clone(),
        field_type: input.field_type.clone(),
        required: input.required,
        value,
        default_value: None,
        help_text: input.help_text.clone(),
        validation: field_validation(input, capability),
        errors: Vec::new(),
    }
}

fn field_validation(
    input: &ParameterInputKnowledge,
    capability: &CapabilityKnowledge,
) -> ClarificationValidation {
    let mut validation = input.validation.clone();
    if input.id == "date_range"
        && let Some(max) = capability.guards.max_date_range_days
    {
        validation.max_range_days = Some(
            validation
                .max_range_days
                .map_or(max, |current| current.min(max)),
        );
    }
    validation
}
fn date_value(facts: &ClarificationFacts, field: ConstraintField) -> Option<String> {
    match facts.values.get(&field) {
        Some(TypedFactValue::Date(value)) => Some(value.clone()),
        _ => None,
    }
}
fn common_defaults(candidates: &[Candidate<'_>]) -> ConstraintPatch {
    candidates
        .first()
        .map(|first| {
            first
                .defaults
                .iter()
                .filter(|(field, value)| {
                    candidates
                        .iter()
                        .all(|candidate| candidate.defaults.get(field) == Some(*value))
                })
                .map(|(field, value)| (field.clone(), value.clone()))
                .collect()
        })
        .unwrap_or_default()
}
fn common_fields(candidates: &[Candidate<'_>]) -> Vec<ClarificationField> {
    candidates
        .first()
        .map(|first| {
            first
                .missing
                .iter()
                .filter(|field| {
                    candidates.iter().all(|candidate| {
                        candidate
                            .missing
                            .iter()
                            .any(|other| same_input(field, other))
                    })
                })
                .cloned()
                .collect()
        })
        .unwrap_or_default()
}
fn same_input(left: &ClarificationField, right: &ClarificationField) -> bool {
    left.key == right.key
        && left.field_type == right.field_type
        && left.required == right.required
        && left.default_value == right.default_value
        && left.validation == right.validation
}
fn option_description(capability: &CapabilityKnowledge) -> Option<String> {
    match (&capability.description, capability.examples.first()) {
        (Some(description), Some(example)) => Some(format!("{description}\nExample: {example}")),
        (Some(description), None) => Some(description.clone()),
        (None, Some(example)) => Some(format!("Example: {example}")),
        (None, None) => None,
    }
}
fn payload(
    id: Uuid,
    kind: ClarificationKind,
    question: &str,
    options: Vec<ClarificationOption>,
    fields: Vec<ClarificationField>,
) -> ClarificationPayload {
    ClarificationPayload {
        version: CLARIFICATION_VERSION_1,
        id,
        revision: 0,
        kind,
        question: question.to_string(),
        options,
        fields,
        attempt: 0,
        source_intent: None,
        allow_free_text: false,
        is_missing_execution_parameters: true,
        workflow_id: None,
        node_id: None,
        resume_node_id: None,
        entity_kind: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        assistant::{ClarificationFieldType, RequestShape},
        knowledge::model::{
            CapabilityDefaults, CapabilityGuards, ClassificationPolicy, QueryParameter,
        },
    };
    use std::path::PathBuf;
    fn input(
        id: &str,
        parameters: Vec<&str>,
        field_type: ClarificationFieldType,
    ) -> ParameterInputKnowledge {
        ParameterInputKnowledge {
            id: id.into(),
            parameters: parameters.into_iter().map(str::to_string).collect(),
            field_type,
            label: id.into(),
            help_text: None,
            required: true,
            validation: if id == "limit" {
                ClarificationValidation {
                    min_integer: Some(1),
                    ..Default::default()
                }
            } else {
                ClarificationValidation::default()
            },
        }
    }
    fn param(name: &str) -> QueryParameter {
        QueryParameter {
            name: name.into(),
            kind: "string".into(),
            required: true,
            source: None,
        }
    }
    fn catalog(items: Vec<(&str, Option<i64>, Option<u32>)>) -> KnowledgeCatalog {
        let capabilities: Vec<_> = items
            .into_iter()
            .map(
                |(id, default_limit, max_date_range_days)| CapabilityKnowledge {
                    id: id.into(),
                    status: "approved_mvp".into(),
                    domain: "test".into(),
                    query_id: format!("{id}_query"),
                    dataset_recipe: None,
                    output_mode: "table".into(),
                    request_shape: RequestShape::default(),
                    kind: Default::default(),
                    member_capability_ids: vec![],
                    display_name: None,
                    description: None,
                    data_areas: vec![],
                    metrics: vec![],
                    examples: vec![],
                    continuation: false,
                    required_parameters: vec![],
                    optional_parameters: vec![],
                    defaults: CapabilityDefaults { default_limit },
                    guards: CapabilityGuards {
                        max_limit: Some(100),
                        max_date_range_days,
                    },
                    supported_intents: Vec::new(),
                    unsupported_intents: Vec::new(),
                    parameter_policies: vec![],
                },
            )
            .collect();
        let queries = capabilities
            .iter()
            .map(|capability| QueryKnowledge {
                id: capability.query_id.clone(),
                database: "db".into(),
                sql_file: "test.sql".into(),
                data_areas: vec![],
                tables: vec![],
                metrics: vec![],
                parameters: if capability.id.contains("top") {
                    vec![param("from_date"), param("to_date"), param("top_n")]
                } else {
                    vec![param("from_date"), param("to_date")]
                },
                output_fields: vec![],
                timeout_ms: None,
            })
            .collect();
        KnowledgeCatalog {
            root_path: PathBuf::new(),
            query_path: PathBuf::new(),
            data_areas: vec![],
            domains: vec![],
            schemas: vec![],
            metrics: vec![],
            capabilities,
            queries,
            policies: vec![],
            responses: vec![],
            parameter_bindings: [
                ("from_date", vec![ConstraintField::FromDate]),
                ("to_date", vec![ConstraintField::ToDate]),
                ("limit", vec![ConstraintField::LimitValue]),
                ("top_n", vec![ConstraintField::LimitValue]),
                ("search", vec![ConstraintField::PersonName]),
            ]
            .into_iter()
            .map(|(name, fields)| (name.to_string(), fields))
            .collect(),
            parameter_inputs: vec![
                input(
                    "date_range",
                    vec!["from_date", "to_date"],
                    ClarificationFieldType::DateRange,
                ),
                input(
                    "limit",
                    vec!["limit", "top_n"],
                    ClarificationFieldType::Integer,
                ),
            ],
            classification: ClassificationPolicy::default(),
            datasets: vec![],
        }
    }
    fn plan(
        catalog: &KnowledgeCatalog,
        ids: &[&str],
        facts: ClarificationFacts,
    ) -> ClarificationPlanResult {
        ClarificationPlanner::new(catalog).plan(
            &ids.iter().map(|id| id.to_string()).collect::<Vec<_>>(),
            &facts,
            Uuid::nil(),
        )
    }
    fn dates() -> ClarificationFacts {
        ClarificationFacts {
            values: [
                (
                    ConstraintField::FromDate,
                    TypedFactValue::Date("2024-01-01".into()),
                ),
                (
                    ConstraintField::ToDate,
                    TypedFactValue::Date("2024-01-31".into()),
                ),
            ]
            .into_iter()
            .collect(),
        }
    }
    #[test]
    fn one_capability_missing_dates_collects_one_date_range() {
        let c = catalog(vec![("total", None, None)]);
        let ClarificationPlanResult::Clarify { payload, .. } =
            plan(&c, &["total"], ClarificationFacts::default())
        else {
            panic!()
        };
        assert_eq!(payload.kind, ClarificationKind::CollectFields);
        assert!(payload.options.is_empty());
        assert_eq!(payload.fields[0].key, "date_range");
    }
    #[test]
    fn known_dates_are_not_asked_again() {
        let c = catalog(vec![("total", None, None)]);
        assert!(matches!(
            plan(&c, &["total"], dates()),
            ClarificationPlanResult::Complete { .. }
        ));
    }
    #[test]
    fn total_and_top_lift_dates_but_keep_top_limit_on_its_option() {
        let c = catalog(vec![("total", None, None), ("top", None, None)]);
        let ClarificationPlanResult::Clarify { payload, .. } =
            plan(&c, &["total", "top"], ClarificationFacts::default())
        else {
            panic!()
        };
        assert_eq!(payload.fields[0].key, "date_range");
        assert!(
            payload
                .options
                .iter()
                .find(|o| o.id == "total")
                .unwrap()
                .fields
                .is_empty()
        );
        assert_eq!(
            payload
                .options
                .iter()
                .find(|o| o.id == "top")
                .unwrap()
                .fields[0]
                .key,
            "limit"
        );
    }
    #[test]
    fn complete_single_candidate_returns_complete() {
        let c = catalog(vec![("total", None, None)]);
        assert!(
            matches!(plan(&c, &["total"], dates()), ClarificationPlanResult::Complete { capability_id, .. } if capability_id == "total")
        );
    }
    #[test]
    fn valid_capability_default_removes_limit_and_returns_patch() {
        let c = catalog(vec![("top", Some(10), None)]);
        let ClarificationPlanResult::Clarify {
            payload,
            approved_defaults,
        } = plan(&c, &["top"], ClarificationFacts::default())
        else {
            panic!()
        };
        assert_eq!(payload.fields.len(), 1);
        assert_eq!(
            approved_defaults[&ConstraintField::LimitMode],
            TypedFactValue::LimitMode(LimitMode::TopN)
        );
    }
    #[test]
    fn partial_date_range_is_prefilled() {
        let c = catalog(vec![("total", None, None)]);
        let facts = ClarificationFacts {
            values: [(
                ConstraintField::FromDate,
                TypedFactValue::Date("2024-01-01".into()),
            )]
            .into_iter()
            .collect(),
        };
        let ClarificationPlanResult::Clarify { payload, .. } = plan(&c, &["total"], facts) else {
            panic!()
        };
        assert_eq!(
            payload.fields[0].value,
            Some(json!({"from":"2024-01-01","to":null}))
        );
    }
    #[test]
    fn incompatible_date_bounds_do_not_lift_shared_field() {
        let c = catalog(vec![("total", None, Some(31)), ("other", None, Some(90))]);
        let ClarificationPlanResult::Clarify { payload, .. } =
            plan(&c, &["total", "other"], ClarificationFacts::default())
        else {
            panic!()
        };
        assert!(payload.fields.is_empty());
        assert_eq!(
            payload
                .options
                .iter()
                .filter(|o| o.id != OTHER_CLARIFICATION_OPTION_ID)
                .count(),
            2
        );
    }
}
