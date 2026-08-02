//! Layer 3 — Clarification Decider.
//!
//! Combines Layer-1 `LlmGatewayExtraction`, Layer-2 `ResolvedRequest`, and the
//! existing classifier `DecideOutcome` into a single decision. See spec §6.

use crate::assistant::understanding::classifier::DecideOutcome;
use crate::assistant::understanding::gateway::LlmGatewayExtraction;
use crate::assistant::understanding::intent::AssistantIntentKind;
use crate::assistant::understanding::resolver::{ResolvedParameter, ResolvedRequest};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq)]
pub enum DecisionOutcome {
    Execute {
        capability_id: String,
        parameters: BTreeMap<String, ResolvedParameter>,
    },
    Clarify {
        missing_fields: Vec<String>,
    },
    Reject {
        code: &'static str,
    },
}

pub fn decide(
    extraction: &LlmGatewayExtraction,
    resolved: &ResolvedRequest,
    classification: DecideOutcome,
) -> DecisionOutcome {
    // Sanitized rejection short-circuits everything else (spec §6 third bullet).
    match extraction.intent_kind {
        AssistantIntentKind::UnsafeRequest => {
            return DecisionOutcome::Reject {
                code: "unsafe_request",
            };
        }
        AssistantIntentKind::UnsupportedInDomain => {
            return DecisionOutcome::Reject {
                code: "unsupported_in_domain",
            };
        }
        _ => {}
    }
    if !resolved.unfilled_required.is_empty() {
        return DecisionOutcome::Clarify {
            missing_fields: resolved.unfilled_required.clone(),
        };
    }
    match classification {
        DecideOutcome::Match { .. } => DecisionOutcome::Execute {
            capability_id: resolved.capability_id.clone(),
            parameters: resolved.parameters.clone(),
        },
        DecideOutcome::Clarify => DecisionOutcome::Clarify {
            missing_fields: Vec::new(),
        },
        DecideOutcome::Unsupported => DecisionOutcome::Reject {
            code: "unsupported_by_catalog",
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assistant::understanding::gateway::{GatewayCandidate, LlmGatewayExtraction};
    use crate::assistant::understanding::intent::{
        AssistantDomain, AssistantIntentKind, AssistantLanguage,
    };

    fn extraction(intent_kind: AssistantIntentKind) -> LlmGatewayExtraction {
        LlmGatewayExtraction {
            intent_kind,
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

    fn resolved(unfilled: Vec<&str>) -> ResolvedRequest {
        ResolvedRequest {
            capability_id: "cap".into(),
            parameters: BTreeMap::new(),
            unfilled_required: unfilled.into_iter().map(str::to_string).collect(),
        }
    }

    #[test]
    fn all_filled_and_matched_executes() {
        let out = decide(
            &extraction(AssistantIntentKind::ReportRequest),
            &resolved(vec![]),
            DecideOutcome::Match {
                capability: "cap".into(),
            },
        );
        assert!(matches!(out, DecisionOutcome::Execute { .. }));
    }

    #[test]
    fn unfilled_required_forces_clarify_even_when_classifier_matches() {
        let out = decide(
            &extraction(AssistantIntentKind::ReportRequest),
            &resolved(vec!["search"]),
            DecideOutcome::Match {
                capability: "cap".into(),
            },
        );
        match out {
            DecisionOutcome::Clarify { missing_fields } => {
                assert_eq!(missing_fields, vec!["search".to_string()]);
            }
            other => panic!("expected Clarify, got {other:?}"),
        }
    }

    #[test]
    fn classifier_clarify_wins_when_all_filled() {
        let out = decide(
            &extraction(AssistantIntentKind::ReportRequest),
            &resolved(vec![]),
            DecideOutcome::Clarify,
        );
        assert!(matches!(out, DecisionOutcome::Clarify { .. }));
    }

    #[test]
    fn unsafe_intent_rejects_regardless_of_parameters() {
        let out = decide(
            &extraction(AssistantIntentKind::UnsafeRequest),
            &resolved(vec![]),
            DecideOutcome::Match {
                capability: "cap".into(),
            },
        );
        assert!(matches!(
            out,
            DecisionOutcome::Reject {
                code: "unsafe_request"
            }
        ));
    }

    #[test]
    fn unsupported_in_domain_rejects() {
        let out = decide(
            &extraction(AssistantIntentKind::UnsupportedInDomain),
            &resolved(vec![]),
            DecideOutcome::Match {
                capability: "cap".into(),
            },
        );
        assert!(matches!(
            out,
            DecisionOutcome::Reject {
                code: "unsupported_in_domain"
            }
        ));
    }

    #[test]
    fn classifier_unsupported_rejects() {
        let out = decide(
            &extraction(AssistantIntentKind::ReportRequest),
            &resolved(vec![]),
            DecideOutcome::Unsupported,
        );
        assert!(matches!(
            out,
            DecisionOutcome::Reject {
                code: "unsupported_by_catalog"
            }
        ));
    }
}
