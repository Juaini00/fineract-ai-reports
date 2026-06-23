use chrono::{Datelike, NaiveDate};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClassificationOutcome {
    Matched,
    ClarificationRequired,
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClassificationResult {
    pub outcome: ClassificationOutcome,
    pub domain: Option<String>,
    pub capability: Option<String>,
    pub confidence: f32,
    pub params: Value,
    pub clarification: Option<String>,
    pub options: Vec<ClarificationOption>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub candidates: Vec<ClassificationCandidate>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClarificationOption {
    pub label: String,
    pub capability: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClassificationCandidate {
    pub capability: String,
    pub confidence: f32,
}

pub fn classify_message(message: &str, today: NaiveDate) -> ClassificationResult {
    let normalized = message.to_lowercase();

    if !contains_any(
        &normalized,
        &[
            "deposit",
            "money in",
            "put into savings",
            "savings accounts",
        ],
    ) {
        return unsupported();
    }

    let Some((from_date, to_date)) = date_range(&normalized, today) else {
        return ClassificationResult {
            outcome: ClassificationOutcome::ClarificationRequired,
            domain: Some("savings".to_string()),
            capability: None,
            confidence: 0.45,
            params: json!({}),
            clarification: Some("Please clarify the deposit report date or period.".to_string()),
            options: Vec::new(),
            source: Some("local_rule".to_string()),
            candidates: Vec::new(),
        };
    };

    if contains_any(&normalized, &["top", "largest", "biggest"]) {
        return ClassificationResult {
            outcome: ClassificationOutcome::Matched,
            domain: Some("savings".to_string()),
            capability: Some("savings_deposit_top_n".to_string()),
            confidence: 0.86,
            params: json!({
                "from_date": from_date.to_string(),
                "to_date": to_date.to_string(),
                "office_scope": "authorized_scope",
                "limit": limit_from_message(&normalized).unwrap_or(10),
            }),
            clarification: None,
            options: Vec::new(),
            source: Some("local_rule".to_string()),
            candidates: Vec::new(),
        };
    }

    if contains_any(&normalized, &["total", "how much"]) {
        return ClassificationResult {
            outcome: ClassificationOutcome::Matched,
            domain: Some("savings".to_string()),
            capability: Some("savings_deposit_total".to_string()),
            confidence: 0.86,
            params: json!({
                "from_date": from_date.to_string(),
                "to_date": to_date.to_string(),
                "office_scope": "authorized_scope",
            }),
            clarification: None,
            options: Vec::new(),
            source: Some("local_rule".to_string()),
            candidates: Vec::new(),
        };
    }

    ClassificationResult {
        outcome: ClassificationOutcome::ClarificationRequired,
        domain: Some("savings".to_string()),
        capability: None,
        confidence: 0.5,
        params: json!({
            "from_date": from_date.to_string(),
            "to_date": to_date.to_string(),
        }),
        clarification: Some(
            "Please clarify whether you want the total deposit amount or the largest deposit transactions."
                .to_string(),
        ),
        options: deposit_options(),
        source: Some("local_rule".to_string()),
        candidates: Vec::new(),
    }
}

fn unsupported() -> ClassificationResult {
    ClassificationResult {
        outcome: ClassificationOutcome::Unsupported,
        domain: None,
        capability: None,
        confidence: 0.0,
        params: json!({}),
        clarification: None,
        options: Vec::new(),
        source: Some("local_rule".to_string()),
        candidates: Vec::new(),
    }
}

pub fn classify_clarification_response(
    original: &ClassificationResult,
    response: &str,
) -> ClassificationResult {
    let normalized = response.to_lowercase();
    if let Some(option) = selected_option(original, &normalized) {
        let mut params = original.params.clone();
        if option.capability == "savings_deposit_top_n" && params.get("limit").is_none() {
            params["limit"] = json!(limit_from_message(&normalized).unwrap_or(10));
        }

        return ClassificationResult {
            outcome: ClassificationOutcome::Matched,
            domain: original.domain.clone(),
            capability: Some(option.capability.clone()),
            confidence: 0.8,
            params,
            clarification: None,
            options: Vec::new(),
            source: Some("clarification_option".to_string()),
            candidates: original.candidates.clone(),
        };
    }

    let capability = if contains_any(&normalized, &["total", "amount", "sum"]) {
        Some("savings_deposit_total")
    } else if contains_any(&normalized, &["largest", "top", "biggest", "transactions"]) {
        Some("savings_deposit_top_n")
    } else {
        None
    };

    let Some(capability) = capability else {
        let mut result = original.clone();
        result.clarification = Some(
            "Please choose either total deposits or largest deposit transactions.".to_string(),
        );
        if result.options.is_empty() {
            result.options = deposit_options();
        }
        return result;
    };

    let mut params = original.params.clone();
    if capability == "savings_deposit_top_n" && params.get("limit").is_none() {
        params["limit"] = json!(limit_from_message(&normalized).unwrap_or(10));
    }

    ClassificationResult {
        outcome: ClassificationOutcome::Matched,
        domain: Some("savings".to_string()),
        capability: Some(capability.to_string()),
        confidence: 0.78,
        params,
        clarification: None,
        options: Vec::new(),
        source: Some("clarification_rule".to_string()),
        candidates: original.candidates.clone(),
    }
}

pub fn classify_retrieved_capability(
    message: &str,
    today: NaiveDate,
    domain: &str,
    capability: &str,
    output_mode: &str,
    confidence: f32,
    candidates: Vec<ClassificationCandidate>,
) -> ClassificationResult {
    let normalized = message.to_lowercase();
    let Some((from_date, to_date)) = date_range(&normalized, today) else {
        return ClassificationResult {
            outcome: ClassificationOutcome::ClarificationRequired,
            domain: Some(domain.to_string()),
            capability: None,
            confidence,
            params: json!({}),
            clarification: Some("Please clarify the report date or period.".to_string()),
            options: Vec::new(),
            source: Some("vector".to_string()),
            candidates,
        };
    };

    let mut params = json!({
        "from_date": from_date.to_string(),
        "to_date": to_date.to_string(),
        "office_scope": "authorized_scope",
    });

    if output_mode == "top_n" {
        params["limit"] = json!(limit_from_message(&normalized).unwrap_or(10));
    }

    ClassificationResult {
        outcome: ClassificationOutcome::Matched,
        domain: Some(domain.to_string()),
        capability: Some(capability.to_string()),
        confidence,
        params,
        clarification: None,
        options: Vec::new(),
        source: Some("vector".to_string()),
        candidates,
    }
}

pub fn clarify_retrieved_capabilities(
    message: &str,
    today: NaiveDate,
    domain: Option<String>,
    options: Vec<ClarificationOption>,
    confidence: f32,
    candidates: Vec<ClassificationCandidate>,
) -> ClassificationResult {
    let normalized = message.to_lowercase();
    let params = date_range(&normalized, today)
        .map(|(from_date, to_date)| {
            json!({
                "from_date": from_date.to_string(),
                "to_date": to_date.to_string(),
            })
        })
        .unwrap_or_else(|| json!({}));

    ClassificationResult {
        outcome: ClassificationOutcome::ClarificationRequired,
        domain,
        capability: None,
        confidence,
        params,
        clarification: Some("Please clarify which report you want.".to_string()),
        options,
        source: Some("vector".to_string()),
        candidates,
    }
}

fn selected_option<'a>(
    original: &'a ClassificationResult,
    normalized_response: &str,
) -> Option<&'a ClarificationOption> {
    let trimmed = normalized_response.trim();
    if let Ok(number) = trimmed.parse::<usize>() {
        return number
            .checked_sub(1)
            .and_then(|index| original.options.get(index));
    }

    original.options.iter().find(|option| {
        normalized_response.contains(&option.capability.to_lowercase())
            || normalized_response.contains(&option.label.to_lowercase())
    })
}

fn contains_any(value: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| value.contains(needle))
}

fn date_range(message: &str, today: NaiveDate) -> Option<(NaiveDate, NaiveDate)> {
    if contains_any(message, &["today"]) {
        return Some((today, today));
    }

    if contains_any(message, &["this month"]) {
        let first_day = NaiveDate::from_ymd_opt(today.year(), today.month(), 1)?;
        return Some((first_day, today));
    }

    if contains_any(message, &["this week"]) {
        let days_from_monday = today.weekday().num_days_from_monday() as i64;
        return Some((today - chrono::Duration::days(days_from_monday), today));
    }

    None
}

fn deposit_options() -> Vec<ClarificationOption> {
    vec![
        ClarificationOption {
            label: "Total deposits".to_string(),
            capability: "savings_deposit_total".to_string(),
        },
        ClarificationOption {
            label: "Largest deposit transactions".to_string(),
            capability: "savings_deposit_top_n".to_string(),
        },
    ]
}

fn limit_from_message(message: &str) -> Option<u32> {
    message
        .split(|character: char| !character.is_ascii_alphanumeric())
        .find_map(|token| token.parse::<u32>().ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 6, 21).unwrap()
    }

    #[test]
    fn classifies_total_deposit_today() {
        let result = classify_message("How much is the total deposit today?", today());

        assert_eq!(result.outcome, ClassificationOutcome::Matched);
        assert_eq!(result.capability.as_deref(), Some("savings_deposit_total"));
        assert_eq!(result.params["from_date"], "2026-06-21");
        assert_eq!(result.params["to_date"], "2026-06-21");
    }

    #[test]
    fn classifies_top_deposit_today() {
        let result = classify_message("Top 5 largest deposits today", today());

        assert_eq!(result.outcome, ClassificationOutcome::Matched);
        assert_eq!(result.capability.as_deref(), Some("savings_deposit_top_n"));
        assert_eq!(result.params["limit"], 5);
    }

    #[test]
    fn asks_clarification_when_date_missing() {
        let result = classify_message("How much is the total deposit?", today());

        assert_eq!(result.outcome, ClassificationOutcome::ClarificationRequired);
        assert!(result.clarification.is_some());
    }

    #[test]
    fn asks_clarification_for_ambiguous_money_in() {
        let result = classify_message(
            "What did customers put into savings accounts this week?",
            today(),
        );

        assert_eq!(result.outcome, ClassificationOutcome::ClarificationRequired);
        assert_eq!(result.options.len(), 2);
        assert_eq!(result.params["from_date"], "2026-06-15");
    }

    #[test]
    fn classifies_clarification_response_with_original_dates() {
        let original = classify_message(
            "What did customers put into savings accounts this week?",
            today(),
        );
        let result = classify_clarification_response(&original, "Total deposits");

        assert_eq!(result.outcome, ClassificationOutcome::Matched);
        assert_eq!(result.capability.as_deref(), Some("savings_deposit_total"));
        assert_eq!(result.params["from_date"], "2026-06-15");
    }

    #[test]
    fn classifies_retrieved_top_n_capability_with_params() {
        let result = classify_retrieved_capability(
            "Show customer savings activity this week top 7",
            today(),
            "savings",
            "savings_deposit_top_n",
            "top_n",
            0.72,
            Vec::new(),
        );

        assert_eq!(result.outcome, ClassificationOutcome::Matched);
        assert_eq!(result.capability.as_deref(), Some("savings_deposit_top_n"));
        assert_eq!(result.params["from_date"], "2026-06-15");
        assert_eq!(result.params["limit"], 7);
    }

    #[test]
    fn classifies_numeric_clarification_option() {
        let mut original = classify_message(
            "What did customers put into savings accounts this week?",
            today(),
        );
        original.options = vec![
            ClarificationOption {
                label: "Total deposits".to_string(),
                capability: "savings_deposit_total".to_string(),
            },
            ClarificationOption {
                label: "Largest deposits".to_string(),
                capability: "savings_deposit_top_n".to_string(),
            },
        ];

        let result = classify_clarification_response(&original, "2");

        assert_eq!(result.outcome, ClassificationOutcome::Matched);
        assert_eq!(result.capability.as_deref(), Some("savings_deposit_top_n"));
    }
}
