use std::collections::BTreeMap;

use serde_json::json;
use uuid::Uuid;

use crate::{
    assistant::{
        CLARIFICATION_VERSION_1, ClarificationField, ClarificationKind, ClarificationOption,
        ClarificationPayload, ClarificationValidation, ConstraintField, ConstraintPatch, LimitMode,
        OTHER_CLARIFICATION_OPTION_ID, TypedFactValue,
    },
    knowledge::{
        catalog::parameter_policy::ParameterPolicy,
        model::{CapabilityKnowledge, KnowledgeCatalog, ParameterInputKnowledge, QueryKnowledge},
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
            .filter(|input| !input_satisfied(input, facts, &defaults))
            .map(|input| field_for(input, facts, capability))
            .collect();
        Some(Candidate {
            capability,
            missing,
            defaults,
        })
    }
}

/// Required user inputs for `capability_id` whose backing query parameters have
/// no policy default and are not yet satisfied by `facts`. The confident routing
/// path may ask only for these — parameters with a default are filled silently
/// (W-E). Today the set is `{ search }` for `client_name_lookup` and empty
/// elsewhere.
/// ponytail: general loop; a new defaultless required parameter is covered with
/// no further code change.
pub fn defaultless_missing_fields(
    catalog: &KnowledgeCatalog,
    capability_id: &str,
    facts: &ClarificationFacts,
) -> Vec<ClarificationField> {
    let Some(capability) = catalog
        .capabilities
        .iter()
        .find(|c| c.id == capability_id && c.status == "approved_mvp")
    else {
        return Vec::new();
    };
    let Some(query) = catalog.queries.iter().find(|q| q.id == capability.query_id) else {
        return Vec::new();
    };
    let inputs = required_inputs(query, &catalog.parameter_inputs);
    let defaults = limit_default(capability, query, &inputs);
    inputs
        .into_iter()
        .filter(|input| {
            input.parameters.iter().all(|name| {
                !(parameter_has_default(&capability.parameter_policies, name)
                    || (matches!(name.as_str(), "limit" | "top_n")
                        && capability.defaults.default_limit.is_some()))
            })
        })
        .filter(|input| !input_satisfied(input, facts, &defaults))
        .map(|input| field_for(input, facts, capability))
        .collect()
}

fn parameter_has_default(policies: &[ParameterPolicy], name: &str) -> bool {
    policies
        .iter()
        .any(|policy| policy.name == name && !policy.required && policy.default.is_some())
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
    for parameter in query
        .parameters
        .iter()
        .filter(|p| p.required && p.source.as_deref() != Some("authorized_scope"))
    {
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

fn input_satisfied(
    input: &ParameterInputKnowledge,
    facts: &ClarificationFacts,
    defaults: &ConstraintPatch,
) -> bool {
    match input.id.as_str() {
        "date_range" => {
            matches!(
                facts.values.get(&ConstraintField::FromDate),
                Some(TypedFactValue::Date(_))
            ) && matches!(
                facts.values.get(&ConstraintField::ToDate),
                Some(TypedFactValue::Date(_))
            )
        }
        "limit" => {
            matches!(
                facts
                    .values
                    .get(&ConstraintField::LimitMode)
                    .or_else(|| defaults.get(&ConstraintField::LimitMode)),
                Some(TypedFactValue::LimitMode(
                    LimitMode::TopN | LimitMode::Limit
                ))
            ) && matches!(
                facts
                    .values
                    .get(&ConstraintField::LimitValue)
                    .or_else(|| defaults.get(&ConstraintField::LimitValue)),
                Some(TypedFactValue::Integer(_))
            )
        }
        "search" => matches!(
            facts.values.get(&ConstraintField::PersonName),
            Some(TypedFactValue::PersonName(_))
        ),
        _ => false,
    }
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
    fn date_policy(name: &str) -> ParameterPolicy {
        use crate::knowledge::catalog::parameter_policy::{DefaultExpr, ParameterType};
        ParameterPolicy {
            name: name.into(),
            kind: ParameterType::Date,
            required: false,
            default: Some(DefaultExpr::BusinessToday),
            fill_when_missing: true,
            user_may_override: true,
            hard_cap: None,
        }
    }
    fn required_no_default(name: &str) -> ParameterPolicy {
        use crate::knowledge::catalog::parameter_policy::ParameterType;
        ParameterPolicy {
            name: name.into(),
            kind: ParameterType::String,
            required: true,
            default: None,
            fill_when_missing: false,
            user_may_override: true,
            hard_cap: None,
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
                    output_mode: "table".into(),
                    request_shape: RequestShape::default(),
                    display_name: None,
                    description: None,
                    data_areas: vec![],
                    metrics: vec![],
                    examples: vec![],
                    required_parameters: vec![],
                    optional_parameters: vec![],
                    defaults: CapabilityDefaults { default_limit },
                    guards: CapabilityGuards {
                        max_limit: Some(100),
                        max_date_range_days,
                    },
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
    fn defaulted_capability_has_no_defaultless_fields() {
        let mut c = catalog(vec![("total", None, None)]);
        c.capabilities[0].parameter_policies =
            vec![date_policy("from_date"), date_policy("to_date")];
        let fields = defaultless_missing_fields(&c, "total", &ClarificationFacts::default());
        assert!(
            fields.is_empty(),
            "defaulted params must not be asked: {fields:?}"
        );
    }
    #[test]
    fn defaultless_required_param_is_asked_when_fact_absent() {
        let mut c = catalog(vec![("lookup", None, None)]);
        c.queries[0].parameters = vec![param("search")];
        c.parameter_inputs.push(input(
            "search",
            vec!["search"],
            ClarificationFieldType::Text,
        ));
        c.capabilities[0].parameter_policies = vec![required_no_default("search")];
        let fields = defaultless_missing_fields(&c, "lookup", &ClarificationFacts::default());
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0].key, "search");
    }
    #[test]
    fn present_fact_satisfies_defaultless_required_param() {
        let mut c = catalog(vec![("lookup", None, None)]);
        c.queries[0].parameters = vec![param("search")];
        c.parameter_inputs.push(input(
            "search",
            vec!["search"],
            ClarificationFieldType::Text,
        ));
        c.capabilities[0].parameter_policies = vec![required_no_default("search")];
        let facts = ClarificationFacts {
            values: [(
                ConstraintField::PersonName,
                TypedFactValue::PersonName("Tony".into()),
            )]
            .into_iter()
            .collect(),
        };
        assert!(defaultless_missing_fields(&c, "lookup", &facts).is_empty());
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
