use crate::chat::pipeline::model::{ParsedIntent, ParsedIntentKind, RouteDecision};

pub fn route_intent(parsed: &ParsedIntent) -> RouteDecision {
    match parsed.intent {
        ParsedIntentKind::Report | ParsedIntentKind::ClarificationAnswer => {
            if parsed.confidence < 0.40 {
                RouteDecision::Clarify
            } else {
                RouteDecision::Report
            }
        }
        ParsedIntentKind::Unsupported | ParsedIntentKind::ToolAction => RouteDecision::Unsupported,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::pipeline::model::{
        ParsedConstraints, ParsedIntent, ParsedIntentKind, RouteDecision,
    };

    fn parsed(intent: ParsedIntentKind, confidence: f32) -> ParsedIntent {
        ParsedIntent {
            intent,
            domain: Some("savings".to_string()),
            entities: Vec::new(),
            constraints: ParsedConstraints::default(),
            requires_retrieval: true,
            confidence,
        }
    }

    #[test]
    fn report_routes_to_report_when_confident() {
        assert_eq!(
            route_intent(&parsed(ParsedIntentKind::Report, 0.8)),
            RouteDecision::Report
        );
    }

    #[test]
    fn low_confidence_report_routes_to_clarify() {
        assert_eq!(
            route_intent(&parsed(ParsedIntentKind::Report, 0.3)),
            RouteDecision::Clarify
        );
    }

    #[test]
    fn tool_action_routes_to_unsupported() {
        assert_eq!(
            route_intent(&parsed(ParsedIntentKind::ToolAction, 0.9)),
            RouteDecision::Unsupported
        );
    }
}
