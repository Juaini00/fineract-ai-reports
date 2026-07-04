use chrono::{Datelike, NaiveDate};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub const OTHER_ACTIVITY_CAPABILITY: &str = "other_activity";

/// Append an "Others" escape-hatch option so users can decline all suggestions
/// and describe their intent in their own words.
pub fn append_others_option(
    mut options: Vec<ClarificationOption>,
    others_label: &str,
) -> Vec<ClarificationOption> {
    if !options
        .iter()
        .any(|opt| opt.capability == OTHER_ACTIVITY_CAPABILITY)
    {
        options.push(ClarificationOption {
            label: others_label.to_string(),
            capability: OTHER_ACTIVITY_CAPABILITY.to_string(),
            output_mode: None,
        });
    }
    options
}

/// Pure decision function for gap-based classifier choice. Given top-N
/// candidate scores (sorted DESC) and matching capability ids, decide whether
/// to Match top1, Clarify with options, or return Unsupported.
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum DecideOutcome {
    Match { capability: String },
    Clarify,
    Unsupported,
}

pub fn decide_from_scores(
    policy: &crate::knowledge::model::ClassificationPolicy,
    sorted_scores: &[f32],
    sorted_capabilities: &[&str],
) -> DecideOutcome {
    let top = sorted_scores.first().copied().unwrap_or(0.0);
    if top < policy.min_floor {
        return DecideOutcome::Unsupported;
    }
    let second = sorted_scores.get(1).copied().unwrap_or(0.0);
    if (top - second) >= policy.min_gap
        && let Some(cap) = sorted_capabilities.first()
    {
        return DecideOutcome::Match {
            capability: (*cap).to_string(),
        };
    }
    DecideOutcome::Clarify
}

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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_mode: Option<String>,
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

pub fn classify_clarification_response(
    original: &ClassificationResult,
    response: &str,
) -> ClassificationResult {
    let normalized = response.to_lowercase();
    if let Some(option) = selected_option(original, &normalized) {
        if option.capability == OTHER_ACTIVITY_CAPABILITY {
            return ClassificationResult {
                outcome: ClassificationOutcome::ClarificationRequired,
                domain: original.domain.clone(),
                capability: None,
                confidence: 0.6,
                params: original.params.clone(),
                clarification: Some("Please describe what you would like to know.".to_string()),
                options: Vec::new(),
                source: Some("clarification_other_selected".to_string()),
                candidates: original.candidates.clone(),
            };
        }

        let mut params = original.params.clone();
        if option_needs_limit(option) && params.get("limit").is_none() {
            params["limit"] = json!(limit_from_message(&normalized).unwrap_or_else(|| {
                default_limit_for_output_mode(option.output_mode.as_deref())
            }));
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

    let mut result = original.clone();
    result.clarification = Some("Please choose one of the available report options.".to_string());
    result
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
        clarification: Some("Please choose one of the available report options.".to_string()),
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

    if let Some(option) = original.options.iter().find(|option| {
        let capability = option.capability.to_lowercase();
        let label = option.label.to_lowercase();
        normalized_response.contains(&capability)
            || normalized_response.contains(&label)
            || label.contains(normalized_response)
            || tokens_match(normalized_response, &label)
    }) {
        return Some(option);
    }

    original
        .options
        .iter()
        .fold(None, |best: Option<(&ClarificationOption, i32)>, option| {
            let score = option_score(option, normalized_response);
            if score <= 0 {
                return best;
            }
            match best {
                Some((_, best_score)) if best_score >= score => best,
                _ => Some((option, score)),
            }
        })
        .map(|(option, _)| option)
}

fn option_score(option: &ClarificationOption, response: &str) -> i32 {
    let mut score = 0;
    let tokens = comparable_tokens(response);
    let capability = option.capability.as_str();
    let output_mode = option.output_mode.as_deref();

    if output_mode.is_some_and(|mode| mode.ends_with("top_n"))
        && has_any_token(
            &tokens,
            &["largest", "biggest", "top", "max", "maximum", "highest"],
        )
    {
        score += 5;
    }
    if output_mode.is_some_and(|mode| mode == "total" || mode == "monthly_breakdown")
        && has_any_token(&tokens, &["total", "sum", "aggregate", "overall"])
    {
        score += 5;
    }
    if capability.contains("deposit") && has_any_token(&tokens, &["deposit", "deposits", "setoran"])
    {
        score += 3;
    }
    if capability.contains("withdrawal")
        && has_any_token(&tokens, &["withdrawal", "withdrawals", "withdraw", "tarik"])
    {
        score += 3;
    }
    if capability == OTHER_ACTIVITY_CAPABILITY {
        if has_any_token(&tokens, &["other", "others", "all"])
            || tokens
                .iter()
                .any(|token| token.starts_with("activ") || token.starts_with("actic"))
            || has_any_token(&tokens, &["transaction", "transactions"])
        {
            score += 4;
        }
        if has_any_token(
            &tokens,
            &["largest", "total", "deposit", "withdrawal", "withdraw"],
        ) {
            score -= 3;
        }
    }

    score
}

fn has_any_token(tokens: &[String], needles: &[&str]) -> bool {
    tokens
        .iter()
        .any(|token| needles.iter().any(|needle| token == needle))
}

fn tokens_match(response: &str, label: &str) -> bool {
    let response_tokens = comparable_tokens(response);
    if response_tokens.is_empty() {
        return false;
    }
    let label_tokens = comparable_tokens(label);
    response_tokens
        .iter()
        .all(|token| label_tokens.iter().any(|label_token| label_token == token))
}

fn comparable_tokens(value: &str) -> Vec<String> {
    value
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(|token| token.trim_end_matches('s').to_string())
        .collect()
}

fn option_needs_limit(option: &ClarificationOption) -> bool {
    option
        .output_mode
        .as_deref()
        .is_some_and(|mode| mode.ends_with("top_n"))
}

fn default_limit_for_output_mode(output_mode: Option<&str>) -> u32 {
    if output_mode == Some("monthly_top_n") {
        1
    } else {
        10
    }
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

fn limit_from_message(message: &str) -> Option<u32> {
    message
        .split(|character: char| !character.is_ascii_alphanumeric())
        .find_map(|token| token.parse::<u32>().ok())
}

#[cfg(test)]
mod tests;
