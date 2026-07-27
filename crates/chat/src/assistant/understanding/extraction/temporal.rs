use chrono::{DateTime, Datelike, Duration, NaiveDate, Utc};

use super::token::tokens_with_spans;
use super::{TemporalProvenance, TemporalValidationError};

pub(super) struct ResolvedTemporal {
    pub(super) from: NaiveDate,
    pub(super) to: NaiveDate,
    pub(super) provenance: TemporalProvenance,
}

pub(super) fn resolve_temporal(
    message: &str,
    reference_instant: DateTime<Utc>,
    business_today: NaiveDate,
    max_range_days: i64,
) -> Result<Option<ResolvedTemporal>, TemporalValidationError> {
    let lower = message.to_ascii_lowercase();
    let tokens = tokens_with_spans(&lower);
    let today = business_today;
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

fn parse_date(value: &str) -> Result<NaiveDate, TemporalValidationError> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d").map_err(|_| TemporalValidationError {
        code: "temporal_invalid_date".into(),
        message: "Use a valid Gregorian date in YYYY-MM-DD format.".into(),
    })
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
