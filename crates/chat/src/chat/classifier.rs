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
    /// Index source kind: capability / data_area / domain / query. Optional for
    /// back-compat with older jobs whose state_json predates broader retrieval.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_type: Option<String>,
}

pub fn classify_message(message: &str, today: NaiveDate) -> ClassificationResult {
    let normalized = message.to_lowercase();

    if contains_any(
        &normalized,
        &[
            "savings",
            "deposit",
            "deposits",
            "withdrawal",
            "withdrawals",
            "money in",
            "money out",
            "put into savings",
        ],
    ) {
        if let Some(capability) = inferred_capability(&normalized) {
            let mut params = json!({ "office_scope": "authorized_scope" });
            if capability != "savings_balance_summary" {
                let Some((from_date, to_date)) = date_range(&normalized, today) else {
                    return ClassificationResult {
                        outcome: ClassificationOutcome::ClarificationRequired,
                        domain: Some("savings".to_string()),
                        capability: None,
                        confidence: 0.45,
                        params: json!({}),
                        clarification: Some(
                            "Please clarify the report date or period.".to_string(),
                        ),
                        options: Vec::new(),
                        source: Some("local_rule".to_string()),
                        candidates: Vec::new(),
                    };
                };
                params["from_date"] = json!(from_date.to_string());
                params["to_date"] = json!(to_date.to_string());
            }
            if capability.ends_with("_top_n") {
                params["limit"] = json!(
                    limit_from_message(&normalized)
                        .unwrap_or_else(|| default_limit_for(capability))
                );
            }

            return ClassificationResult {
                outcome: ClassificationOutcome::Matched,
                domain: Some("savings".to_string()),
                capability: Some(capability.to_string()),
                confidence: 0.86,
                params,
                clarification: None,
                options: Vec::new(),
                source: Some("local_rule".to_string()),
                candidates: Vec::new(),
            };
        }
    }

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
        if option.capability.ends_with("_top_n") && params.get("limit").is_none() {
            params["limit"] = json!(
                limit_from_message(&normalized)
                    .unwrap_or_else(|| default_limit_for(&option.capability))
            );
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

    let capability = inferred_capability(&normalized);

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
    if capability.ends_with("_top_n") && params.get("limit").is_none() {
        params["limit"] =
            json!(limit_from_message(&normalized).unwrap_or_else(|| default_limit_for(capability)));
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

fn inferred_capability(message: &str) -> Option<&'static str> {
    if contains_any(message, &["balance", "portfolio"]) {
        return Some("savings_balance_summary");
    }

    let activity = if contains_any(message, &["withdrawal", "withdrawals", "money out"]) {
        "withdrawal"
    } else {
        "deposit"
    };
    let monthly = contains_any(message, &["monthly", "per month", "by month", "breakdown"]);
    let top = contains_any(message, &["largest", "top", "biggest", "transactions"]);

    match (activity, monthly, top) {
        ("deposit", true, true) => Some("savings_deposit_monthly_top_n"),
        ("deposit", true, false) => Some("savings_deposit_monthly_breakdown"),
        ("deposit", false, true) => Some("savings_deposit_top_n"),
        ("deposit", false, false) if contains_any(message, &["total", "amount", "sum"]) => {
            Some("savings_deposit_total")
        }
        ("withdrawal", true, true) => Some("savings_withdrawal_monthly_top_n"),
        ("withdrawal", true, false) => Some("savings_withdrawal_monthly_breakdown"),
        ("withdrawal", false, true) => Some("savings_withdrawal_top_n"),
        ("withdrawal", false, false) if contains_any(message, &["total", "amount", "sum"]) => {
            Some("savings_withdrawal_total")
        }
        _ => None,
    }
}

fn default_limit_for(capability: &str) -> u32 {
    if capability.contains("monthly") {
        1
    } else {
        10
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
    let mut params = json!({ "office_scope": "authorized_scope" });

    // Snapshot output_modes have no time dimension — skip date_range entirely.
    if output_mode != "summary" {
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
        params["from_date"] = json!(from_date.to_string());
        params["to_date"] = json!(to_date.to_string());
    }

    // `top_n` (atomic) and `monthly_top_n` (per-month) both need a `limit`
    // parameter. Defaults: 10 for atomic top_n, 1 for monthly_top_n (one row per
    // month unless the user asks for top-N per month).
    if output_mode.ends_with("top_n") {
        let default_limit = if output_mode == "monthly_top_n" {
            1
        } else {
            10
        };
        params["limit"] = json!(limit_from_message(&normalized).unwrap_or(default_limit));
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
    // Lowercase once so sub-helpers can match `January` / `Januari` /
    // `JANUARY` consistently. Production callers already lowercase, but unit
    // tests and any direct call get the same treatment here.
    let message = message.to_lowercase();
    let message = message.as_str();

    // Order matters: more specific patterns first.
    if let Some(range) = month_range(message, today) {
        return Some(range);
    }
    if let Some(range) = relative_count_range(message, today) {
        return Some(range);
    }
    if let Some(range) = relative_literal_range(message, today) {
        return Some(range);
    }
    if let Some(range) = bare_year_range(message) {
        return Some(range);
    }

    if contains_any(message, &["today"]) {
        return Some((today, today));
    }

    if contains_any(message, &["this month", "bulan ini"]) {
        let first_day = NaiveDate::from_ymd_opt(today.year(), today.month(), 1)?;
        return Some((first_day, today));
    }

    if contains_any(message, &["this week", "minggu ini"]) {
        let days_from_monday = today.weekday().num_days_from_monday() as i64;
        return Some((today - chrono::Duration::days(days_from_monday), today));
    }

    None
}

fn month_range(message: &str, today: NaiveDate) -> Option<(NaiveDate, NaiveDate)> {
    let tokens = message
        .split(|character: char| !character.is_alphanumeric())
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();
    let months = tokens
        .iter()
        .filter_map(|token| month_number(token))
        .collect::<Vec<_>>();
    if months.is_empty() {
        return None;
    }
    // Use explicit year token if present, otherwise default to current year.
    let year = tokens
        .iter()
        .rev()
        .find_map(|token| {
            token
                .parse::<i32>()
                .ok()
                .filter(|y| (2000..2100).contains(y))
        })
        .unwrap_or_else(|| today.year());

    match months.as_slice() {
        [month] => {
            let from_date = NaiveDate::from_ymd_opt(year, *month, 1)?;
            let to_date = end_of_month(year, *month)?;
            Some((from_date, to_date))
        }
        [from_month, to_month, ..] => {
            let from_date = NaiveDate::from_ymd_opt(year, *from_month, 1)?;
            let to_date = end_of_month(year, *to_month)?;
            Some((from_date, to_date))
        }
        _ => None,
    }
}

fn relative_literal_range(message: &str, today: NaiveDate) -> Option<(NaiveDate, NaiveDate)> {
    if contains_any(message, &["yesterday", "kemarin"]) {
        let y = today - chrono::Duration::days(1);
        return Some((y, y));
    }
    if contains_any(message, &["last year", "tahun lalu", "tahun kemarin"]) {
        let year = today.year() - 1;
        let from = NaiveDate::from_ymd_opt(year, 1, 1)?;
        let to = NaiveDate::from_ymd_opt(year, 12, 31)?;
        return Some((from, to));
    }
    if contains_any(
        message,
        &[
            "this year",
            "tahun ini",
            "year to date",
            "year-to-date",
            "ytd",
        ],
    ) {
        let from = NaiveDate::from_ymd_opt(today.year(), 1, 1)?;
        return Some((from, today));
    }
    if contains_any(message, &["last month", "bulan lalu", "bulan kemarin"]) {
        let (year, month) = previous_month(today.year(), today.month());
        let from = NaiveDate::from_ymd_opt(year, month, 1)?;
        let to = end_of_month(year, month)?;
        return Some((from, to));
    }
    if contains_any(message, &["last week", "minggu lalu"]) {
        let days_from_monday = today.weekday().num_days_from_monday() as i64;
        let this_monday = today - chrono::Duration::days(days_from_monday);
        let last_monday = this_monday - chrono::Duration::days(7);
        let last_sunday = this_monday - chrono::Duration::days(1);
        return Some((last_monday, last_sunday));
    }
    None
}

/// "last 7 days" / "past 30 days" / "3 months ago" / "2 minggu terakhir" / "5 hari lalu".
/// Looks for a number followed by a unit token. Returns (today - N units, today).
fn relative_count_range(message: &str, today: NaiveDate) -> Option<(NaiveDate, NaiveDate)> {
    let tokens: Vec<&str> = message
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .collect();
    for (i, token) in tokens.iter().enumerate() {
        let Ok(n) = token.parse::<i64>() else {
            continue;
        };
        if !(1..=120).contains(&n) {
            continue;
        }
        let unit_token = tokens.get(i + 1)?.to_ascii_lowercase();
        let from = match unit_token.as_str() {
            "day" | "days" | "hari" => Some(today - chrono::Duration::days(n)),
            "week" | "weeks" | "minggu" => Some(today - chrono::Duration::days(7 * n)),
            "month" | "months" | "bulan" => Some(subtract_months(today, n as u32)),
            _ => None,
        };
        if let Some(from) = from {
            return Some((from, today));
        }
    }
    None
}

/// Bare year mention like "deposits in 2026" — no month, no relative word.
fn bare_year_range(message: &str) -> Option<(NaiveDate, NaiveDate)> {
    let tokens: Vec<&str> = message
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .collect();
    if tokens.iter().any(|t| month_number(t).is_some()) {
        return None;
    }
    let year = tokens
        .iter()
        .rev()
        .find_map(|t| t.parse::<i32>().ok().filter(|y| (2000..2100).contains(y)))?;
    let from = NaiveDate::from_ymd_opt(year, 1, 1)?;
    let to = NaiveDate::from_ymd_opt(year, 12, 31)?;
    Some((from, to))
}

fn previous_month(year: i32, month: u32) -> (i32, u32) {
    if month == 1 {
        (year - 1, 12)
    } else {
        (year, month - 1)
    }
}

fn subtract_months(date: NaiveDate, n: u32) -> NaiveDate {
    let mut year = date.year();
    let mut month = date.month();
    for _ in 0..n {
        let (y, m) = previous_month(year, month);
        year = y;
        month = m;
    }
    NaiveDate::from_ymd_opt(year, month, date.day())
        .or_else(|| end_of_month(year, month))
        .unwrap_or(date)
}

fn month_number(token: &str) -> Option<u32> {
    match token {
        "jan" | "january" | "januari" => Some(1),
        "feb" | "february" | "februari" => Some(2),
        "mar" | "march" | "maret" => Some(3),
        "apr" | "april" => Some(4),
        "may" | "mei" => Some(5),
        "jun" | "june" | "juni" => Some(6),
        "jul" | "july" | "juli" => Some(7),
        "aug" | "august" | "agustus" => Some(8),
        "sep" | "sept" | "september" => Some(9),
        "oct" | "october" | "okt" | "oktober" => Some(10),
        "nov" | "november" => Some(11),
        "dec" | "december" | "des" | "desember" => Some(12),
        _ => None,
    }
}

fn end_of_month(year: i32, month: u32) -> Option<NaiveDate> {
    let (next_year, next_month) = if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    };
    NaiveDate::from_ymd_opt(next_year, next_month, 1).map(|date| date - chrono::Duration::days(1))
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
    fn classifies_withdrawal_monthly_top_n() {
        let result = classify_message("Show top withdrawals per month this month", today());

        assert_eq!(result.outcome, ClassificationOutcome::Matched);
        assert_eq!(
            result.capability.as_deref(),
            Some("savings_withdrawal_monthly_top_n")
        );
        assert_eq!(result.params["limit"], 1);
    }

    #[test]
    fn parses_yesterday() {
        let range = date_range("total deposit yesterday", today()).unwrap();
        assert_eq!(
            range,
            (
                NaiveDate::from_ymd_opt(2026, 6, 20).unwrap(),
                NaiveDate::from_ymd_opt(2026, 6, 20).unwrap(),
            )
        );
    }

    #[test]
    fn parses_kemarin() {
        let range = date_range("total setoran kemarin", today()).unwrap();
        assert_eq!(range.0, NaiveDate::from_ymd_opt(2026, 6, 20).unwrap());
    }

    #[test]
    fn parses_this_year_and_ytd() {
        let r1 = date_range("deposits this year", today()).unwrap();
        let r2 = date_range("deposits ytd", today()).unwrap();
        let r3 = date_range("setoran tahun ini", today()).unwrap();
        let expected = (NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(), today());
        assert_eq!(r1, expected);
        assert_eq!(r2, expected);
        assert_eq!(r3, expected);
    }

    #[test]
    fn parses_last_year() {
        let range = date_range("deposits last year", today()).unwrap();
        assert_eq!(
            range,
            (
                NaiveDate::from_ymd_opt(2025, 1, 1).unwrap(),
                NaiveDate::from_ymd_opt(2025, 12, 31).unwrap(),
            )
        );
    }

    #[test]
    fn parses_last_month() {
        let range = date_range("deposits last month", today()).unwrap();
        assert_eq!(
            range,
            (
                NaiveDate::from_ymd_opt(2026, 5, 1).unwrap(),
                NaiveDate::from_ymd_opt(2026, 5, 31).unwrap(),
            )
        );
    }

    #[test]
    fn parses_last_month_january_wraps() {
        let today_jan = NaiveDate::from_ymd_opt(2026, 1, 15).unwrap();
        let range = date_range("deposits last month", today_jan).unwrap();
        assert_eq!(
            range,
            (
                NaiveDate::from_ymd_opt(2025, 12, 1).unwrap(),
                NaiveDate::from_ymd_opt(2025, 12, 31).unwrap(),
            )
        );
    }

    #[test]
    fn parses_last_week() {
        // today is 2026-06-21 (Sunday). this_monday = 2026-06-15. last_monday = 2026-06-08, last_sunday = 2026-06-14.
        let range = date_range("deposits last week", today()).unwrap();
        assert_eq!(
            range,
            (
                NaiveDate::from_ymd_opt(2026, 6, 8).unwrap(),
                NaiveDate::from_ymd_opt(2026, 6, 14).unwrap(),
            )
        );
    }

    #[test]
    fn parses_last_n_days() {
        let range = date_range("deposits last 7 days", today()).unwrap();
        assert_eq!(range.0, today() - chrono::Duration::days(7));
        assert_eq!(range.1, today());
    }

    #[test]
    fn parses_last_n_months_id() {
        let range = date_range("setoran 3 bulan terakhir", today()).unwrap();
        // today is 2026-06-21; minus 3 months → 2026-03-21
        assert_eq!(range.0, NaiveDate::from_ymd_opt(2026, 3, 21).unwrap());
        assert_eq!(range.1, today());
    }

    #[test]
    fn parses_bare_year() {
        let range = date_range("deposits in 2025", today()).unwrap();
        assert_eq!(
            range,
            (
                NaiveDate::from_ymd_opt(2025, 1, 1).unwrap(),
                NaiveDate::from_ymd_opt(2025, 12, 31).unwrap(),
            )
        );
    }

    #[test]
    fn parses_month_range_default_year() {
        // No year token; should default to today.year() = 2026.
        let range = date_range("deposits from January to September", today()).unwrap();
        assert_eq!(
            range,
            (
                NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
                NaiveDate::from_ymd_opt(2026, 9, 30).unwrap(),
            )
        );
    }

    #[test]
    fn parses_month_range_with_year() {
        let range = date_range("deposits from January to September 2025", today()).unwrap();
        assert_eq!(
            range,
            (
                NaiveDate::from_ymd_opt(2025, 1, 1).unwrap(),
                NaiveDate::from_ymd_opt(2025, 9, 30).unwrap(),
            )
        );
    }

    #[test]
    fn parses_id_month_range_with_sampai() {
        let range = date_range("setoran dari Januari sampai September 2026", today()).unwrap();
        assert_eq!(
            range,
            (
                NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
                NaiveDate::from_ymd_opt(2026, 9, 30).unwrap(),
            )
        );
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
    fn classifies_withdrawal_monthly_clarification_response() {
        let original = clarify_retrieved_capabilities(
            "Show savings activity this month",
            today(),
            Some("savings".to_string()),
            Vec::new(),
            0.7,
            Vec::new(),
        );

        let result = classify_clarification_response(&original, "monthly withdrawal breakdown");

        assert_eq!(result.outcome, ClassificationOutcome::Matched);
        assert_eq!(
            result.capability.as_deref(),
            Some("savings_withdrawal_monthly_breakdown")
        );
        assert_eq!(result.params["from_date"], "2026-06-01");
    }

    #[test]
    fn parses_indonesian_month_range() {
        let result = classify_retrieved_capability(
            "saya mau tau deposit bulan mei - september 2025",
            today(),
            "savings",
            "savings_deposit_total",
            "total",
            0.7,
            Vec::new(),
        );

        assert_eq!(result.outcome, ClassificationOutcome::Matched);
        assert_eq!(result.params["from_date"], "2025-05-01");
        assert_eq!(result.params["to_date"], "2025-09-30");
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
