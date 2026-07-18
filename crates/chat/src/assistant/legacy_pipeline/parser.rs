use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::assistant::legacy_pipeline::model::{
    ParsedConstraints, ParsedEntity, ParsedIntent, ParsedIntentKind, QuantityConstraint,
};

#[derive(Debug, Deserialize)]
struct RawParsedIntent {
    intent: String,
    #[serde(default)]
    domain: Option<String>,
    #[serde(default)]
    entities: Vec<RawEntity>,
    #[serde(default)]
    constraints: RawConstraints,
    #[serde(default)]
    requires_retrieval: bool,
    confidence: f32,
}

#[derive(Debug, Deserialize)]
struct RawEntity {
    #[serde(rename = "type")]
    entity_type: String,
    value: String,
}

#[derive(Debug, Default, Deserialize)]
struct RawConstraints {
    from_date: Option<String>,
    to_date: Option<String>,
    quantity: Option<RawQuantity>,
    currency_code: Option<String>,
    product_ids: Option<Vec<i64>>,
}

#[derive(Debug, Deserialize)]
struct RawQuantity {
    mode: String,
    value: Option<i64>,
}

pub fn parse_semantic_response(content: &str) -> Result<ParsedIntent> {
    let raw: RawParsedIntent =
        serde_json::from_str(content).context("parse semantic parser JSON")?;
    if !(0.0..=1.0).contains(&raw.confidence) {
        bail!("semantic parser confidence must be between 0 and 1");
    }

    let intent = match raw.intent.as_str() {
        "report" => ParsedIntentKind::Report,
        "clarification_answer" => ParsedIntentKind::ClarificationAnswer,
        "unsupported" => ParsedIntentKind::Unsupported,
        "tool_action" => ParsedIntentKind::ToolAction,
        other => bail!("unsupported semantic parser intent {other}"),
    };

    let quantity = match raw.constraints.quantity {
        None => None,
        Some(raw) => Some(match raw.mode.as_str() {
            "all" => QuantityConstraint::All,
            "default" => QuantityConstraint::Default,
            "limit" => QuantityConstraint::Limit {
                value: positive_quantity(raw.value, "limit")?,
            },
            "top_n" => QuantityConstraint::TopN {
                value: positive_quantity(raw.value, "top_n")?,
            },
            other => bail!("unsupported quantity mode {other}"),
        }),
    };

    Ok(ParsedIntent {
        intent,
        domain: raw.domain,
        entities: raw
            .entities
            .into_iter()
            .map(|entity| ParsedEntity {
                entity_type: entity.entity_type,
                value: entity.value,
            })
            .collect(),
        constraints: ParsedConstraints {
            from_date: raw.constraints.from_date,
            to_date: raw.constraints.to_date,
            quantity,
            currency_code: raw.constraints.currency_code,
            product_ids: raw.constraints.product_ids,
        },
        requires_retrieval: raw.requires_retrieval,
        confidence: raw.confidence,
    })
}

fn positive_quantity(value: Option<i64>, mode: &str) -> Result<i64> {
    let Some(value) = value else {
        bail!("quantity {mode} requires value");
    };
    if value < 1 {
        bail!("quantity {mode} value must be positive");
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assistant::legacy_pipeline::model::{ParsedIntentKind, QuantityConstraint};

    #[test]
    fn parses_all_activity_intent() {
        let parsed = parse_semantic_response(
            r#"{
              "intent":"report",
              "domain":"savings",
              "entities":[{"type":"capability_hint","value":"savings activity"}],
              "constraints":{
                "from_date":"2026-07-01",
                "to_date":"2026-07-07",
                "quantity":{"mode":"all"},
                "currency_code":null,
                "product_ids":null
              },
              "requires_retrieval":true,
              "confidence":0.91
            }"#,
        )
        .unwrap();

        assert_eq!(parsed.intent, ParsedIntentKind::Report);
        assert_eq!(parsed.domain.as_deref(), Some("savings"));
        assert_eq!(parsed.constraints.quantity, Some(QuantityConstraint::All));
    }

    #[test]
    fn rejects_malformed_parser_json() {
        let error = parse_semantic_response("not-json").unwrap_err();
        assert!(error.to_string().contains("parse semantic parser JSON"));
    }
}
