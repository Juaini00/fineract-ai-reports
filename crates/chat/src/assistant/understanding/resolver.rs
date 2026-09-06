//! Layer 2 — Deterministic Resolver.
//!
//! Converts an `LlmGatewayExtraction` + selected `CapabilityKnowledge` into a
//! `ResolvedRequest` whose parameters are bound to concrete values with a
//! recorded `PayloadSource`, or listed as `unfilled_required`. See spec §5.4.

use std::collections::BTreeMap;

use chrono::{Datelike, Duration, Months, NaiveDate};

use crate::assistant::understanding::extraction::PayloadSource;
use crate::assistant::understanding::gateway::{
    LlmGatewayExtraction, QuantityHint, QuantityInferred, TemporalHint, TemporalInferred,
};
use crate::knowledge::catalog::parameter_policy::{EvaluationContext, ResolvedValue};
use crate::knowledge::model::CapabilityKnowledge;

const LLM_CONFIDENCE_FLOOR: f32 = 0.7;

pub struct ResolverRequest<'a> {
    pub extraction: &'a LlmGatewayExtraction,
    pub capability: &'a CapabilityKnowledge,
    pub business_today: NaiveDate,
    pub authorized_office_ids: Vec<i64>,
    pub user_message: &'a str,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedRequest {
    pub capability_id: String,
    pub parameters: BTreeMap<String, ResolvedParameter>,
    pub unfilled_required: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedParameter {
    pub value: ResolvedValue,
    pub source: PayloadSource,
}

pub fn resolve(request: &ResolverRequest<'_>) -> ResolvedRequest {
    let ctx = EvaluationContext {
        business_today: request.business_today,
        wall_today: request.business_today,
        authorized_office_ids: request.authorized_office_ids.clone(),
    };
    let mut parameters = BTreeMap::new();
    let mut unfilled_required = Vec::new();
    for policy in &request.capability.parameter_policies {
        if let Some(value) = fill_from_hint(
            policy.name.as_str(),
            request.extraction,
            request.business_today,
        ) {
            parameters.insert(
                policy.name.clone(),
                ResolvedParameter {
                    value,
                    source: PayloadSource::LlmClaim,
                },
            );
            continue;
        }
        if let Some(default) = &policy.default {
            parameters.insert(
                policy.name.clone(),
                ResolvedParameter {
                    value: default.evaluate(&ctx),
                    source: PayloadSource::CatalogDefault,
                },
            );
            continue;
        }
        if policy.required {
            unfilled_required.push(policy.name.clone());
        }
    }
    ResolvedRequest {
        capability_id: request.capability.id.clone(),
        parameters,
        unfilled_required,
    }
}

fn fill_from_hint(
    parameter: &str,
    extraction: &LlmGatewayExtraction,
    business_today: NaiveDate,
) -> Option<ResolvedValue> {
    match parameter {
        "from_date" => extraction
            .temporal_hint
            .as_ref()
            .and_then(|hint| resolve_temporal(hint, business_today))
            .map(|(from, _)| ResolvedValue::Date(from)),
        "to_date" => extraction
            .temporal_hint
            .as_ref()
            .and_then(|hint| resolve_temporal(hint, business_today))
            .map(|(_, to)| ResolvedValue::Date(to)),
        "limit" | "top_n" => extraction.quantity_hint.as_ref().and_then(resolve_quantity),
        _ => None,
    }
}

fn resolve_temporal(hint: &TemporalHint, today: NaiveDate) -> Option<(NaiveDate, NaiveDate)> {
    if hint.confidence < LLM_CONFIDENCE_FLOOR {
        return None;
    }
    match hint.inferred {
        TemporalInferred::Today | TemporalInferred::AsOfNow => Some((today, today)),
        TemporalInferred::Yesterday => {
            let d = today - Duration::days(1);
            Some((d, d))
        }
        TemporalInferred::ThisWeek => Some((week_start(today), today)),
        TemporalInferred::LastWeek => {
            let prev = week_start(today) - Duration::days(7);
            Some((prev, prev + Duration::days(6)))
        }
        TemporalInferred::ThisMonth => Some((start_of_month(today), today)),
        TemporalInferred::LastMonth => {
            let anchor = subtract_months(today, 1);
            Some((start_of_month(anchor), end_of_month(anchor)))
        }
        TemporalInferred::ThisYear => Some((
            NaiveDate::from_ymd_opt(today.year(), 1, 1).unwrap_or(today),
            today,
        )),
        TemporalInferred::LastYear => Some((
            NaiveDate::from_ymd_opt(today.year() - 1, 1, 1).unwrap_or(today),
            NaiveDate::from_ymd_opt(today.year() - 1, 12, 31).unwrap_or(today),
        )),
        TemporalInferred::Recent => Some((today - Duration::days(1), today)),
        TemporalInferred::Range => None, // range_hint phrases are user text; deferred to Phase 5 step 1
        TemporalInferred::None => None,
    }
}

fn resolve_quantity(hint: &QuantityHint) -> Option<ResolvedValue> {
    if hint.confidence < LLM_CONFIDENCE_FLOOR {
        return None;
    }
    match hint.inferred {
        QuantityInferred::All => Some(ResolvedValue::Unbounded),
        QuantityInferred::TopN | QuantityInferred::Limit => hint.value.map(ResolvedValue::Integer),
        QuantityInferred::Default => None,
    }
}

fn week_start(date: NaiveDate) -> NaiveDate {
    let offset = date.weekday().num_days_from_monday() as i64;
    date - Duration::days(offset)
}

fn start_of_month(date: NaiveDate) -> NaiveDate {
    NaiveDate::from_ymd_opt(date.year(), date.month(), 1).unwrap_or(date)
}

fn end_of_month(date: NaiveDate) -> NaiveDate {
    let next = start_of_month(date)
        .checked_add_months(Months::new(1))
        .unwrap_or(date);
    next - Duration::days(1)
}

fn subtract_months(date: NaiveDate, months: u32) -> NaiveDate {
    date.checked_sub_months(Months::new(months)).unwrap_or(date)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assistant::intent::RequestShape;
    use crate::assistant::understanding::gateway::{
        GatewayCandidate, LlmGatewayExtraction, TemporalHint, TemporalInferred,
    };
    use crate::assistant::understanding::intent::{
        AssistantDomain, AssistantIntentKind, AssistantLanguage,
    };
    use crate::knowledge::catalog::parameter_policy::{
        DefaultExpr, ParameterPolicy, ParameterType,
    };
    use crate::knowledge::model::{CapabilityDefaults, CapabilityGuards, CapabilityKnowledge};

    fn capability(policies: Vec<ParameterPolicy>) -> CapabilityKnowledge {
        CapabilityKnowledge {
            id: "cap".into(),
            status: "approved_mvp".into(),
            domain: "savings".into(),
            query_id: "cap.q".into(),
            dataset_recipe: None,
            output_mode: "top_n".into(),
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
            defaults: CapabilityDefaults::default(),
            guards: CapabilityGuards::default(),
            supported_intents: Vec::new(),
            unsupported_intents: Vec::new(),
            parameter_policies: policies,
        }
    }

    fn empty_extraction() -> LlmGatewayExtraction {
        LlmGatewayExtraction {
            intent_kind: AssistantIntentKind::ReportRequest,
            domain: AssistantDomain::Savings,
            language: AssistantLanguage::En,
            entities: vec![],
            temporal_hint: None,
            quantity_hint: None,
            dataset_hints: None,
            candidates: vec![GatewayCandidate {
                capability_id: "cap".into(),
                confidence: 0.9,
                why: "test".into(),
            }],
        }
    }

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 7, 15).unwrap() // Wednesday
    }

    #[test]
    fn fully_defaulted_capability_resolves_all_from_yaml() {
        let cap = capability(vec![
            ParameterPolicy {
                name: "as_of".into(),
                kind: ParameterType::Date,
                required: false,
                default: Some(DefaultExpr::BusinessToday),
                fill_when_missing: true,
                user_may_override: true,
                hard_cap: None,
                user_required: false,
                resolution: vec![],
                probe: None,
            },
            ParameterPolicy {
                name: "limit".into(),
                kind: ParameterType::Integer,
                required: false,
                default: Some(DefaultExpr::Unbounded),
                fill_when_missing: true,
                user_may_override: true,
                hard_cap: Some(10_000),
                user_required: false,
                resolution: vec![],
                probe: None,
            },
        ]);
        let ext = empty_extraction();
        let resolved = resolve(&ResolverRequest {
            extraction: &ext,
            capability: &cap,
            business_today: today(),
            authorized_office_ids: vec![1],
            user_message: "loan arrears clients",
        });
        assert!(resolved.unfilled_required.is_empty());
        assert_eq!(
            resolved.parameters["as_of"],
            ResolvedParameter {
                value: ResolvedValue::Date(today()),
                source: PayloadSource::CatalogDefault
            }
        );
        assert!(matches!(
            resolved.parameters["limit"].value,
            ResolvedValue::Unbounded
        ));
    }

    #[test]
    fn temporal_hint_this_month_binds_from_and_to() {
        let cap = capability(vec![
            ParameterPolicy {
                name: "from_date".into(),
                kind: ParameterType::Date,
                required: false,
                default: Some(DefaultExpr::BusinessToday),
                fill_when_missing: true,
                user_may_override: true,
                hard_cap: None,
                user_required: false,
                resolution: vec![],
                probe: None,
            },
            ParameterPolicy {
                name: "to_date".into(),
                kind: ParameterType::Date,
                required: false,
                default: Some(DefaultExpr::BusinessToday),
                fill_when_missing: true,
                user_may_override: true,
                hard_cap: None,
                user_required: false,
                resolution: vec![],
                probe: None,
            },
        ]);
        let mut ext = empty_extraction();
        ext.temporal_hint = Some(TemporalHint {
            phrase: "this month".into(),
            phrase_span: [0, 10],
            inferred: TemporalInferred::ThisMonth,
            range_hint: None,
            confidence: 0.95,
        });
        let resolved = resolve(&ResolverRequest {
            extraction: &ext,
            capability: &cap,
            business_today: today(),
            authorized_office_ids: vec![1],
            user_message: "this month",
        });
        assert_eq!(
            resolved.parameters["from_date"].value,
            ResolvedValue::Date(NaiveDate::from_ymd_opt(2026, 7, 1).unwrap())
        );
        assert_eq!(
            resolved.parameters["from_date"].source,
            PayloadSource::LlmClaim
        );
        assert_eq!(
            resolved.parameters["to_date"].value,
            ResolvedValue::Date(today())
        );
    }

    #[test]
    fn low_confidence_temporal_hint_falls_back_to_default() {
        let cap = capability(vec![ParameterPolicy {
            name: "from_date".into(),
            kind: ParameterType::Date,
            required: false,
            default: Some(DefaultExpr::BusinessToday),
            fill_when_missing: true,
            user_may_override: true,
            hard_cap: None,
            user_required: false,
            resolution: vec![],
            probe: None,
        }]);
        let mut ext = empty_extraction();
        ext.temporal_hint = Some(TemporalHint {
            phrase: "recently".into(),
            phrase_span: [0, 8],
            inferred: TemporalInferred::Recent,
            range_hint: None,
            confidence: 0.4, // below the 0.7 floor
        });
        let resolved = resolve(&ResolverRequest {
            extraction: &ext,
            capability: &cap,
            business_today: today(),
            authorized_office_ids: vec![1],
            user_message: "recently",
        });
        assert_eq!(
            resolved.parameters["from_date"].source,
            PayloadSource::CatalogDefault
        );
    }

    #[test]
    fn defaultless_required_param_lands_in_unfilled() {
        let cap = capability(vec![ParameterPolicy {
            name: "search".into(),
            kind: ParameterType::String,
            required: true,
            default: None,
            fill_when_missing: false,
            user_may_override: true,
            hard_cap: None,
            user_required: false,
            resolution: vec![],
            probe: None,
        }]);
        let resolved = resolve(&ResolverRequest {
            extraction: &empty_extraction(),
            capability: &cap,
            business_today: today(),
            authorized_office_ids: vec![],
            user_message: "look up a client",
        });
        assert_eq!(resolved.unfilled_required, vec!["search".to_string()]);
        assert!(resolved.parameters.is_empty());
    }

    #[test]
    fn temporal_hint_mapping_covers_all_inferred_variants() {
        let cap = capability(vec![
            ParameterPolicy {
                name: "from_date".into(),
                kind: ParameterType::Date,
                required: false,
                default: Some(DefaultExpr::BusinessToday),
                fill_when_missing: true,
                user_may_override: true,
                hard_cap: None,
                user_required: false,
                resolution: vec![],
                probe: None,
            },
            ParameterPolicy {
                name: "to_date".into(),
                kind: ParameterType::Date,
                required: false,
                default: Some(DefaultExpr::BusinessToday),
                fill_when_missing: true,
                user_may_override: true,
                hard_cap: None,
                user_required: false,
                resolution: vec![],
                probe: None,
            },
        ]);
        let cases = [
            (TemporalInferred::Today, today(), today()),
            (
                TemporalInferred::Yesterday,
                today() - Duration::days(1),
                today() - Duration::days(1),
            ),
            (
                TemporalInferred::ThisWeek,
                NaiveDate::from_ymd_opt(2026, 7, 13).unwrap(), // Monday
                today(),
            ),
            (
                TemporalInferred::LastWeek,
                NaiveDate::from_ymd_opt(2026, 7, 6).unwrap(),
                NaiveDate::from_ymd_opt(2026, 7, 12).unwrap(),
            ),
            (
                TemporalInferred::LastMonth,
                NaiveDate::from_ymd_opt(2026, 6, 1).unwrap(),
                NaiveDate::from_ymd_opt(2026, 6, 30).unwrap(),
            ),
            (
                TemporalInferred::ThisYear,
                NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
                today(),
            ),
            (
                TemporalInferred::LastYear,
                NaiveDate::from_ymd_opt(2025, 1, 1).unwrap(),
                NaiveDate::from_ymd_opt(2025, 12, 31).unwrap(),
            ),
            (
                TemporalInferred::Recent,
                today() - Duration::days(1),
                today(),
            ),
            (TemporalInferred::AsOfNow, today(), today()),
        ];
        for (inferred, expected_from, expected_to) in cases {
            let mut ext = empty_extraction();
            ext.temporal_hint = Some(TemporalHint {
                phrase: "x".into(),
                phrase_span: [0, 1],
                inferred,
                range_hint: None,
                confidence: 0.9,
            });
            let resolved = resolve(&ResolverRequest {
                extraction: &ext,
                capability: &cap,
                business_today: today(),
                authorized_office_ids: vec![],
                user_message: "x",
            });
            assert_eq!(
                resolved.parameters["from_date"].value,
                ResolvedValue::Date(expected_from),
                "from_date mismatch for {inferred:?}"
            );
            assert_eq!(
                resolved.parameters["to_date"].value,
                ResolvedValue::Date(expected_to),
                "to_date mismatch for {inferred:?}"
            );
        }
    }

    #[test]
    fn none_and_range_variants_fall_back_to_default() {
        let cap = capability(vec![ParameterPolicy {
            name: "from_date".into(),
            kind: ParameterType::Date,
            required: false,
            default: Some(DefaultExpr::BusinessToday),
            fill_when_missing: true,
            user_may_override: true,
            hard_cap: None,
            user_required: false,
            resolution: vec![],
            probe: None,
        }]);
        for inferred in [TemporalInferred::None, TemporalInferred::Range] {
            let mut ext = empty_extraction();
            ext.temporal_hint = Some(TemporalHint {
                phrase: "".into(),
                phrase_span: [0, 0],
                inferred,
                range_hint: None,
                confidence: 0.95,
            });
            let resolved = resolve(&ResolverRequest {
                extraction: &ext,
                capability: &cap,
                business_today: today(),
                authorized_office_ids: vec![],
                user_message: "",
            });
            assert_eq!(
                resolved.parameters["from_date"].source,
                PayloadSource::CatalogDefault
            );
        }
    }
}
