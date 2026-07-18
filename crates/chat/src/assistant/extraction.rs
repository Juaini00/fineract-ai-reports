use chrono::{DateTime, Datelike, Duration, FixedOffset, NaiveDate, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::assistant::{
    AssistantConstraints, AssistantDomain, AssistantEntity, AssistantEntityType, AssistantIntent,
    Quantity,
};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DeterministicExtraction {
    pub constraints: AssistantConstraints,
    pub domain: Option<AssistantDomain>,
    pub entities: Vec<AssistantEntity>,
    #[serde(default)]
    pub candidates: Vec<PayloadCandidate>,
    #[serde(default)]
    pub temporal_provenance: Option<TemporalProvenance>,
    #[serde(default)]
    pub temporal_error: Option<TemporalValidationError>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TemporalProvenance {
    pub rule: String,
    pub phrase_span: [usize; 2],
    #[schemars(with = "String")]
    pub reference_instant: DateTime<Utc>,
    pub timezone: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TemporalValidationError {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PayloadCandidate {
    pub field: PayloadField,
    pub value: serde_json::Value,
    pub source: PayloadSource,
    pub trust: PayloadTrust,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DeterministicExtractionConflict {
    pub field: PayloadField,
    pub llm_value: serde_json::Value,
    pub trusted_value: serde_json::Value,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PayloadField {
    Limit,
    FromDate,
    ToDate,
    CurrencyCode,
    Metric,
    Domain,
    PersonName,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PayloadSource {
    UserText,
    LlmClaim,
    CatalogDefault,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PayloadTrust {
    Trusted,
    Untrusted,
    Rejected,
}

impl DeterministicExtraction {
    pub fn is_empty(&self) -> bool {
        self.constraints == AssistantConstraints::default()
            && self.domain.is_none()
            && self.entities.is_empty()
            && self.candidates.is_empty()
            && self.temporal_error.is_none()
    }

    pub fn merge_into(&self, intent: &mut AssistantIntent) {
        self.merge_into_legacy_non_authoritative(intent);
    }

    pub fn merge_into_legacy_non_authoritative(&self, intent: &mut AssistantIntent) {
        if let Some(quantity) = &self.constraints.quantity {
            match (&intent.constraints.quantity, quantity) {
                (None | Some(Quantity::Default), _) => {
                    intent.constraints.quantity = Some(quantity.clone());
                }
                (
                    Some(Quantity::Limit { value: old } | Quantity::TopN { value: old }),
                    Quantity::Limit { value: new } | Quantity::TopN { value: new },
                ) if old != new => {
                    intent.constraints.quantity = Some(quantity.clone());
                }
                _ => {}
            }
        }
        if self.constraints.from_date.is_some() {
            intent.constraints.from_date = self.constraints.from_date.clone();
        }
        if self.constraints.to_date.is_some() {
            intent.constraints.to_date = self.constraints.to_date.clone();
        }
        if self.constraints.currency_code.is_some() {
            intent.constraints.currency_code = self.constraints.currency_code.clone();
        }
        if let Some(metric) = &self.constraints.metric
            && intent.constraints.metric.is_none()
        {
            intent.constraints.metric = Some(metric.clone());
        }
        if matches!(intent.domain, AssistantDomain::Unknown)
            && let Some(domain) = &self.domain
        {
            intent.domain = domain.clone();
        }
        for entity in &self.entities {
            let canonical = entity.canonical.as_deref().unwrap_or(&entity.value);
            let exists = intent.entities.iter().any(|existing| {
                existing.entity_type == entity.entity_type
                    && existing.canonical.as_deref().unwrap_or(&existing.value) == canonical
            });
            if !exists {
                intent.entities.push(entity.clone());
            }
        }
    }

    pub fn conflicts_with(&self, intent: &AssistantIntent) -> Vec<DeterministicExtractionConflict> {
        let mut conflicts = Vec::new();
        if let (Some(llm), Some(trusted)) =
            (&intent.constraints.quantity, &self.constraints.quantity)
            && let (Some((llm_mode, llm_value)), Some((trusted_mode, trusted_value))) =
                (quantity_parts(llm), quantity_parts(trusted))
            && (llm_mode != trusted_mode || llm_value != trusted_value)
        {
            conflicts.push(conflict(
                PayloadField::Limit,
                serde_json::json!({ "mode": llm_mode, "value": llm_value }),
                serde_json::json!({ "mode": trusted_mode, "value": trusted_value }),
            ));
        }
        push_string_conflict(
            &mut conflicts,
            PayloadField::FromDate,
            intent.constraints.from_date.as_deref(),
            self.constraints.from_date.as_deref(),
        );
        push_string_conflict(
            &mut conflicts,
            PayloadField::ToDate,
            intent.constraints.to_date.as_deref(),
            self.constraints.to_date.as_deref(),
        );
        push_string_conflict(
            &mut conflicts,
            PayloadField::CurrencyCode,
            intent.constraints.currency_code.as_deref(),
            self.constraints.currency_code.as_deref(),
        );
        push_string_conflict(
            &mut conflicts,
            PayloadField::Metric,
            intent.constraints.metric.as_deref(),
            self.constraints.metric.as_deref(),
        );
        if let Some(trusted) = &self.domain
            && !matches!(intent.domain, AssistantDomain::Unknown)
            && intent.domain != *trusted
        {
            conflicts.push(conflict(
                PayloadField::Domain,
                serde_json::json!(intent.domain),
                serde_json::json!(trusted),
            ));
        }
        conflicts
    }
}

fn quantity_parts(quantity: &Quantity) -> Option<(&'static str, i64)> {
    match quantity {
        Quantity::Limit { value } => Some(("limit", *value)),
        Quantity::TopN { value } => Some(("top_n", *value)),
        Quantity::All | Quantity::Default => None,
    }
}

fn push_string_conflict(
    conflicts: &mut Vec<DeterministicExtractionConflict>,
    field: PayloadField,
    llm: Option<&str>,
    trusted: Option<&str>,
) {
    if let (Some(llm), Some(trusted)) = (llm, trusted)
        && llm != trusted
    {
        conflicts.push(conflict(
            field,
            serde_json::json!(llm),
            serde_json::json!(trusted),
        ));
    }
}

fn conflict(
    field: PayloadField,
    llm_value: serde_json::Value,
    trusted_value: serde_json::Value,
) -> DeterministicExtractionConflict {
    DeterministicExtractionConflict {
        field,
        llm_value,
        trusted_value,
        reason: "deterministic_extraction_preferred".into(),
    }
}

pub fn extract_message_facts(message: &str) -> DeterministicExtraction {
    extract_message_facts_at(message, Utc::now(), 366)
}

pub fn extract_message_facts_at(
    message: &str,
    reference_instant: DateTime<Utc>,
    max_range_days: i64,
) -> DeterministicExtraction {
    let lower = message.to_lowercase();
    let words = words(&lower);
    let mut extraction = DeterministicExtraction::default();

    extraction.constraints.quantity = extract_quantity(&lower, &words);
    match resolve_temporal(message, reference_instant, max_range_days) {
        Ok(Some(resolved)) => {
            extraction.constraints.from_date = Some(resolved.from.to_string());
            extraction.constraints.to_date = Some(resolved.to.to_string());
            extraction.temporal_provenance = Some(resolved.provenance);
        }
        Ok(None) => {}
        Err(error) => extraction.temporal_error = Some(error),
    }
    extraction.constraints.currency_code = extract_currency(message);
    extraction.domain = extract_domain(&lower);
    if let Some(metric) = extract_metric(&lower) {
        extraction.constraints.metric = Some(metric.into());
        extraction.entities.push(AssistantEntity {
            entity_type: AssistantEntityType::Metric,
            value: metric.into(),
            canonical: Some(metric.into()),
            confidence: Some(1.0),
        });
    }
    if let Some(name) = extract_person_name(message) {
        extraction.entities.push(AssistantEntity {
            entity_type: AssistantEntityType::PersonName,
            value: name.clone(),
            canonical: Some(name),
            confidence: Some(1.0),
        });
    }

    record_candidates(&mut extraction);

    extraction
}

struct ResolvedTemporal {
    from: NaiveDate,
    to: NaiveDate,
    provenance: TemporalProvenance,
}

fn resolve_temporal(
    message: &str,
    reference_instant: DateTime<Utc>,
    max_range_days: i64,
) -> Result<Option<ResolvedTemporal>, TemporalValidationError> {
    let lower = message.to_ascii_lowercase();
    let tokens = tokens_with_spans(&lower);
    let jakarta = FixedOffset::east_opt(7 * 3600).expect("valid Jakarta offset");
    let today = reference_instant.with_timezone(&jakarta).date_naive();
    let invalid = |code: &str, message: &str| TemporalValidationError {
        code: code.into(),
        message: message.into(),
    };
    let finish = |from: NaiveDate, to: NaiveDate, rule: &str, span: [usize; 2]| {
        if from > to {
            return Err(invalid(
                "temporal_range_reversed",
                "The start date must not be after the end date.",
            ));
        }
        let days = (to - from).num_days() + 1;
        if max_range_days <= 0 || days > max_range_days {
            return Err(invalid(
                "temporal_range_too_large",
                "The date range exceeds the capability limit.",
            ));
        }
        Ok(Some(ResolvedTemporal {
            from,
            to,
            provenance: TemporalProvenance {
                rule: rule.into(),
                phrase_span: span,
                reference_instant,
                timezone: "Asia/Jakarta".into(),
            },
        }))
    };

    for window in tokens.windows(4) {
        if matches!(
            (window[0].0, window[2].0),
            ("from", "to") | ("dari", "sampai")
        ) {
            let from = parse_date(window[1].0)?;
            let to = parse_date(window[3].0)?;
            return finish(
                from,
                to,
                "inclusive_explicit_range",
                [window[0].1, window[3].2],
            );
        }
    }
    // Bare "DATE to DATE" / "DATE sampai DATE" (parameter-only replies where
    // the user drops the "from" keyword).
    for window in tokens.windows(3) {
        if matches!(window[1].0, "to" | "sampai")
            && looks_like_iso_date(window[0].0)
            && looks_like_iso_date(window[2].0)
        {
            let from = parse_date(window[0].0)?;
            let to = parse_date(window[2].0)?;
            return finish(
                from,
                to,
                "inclusive_explicit_range",
                [window[0].1, window[2].2],
            );
        }
    }

    let relative: &[(&[&str], &str)] = &[
        (&["today"], "today"),
        (&["hari", "ini"], "today"),
        (&["yesterday"], "yesterday"),
        (&["kemarin"], "yesterday"),
        (&["this", "week"], "this_week"),
        (&["minggu", "ini"], "this_week"),
        (&["last", "week"], "last_week"),
        (&["minggu", "lalu"], "last_week"),
        (&["this", "month"], "this_month"),
        (&["bulan", "ini"], "this_month"),
        (&["last", "month"], "last_month"),
        (&["bulan", "lalu"], "last_month"),
        (&["this", "quarter"], "this_quarter"),
        (&["kuartal", "ini"], "this_quarter"),
        (&["last", "quarter"], "last_quarter"),
        (&["kuartal", "lalu"], "last_quarter"),
        (&["this", "year"], "this_year"),
        (&["tahun", "ini"], "this_year"),
        (&["last", "year"], "last_year"),
        (&["tahun", "lalu"], "last_year"),
    ];
    for (phrase, rule) in relative {
        if let Some(window) = tokens.windows(phrase.len()).find(|window| {
            window
                .iter()
                .map(|token| token.0)
                .eq(phrase.iter().copied())
        }) {
            let (from, to) = relative_range(today, rule);
            return finish(from, to, rule, [window[0].1, window[window.len() - 1].2]);
        }
    }
    for window in tokens.windows(3) {
        let english = window[0].0 == "last" && window[2].0 == "days";
        let indonesian = window[0].0.parse::<i64>().is_ok()
            && window[1].0 == "hari"
            && window[2].0 == "terakhir";
        let value = if english {
            window[1].0
        } else if indonesian {
            window[0].0
        } else {
            continue;
        };
        let days = value.parse::<i64>().map_err(|_| {
            invalid(
                "temporal_invalid_count",
                "The day count must be a positive integer.",
            )
        })?;
        if days <= 0 || days > max_range_days {
            return Err(invalid(
                "temporal_invalid_count",
                "The day count must be positive and within the capability limit.",
            ));
        }
        return finish(
            today - Duration::days(days - 1),
            today,
            "last_n_days_inclusive",
            [window[0].1, window[2].2],
        );
    }

    let date_tokens = tokens
        .iter()
        .filter(|token| looks_like_iso_date(token.0))
        .collect::<Vec<_>>();
    if date_tokens.len() == 1 {
        // If the message has an explicit range keyword ("from" / "dari" /
        // "since") before the sole ISO date, the user meant a range that is
        // missing its upper bound. Treat as ambiguous so the executor
        // surfaces "missing parameter to_date" instead of silently
        // collapsing to a single-day window.
        let has_range_lead_in = tokens
            .iter()
            .take_while(|token| token.0 != date_tokens[0].0)
            .any(|token| matches!(token.0, "from" | "dari" | "since"));
        if has_range_lead_in {
            return Err(invalid(
                "temporal_missing_to_date",
                "missing parameter to_date: provide an inclusive from..to range.",
            ));
        }
        let date = parse_date(date_tokens[0].0)?;
        return finish(
            date,
            date,
            "iso_single_date",
            [date_tokens[0].1, date_tokens[0].2],
        );
    }
    if date_tokens.len() > 1 {
        return Err(invalid(
            "temporal_ambiguous",
            "Use an inclusive 'from DATE to DATE' or 'dari DATE sampai DATE' range.",
        ));
    }
    if tokens.iter().any(|token| {
        token.0.matches('-').count() == 2
            && token.0.chars().all(|ch| ch.is_ascii_digit() || ch == '-')
    }) {
        return Err(invalid(
            "temporal_invalid_date",
            "Use a valid Gregorian date in YYYY-MM-DD format.",
        ));
    }
    if tokens.iter().any(|token| {
        matches!(
            token.0,
            "today"
                | "yesterday"
                | "week"
                | "month"
                | "quarter"
                | "year"
                | "kemarin"
                | "tahun"
                | "hari"
                | "minggu"
                | "bulan"
                | "kuartal"
        )
    }) {
        return Err(invalid(
            "temporal_ambiguous",
            "The date expression is ambiguous or unsupported.",
        ));
    }
    Ok(None)
}

fn relative_range(today: NaiveDate, rule: &str) -> (NaiveDate, NaiveDate) {
    let month_start = |date: NaiveDate| date.with_day(1).expect("day one exists");
    let next_month = |date: NaiveDate| {
        if date.month() == 12 {
            NaiveDate::from_ymd_opt(date.year() + 1, 1, 1).unwrap()
        } else {
            NaiveDate::from_ymd_opt(date.year(), date.month() + 1, 1).unwrap()
        }
    };
    let year = |value: i32| NaiveDate::from_ymd_opt(value, 1, 1).expect("valid year");
    match rule {
        "today" => (today, today),
        "yesterday" => (today - Duration::days(1), today - Duration::days(1)),
        "this_week" | "last_week" => {
            let start = today
                - Duration::days(today.weekday().num_days_from_monday() as i64)
                - if rule == "last_week" {
                    Duration::days(7)
                } else {
                    Duration::zero()
                };
            (start, start + Duration::days(6))
        }
        "this_month" => (month_start(today), next_month(today) - Duration::days(1)),
        "last_month" => {
            let end = month_start(today) - Duration::days(1);
            (month_start(end), end)
        }
        "this_quarter" | "last_quarter" => {
            let mut quarter = (today.month0() / 3) as i32;
            let mut year_value = today.year();
            if rule == "last_quarter" {
                quarter -= 1;
                if quarter < 0 {
                    quarter = 3;
                    year_value -= 1;
                }
            }
            let start = NaiveDate::from_ymd_opt(year_value, quarter as u32 * 3 + 1, 1).unwrap();
            let next = if quarter == 3 {
                year(year_value + 1)
            } else {
                NaiveDate::from_ymd_opt(year_value, (quarter as u32 + 1) * 3 + 1, 1).unwrap()
            };
            (start, next - Duration::days(1))
        }
        "this_year" => (
            year(today.year()),
            year(today.year() + 1) - Duration::days(1),
        ),
        "last_year" => (
            year(today.year() - 1),
            year(today.year()) - Duration::days(1),
        ),
        _ => unreachable!(),
    }
}

fn tokens_with_spans(message: &str) -> Vec<(&str, usize, usize)> {
    let mut result = Vec::new();
    let mut start = None;
    for (index, ch) in message
        .char_indices()
        .chain(std::iter::once((message.len(), ' ')))
    {
        if ch.is_ascii_alphanumeric() || ch == '-' {
            start.get_or_insert(index);
        } else if let Some(begin) = start.take() {
            result.push((&message[begin..index], begin, index));
        }
    }
    result
}

fn parse_date(value: &str) -> Result<NaiveDate, TemporalValidationError> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d").map_err(|_| TemporalValidationError {
        code: "temporal_invalid_date".into(),
        message: "Use a valid Gregorian date in YYYY-MM-DD format.".into(),
    })
}

fn record_candidates(extraction: &mut DeterministicExtraction) {
    if let Some(quantity) = &extraction.constraints.quantity {
        let value = match quantity {
            Quantity::Limit { value } | Quantity::TopN { value } => *value,
            Quantity::Default => return,
            Quantity::All => return,
        };
        extraction
            .candidates
            .push(candidate(PayloadField::Limit, serde_json::json!(value)));
    }
    if let Some(value) = &extraction.constraints.from_date {
        extraction
            .candidates
            .push(candidate(PayloadField::FromDate, serde_json::json!(value)));
    }
    if let Some(value) = &extraction.constraints.to_date {
        extraction
            .candidates
            .push(candidate(PayloadField::ToDate, serde_json::json!(value)));
    }
    if let Some(value) = &extraction.constraints.currency_code {
        extraction.candidates.push(candidate(
            PayloadField::CurrencyCode,
            serde_json::json!(value),
        ));
    }
    if let Some(value) = &extraction.constraints.metric {
        extraction
            .candidates
            .push(candidate(PayloadField::Metric, serde_json::json!(value)));
    }
    if let Some(value) = &extraction.domain {
        extraction
            .candidates
            .push(candidate(PayloadField::Domain, serde_json::json!(value)));
    }
    for entity in &extraction.entities {
        if entity.entity_type == AssistantEntityType::PersonName {
            extraction.candidates.push(candidate(
                PayloadField::PersonName,
                serde_json::json!(entity.canonical.as_deref().unwrap_or(&entity.value)),
            ));
        }
    }
}

fn candidate(field: PayloadField, value: serde_json::Value) -> PayloadCandidate {
    PayloadCandidate {
        field,
        value,
        source: PayloadSource::UserText,
        trust: PayloadTrust::Trusted,
    }
}

fn words(message: &str) -> Vec<&str> {
    message
        .split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '-')
        .filter(|part| !part.is_empty())
        .collect()
}

fn extract_quantity(message: &str, words: &[&str]) -> Option<Quantity> {
    for (idx, word) in words.iter().enumerate() {
        let Ok(value) = word.parse::<i64>() else {
            continue;
        };
        if !(1..=100).contains(&value) {
            continue;
        }
        let near = words[idx.saturating_sub(2)..usize::min(words.len(), idx + 3)].join(" ");
        if near.contains("days") || near.contains("hari") {
            continue;
        }
        return Some(
            if near.contains("top")
                || message.contains(" most ")
                || message.contains("highest")
                || message.contains("rank")
            {
                Quantity::TopN { value }
            } else {
                Quantity::Limit { value }
            },
        );
    }
    None
}

fn looks_like_iso_date(word: &str) -> bool {
    let bytes = word.as_bytes();
    bytes.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes
            .iter()
            .enumerate()
            .all(|(idx, byte)| idx == 4 || idx == 7 || byte.is_ascii_digit())
}

fn extract_currency(message: &str) -> Option<String> {
    message
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .find(|word| matches!(*word, "IDR" | "USD" | "EUR" | "AED" | "SAR"))
        .map(str::to_string)
}

fn extract_domain(message: &str) -> Option<AssistantDomain> {
    if message.contains("client") {
        Some(AssistantDomain::Client)
    } else if message.contains("office") || message.contains("organization") {
        Some(AssistantDomain::Organization)
    } else if message.contains("saving") {
        Some(AssistantDomain::Savings)
    } else {
        None
    }
}

fn extract_metric(message: &str) -> Option<&'static str> {
    if message.contains("most savings account") || message.contains("number of savings account") {
        Some("savings_account_count")
    } else if message.contains("highest balance") || message.contains("savings balance") {
        Some("savings_balance")
    } else if message.contains("deposit volume") || message.contains("deposited the most") {
        Some("deposit_volume")
    } else {
        None
    }
}

fn extract_person_name(message: &str) -> Option<String> {
    let parts = message
        .split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '-' && ch != '_')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    for pair in parts.windows(2) {
        if matches!(pair[0].to_ascii_lowercase().as_str(), "named" | "name")
            && pair[1].chars().any(char::is_alphabetic)
        {
            return Some(pair[1].to_string());
        }
    }
    for pair in parts.windows(2) {
        let next = pair[1].to_ascii_lowercase();
        if matches!(pair[0].to_ascii_lowercase().as_str(), "client" | "find")
            && !matches!(next.as_str(), "client" | "name" | "named")
            && pair[1].chars().any(char::is_alphabetic)
        {
            return Some(pair[1].to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extraction_quantity_currency_date_metric() {
        let extraction = extract_message_facts(
            "show top 10 clients with the most savings accounts in USD from 2026-01-01 to 2026-01-31",
        );

        assert_eq!(
            extraction.constraints.quantity,
            Some(Quantity::TopN { value: 10 })
        );
        assert_eq!(extraction.constraints.currency_code.as_deref(), Some("USD"));
        assert_eq!(
            extraction.constraints.from_date.as_deref(),
            Some("2026-01-01")
        );
        assert_eq!(
            extraction.constraints.to_date.as_deref(),
            Some("2026-01-31")
        );
        assert_eq!(
            extraction.constraints.metric.as_deref(),
            Some("savings_account_count")
        );
        assert!(extraction.candidates.iter().any(|candidate| {
            candidate.field == PayloadField::Limit && candidate.trust == PayloadTrust::Trusted
        }));
    }

    #[test]
    fn extraction_merges_metric_when_absent() {
        let extraction = extract_message_facts("show 10 clients with the most savings accounts");
        let mut intent = AssistantIntent {
            intent: Default::default(),
            domain: AssistantDomain::Unknown,
            request_shape: Default::default(),
            language: crate::assistant::AssistantLanguage::En,
            entities: Vec::new(),
            constraints: Default::default(),
            context_reference: Default::default(),
            source: None,
            confidence: 0.0,
            reason: String::new(),
        };

        extraction.merge_into(&mut intent);

        assert_eq!(
            intent.constraints.quantity,
            Some(Quantity::TopN { value: 10 })
        );
        assert_eq!(
            intent.constraints.metric.as_deref(),
            Some("savings_account_count")
        );
        assert_eq!(intent.domain, AssistantDomain::Client);
        assert!(
            intent
                .entities
                .iter()
                .any(|entity| entity.entity_type == AssistantEntityType::Metric)
        );
    }

    #[test]
    fn extracts_trusted_person_name() {
        let extraction = extract_message_facts("find client named Tony");

        assert!(extraction.entities.iter().any(|entity| {
            entity.entity_type == AssistantEntityType::PersonName && entity.value == "Tony"
        }));
        assert!(extraction.candidates.iter().any(|candidate| {
            candidate.field == PayloadField::PersonName && candidate.trust == PayloadTrust::Trusted
        }));
    }

    fn reference(value: &str) -> DateTime<Utc> {
        value.parse().unwrap()
    }

    #[test]
    fn temporal_uses_jakarta_date_and_exact_period_boundaries() {
        let instant = reference("2026-01-01T17:30:00Z");
        let today = extract_message_facts_at("show deposits today", instant, 366);
        assert_eq!(today.constraints.from_date.as_deref(), Some("2026-01-02"));
        assert_eq!(today.constraints.to_date.as_deref(), Some("2026-01-02"));
        assert_eq!(today.temporal_provenance.unwrap().timezone, "Asia/Jakarta");

        let year = extract_message_facts_at("laporan tahun ini", instant, 366);
        assert_eq!(year.constraints.from_date.as_deref(), Some("2026-01-01"));
        assert_eq!(year.constraints.to_date.as_deref(), Some("2026-12-31"));

        let week = extract_message_facts_at("last week", reference("2026-03-11T12:00:00Z"), 366);
        assert_eq!(week.constraints.from_date.as_deref(), Some("2026-03-02"));
        assert_eq!(week.constraints.to_date.as_deref(), Some("2026-03-08"));
    }

    #[test]
    fn temporal_validates_dates_ranges_and_counts() {
        let instant = reference("2026-03-11T12:00:00Z");
        let leap = extract_message_facts_at("2024-02-29", instant, 366);
        assert_eq!(leap.constraints.from_date, leap.constraints.to_date);
        assert!(
            extract_message_facts_at("2026-02-29", instant, 366)
                .temporal_error
                .is_some()
        );
        assert!(
            extract_message_facts_at("from 2026-03-02 to 2026-03-01", instant, 366)
                .temporal_error
                .is_some()
        );
        assert!(
            extract_message_facts_at("last 0 days", instant, 366)
                .temporal_error
                .is_some()
        );

        let range = extract_message_facts_at("dari 2026-03-01 sampai 2026-03-03", instant, 366);
        assert_eq!(range.constraints.from_date.as_deref(), Some("2026-03-01"));
        assert_eq!(range.constraints.to_date.as_deref(), Some("2026-03-03"));
        let days = extract_message_facts_at("last 3 days", instant, 366);
        assert_eq!(days.constraints.from_date.as_deref(), Some("2026-03-09"));
        assert_eq!(days.constraints.to_date.as_deref(), Some("2026-03-11"));
        assert!(days.constraints.quantity.is_none());
    }

    #[test]
    fn temporal_reuses_the_same_job_reference_after_clarification() {
        let job_reference = reference("2026-12-31T18:00:00Z");
        let initial = extract_message_facts_at("today", job_reference, 366);
        let clarification = extract_message_facts_at("hari ini", job_reference, 366);

        assert_eq!(
            initial.constraints.from_date,
            clarification.constraints.from_date
        );
        assert_eq!(
            initial.constraints.to_date,
            clarification.constraints.to_date
        );
        assert_eq!(
            clarification.temporal_provenance.unwrap().reference_instant,
            job_reference
        );
    }
}
