//! Per-parameter policy declared in capability YAML. Replaces the legacy
//! `required_parameters` / `optional_parameters` / `clarification.missing_parameters`
//! trio with a single, expressive block that supports whitelisted default
//! expressions and hard caps.
//!
//! The default-expression grammar is a fixed allowlist parsed at YAML load
//! time (see `DefaultExpr::parse`). No user-evaluable expression is ever
//! accepted at runtime.

use chrono::{Datelike, Months, NaiveDate};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParameterType {
    Date,
    Integer,
    IntegerArray,
    String,
    Currency,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DefaultExpr {
    BusinessToday,
    WallToday,
    BusinessTodayMinusDays(u16),
    BusinessTodayMinusMonths(u16),
    BusinessTodayMinusYears(u16),
    StartOfMonthBusinessToday,
    EndOfMonthBusinessToday,
    Unbounded,
    AuthorizedScope,
    LiteralInt(i64),
    LiteralDate(NaiveDate),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParameterPolicy {
    pub name: String,
    pub kind: ParameterType,
    pub required: bool,
    pub default: Option<DefaultExpr>,
    pub fill_when_missing: bool,
    pub user_may_override: bool,
    pub hard_cap: Option<i64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EvaluationContext {
    pub business_today: NaiveDate,
    pub wall_today: NaiveDate,
    pub authorized_office_ids: Vec<i64>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ResolvedValue {
    Date(NaiveDate),
    Integer(i64),
    IntegerArray(Vec<i64>),
    Unbounded,
}

impl DefaultExpr {
    /// Parse the fixed whitelist from spec §5.2. Any string outside the
    /// grammar returns an error — no user-evaluable expressions are ever
    /// accepted.
    pub fn parse(input: &str) -> Result<Self, String> {
        let trimmed = input.trim();
        match trimmed {
            "business_today" => return Ok(Self::BusinessToday),
            "wall_today" => return Ok(Self::WallToday),
            "unbounded" => return Ok(Self::Unbounded),
            "authorized_scope" => return Ok(Self::AuthorizedScope),
            "start_of_month(business_today)" => return Ok(Self::StartOfMonthBusinessToday),
            "end_of_month(business_today)" => return Ok(Self::EndOfMonthBusinessToday),
            _ => {}
        }
        if let Some(rest) = trimmed.strip_prefix("business_today - ")
            && let Some(unit) = rest.chars().last()
        {
            let number: u16 = rest[..rest.len() - 1]
                .parse()
                .map_err(|_| format!("invalid duration in `{input}`"))?;
            return match unit {
                'd' => Ok(Self::BusinessTodayMinusDays(number)),
                'm' => Ok(Self::BusinessTodayMinusMonths(number)),
                'y' => Ok(Self::BusinessTodayMinusYears(number)),
                _ => Err(format!("unknown duration unit in `{input}`")),
            };
        }
        if let Ok(date) = NaiveDate::parse_from_str(trimmed, "%Y-%m-%d") {
            return Ok(Self::LiteralDate(date));
        }
        if let Ok(int) = trimmed.parse::<i64>() {
            return Ok(Self::LiteralInt(int));
        }
        Err(format!("`{input}` is not an allowed default expression"))
    }

    pub fn evaluate(&self, ctx: &EvaluationContext) -> ResolvedValue {
        match self {
            Self::BusinessToday => ResolvedValue::Date(ctx.business_today),
            Self::WallToday => ResolvedValue::Date(ctx.wall_today),
            Self::BusinessTodayMinusDays(n) => {
                ResolvedValue::Date(ctx.business_today - chrono::Duration::days(i64::from(*n)))
            }
            Self::BusinessTodayMinusMonths(n) => {
                ResolvedValue::Date(subtract_months(ctx.business_today, u32::from(*n)))
            }
            Self::BusinessTodayMinusYears(n) => {
                ResolvedValue::Date(subtract_months(ctx.business_today, u32::from(*n) * 12))
            }
            Self::StartOfMonthBusinessToday => {
                ResolvedValue::Date(start_of_month(ctx.business_today))
            }
            Self::EndOfMonthBusinessToday => ResolvedValue::Date(end_of_month(ctx.business_today)),
            Self::Unbounded => ResolvedValue::Unbounded,
            Self::AuthorizedScope => ResolvedValue::IntegerArray(ctx.authorized_office_ids.clone()),
            Self::LiteralInt(v) => ResolvedValue::Integer(*v),
            Self::LiteralDate(d) => ResolvedValue::Date(*d),
        }
    }
}

fn subtract_months(date: NaiveDate, months: u32) -> NaiveDate {
    date.checked_sub_months(Months::new(months)).unwrap_or(date)
}

fn start_of_month(date: NaiveDate) -> NaiveDate {
    NaiveDate::from_ymd_opt(date.year(), date.month(), 1).unwrap_or(date)
}

fn end_of_month(date: NaiveDate) -> NaiveDate {
    let next = start_of_month(date)
        .checked_add_months(Months::new(1))
        .unwrap_or(date);
    next - chrono::Duration::days(1)
}

/// Validation errors raised by the catalog validator (spec §5.1).
#[derive(Debug, PartialEq, Eq)]
pub enum PolicyValidationError {
    QueryRequiredParamHasNoDefault { name: String },
    HardCapOnNonIntegerType { name: String },
    OfficeIdsMustNotAllowOverride,
    DuplicateParameterName { name: String },
}

pub fn validate_policies(
    policies: &[ParameterPolicy],
    query_required_names: &[&str],
) -> Result<(), PolicyValidationError> {
    let mut seen = std::collections::BTreeSet::new();
    for policy in policies {
        if !seen.insert(&policy.name) {
            return Err(PolicyValidationError::DuplicateParameterName {
                name: policy.name.clone(),
            });
        }
        if policy.hard_cap.is_some()
            && !matches!(
                policy.kind,
                ParameterType::Integer | ParameterType::IntegerArray
            )
        {
            return Err(PolicyValidationError::HardCapOnNonIntegerType {
                name: policy.name.clone(),
            });
        }
        if policy.name == "office_ids" && policy.user_may_override {
            return Err(PolicyValidationError::OfficeIdsMustNotAllowOverride);
        }
    }
    for required in query_required_names {
        let policy = policies
            .iter()
            .find(|p| p.name == *required)
            .ok_or_else(|| PolicyValidationError::QueryRequiredParamHasNoDefault {
                name: (*required).to_string(),
            })?;
        if !policy.required && policy.default.is_none() {
            return Err(PolicyValidationError::QueryRequiredParamHasNoDefault {
                name: policy.name.clone(),
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> EvaluationContext {
        EvaluationContext {
            business_today: NaiveDate::from_ymd_opt(2026, 7, 24).unwrap(),
            wall_today: NaiveDate::from_ymd_opt(2026, 7, 24).unwrap(),
            authorized_office_ids: vec![1, 2],
        }
    }

    // parser --------------------------------------------------------------

    #[test]
    fn parses_business_today_variants() {
        assert_eq!(
            DefaultExpr::parse("business_today").unwrap(),
            DefaultExpr::BusinessToday
        );
        assert_eq!(
            DefaultExpr::parse("wall_today").unwrap(),
            DefaultExpr::WallToday
        );
        assert_eq!(
            DefaultExpr::parse("unbounded").unwrap(),
            DefaultExpr::Unbounded
        );
        assert_eq!(
            DefaultExpr::parse("authorized_scope").unwrap(),
            DefaultExpr::AuthorizedScope
        );
    }

    #[test]
    fn parses_business_today_minus_units() {
        assert_eq!(
            DefaultExpr::parse("business_today - 7d").unwrap(),
            DefaultExpr::BusinessTodayMinusDays(7)
        );
        assert_eq!(
            DefaultExpr::parse("business_today - 1m").unwrap(),
            DefaultExpr::BusinessTodayMinusMonths(1)
        );
        assert_eq!(
            DefaultExpr::parse("business_today - 2y").unwrap(),
            DefaultExpr::BusinessTodayMinusYears(2)
        );
    }

    #[test]
    fn parses_month_bounds_and_literals() {
        assert_eq!(
            DefaultExpr::parse("start_of_month(business_today)").unwrap(),
            DefaultExpr::StartOfMonthBusinessToday
        );
        assert_eq!(
            DefaultExpr::parse("end_of_month(business_today)").unwrap(),
            DefaultExpr::EndOfMonthBusinessToday
        );
        assert_eq!(
            DefaultExpr::parse("2026-07-24").unwrap(),
            DefaultExpr::LiteralDate(NaiveDate::from_ymd_opt(2026, 7, 24).unwrap())
        );
        assert_eq!(
            DefaultExpr::parse("50").unwrap(),
            DefaultExpr::LiteralInt(50)
        );
    }

    #[test]
    fn rejects_unknown_expressions() {
        assert!(DefaultExpr::parse("today() + 1w").is_err());
        assert!(DefaultExpr::parse("business_today + 1d").is_err());
        assert!(DefaultExpr::parse("").is_err());
    }

    // evaluator -----------------------------------------------------------

    #[test]
    fn evaluates_relative_dates() {
        let c = ctx();
        assert_eq!(
            DefaultExpr::BusinessToday.evaluate(&c),
            ResolvedValue::Date(NaiveDate::from_ymd_opt(2026, 7, 24).unwrap())
        );
        assert_eq!(
            DefaultExpr::BusinessTodayMinusDays(1).evaluate(&c),
            ResolvedValue::Date(NaiveDate::from_ymd_opt(2026, 7, 23).unwrap())
        );
        assert_eq!(
            DefaultExpr::BusinessTodayMinusMonths(1).evaluate(&c),
            ResolvedValue::Date(NaiveDate::from_ymd_opt(2026, 6, 24).unwrap())
        );
        assert_eq!(
            DefaultExpr::StartOfMonthBusinessToday.evaluate(&c),
            ResolvedValue::Date(NaiveDate::from_ymd_opt(2026, 7, 1).unwrap())
        );
        assert_eq!(
            DefaultExpr::EndOfMonthBusinessToday.evaluate(&c),
            ResolvedValue::Date(NaiveDate::from_ymd_opt(2026, 7, 31).unwrap())
        );
    }

    #[test]
    fn evaluates_scalars_and_scope() {
        let c = ctx();
        assert_eq!(
            DefaultExpr::Unbounded.evaluate(&c),
            ResolvedValue::Unbounded
        );
        assert_eq!(
            DefaultExpr::AuthorizedScope.evaluate(&c),
            ResolvedValue::IntegerArray(vec![1, 2])
        );
        assert_eq!(
            DefaultExpr::LiteralInt(10).evaluate(&c),
            ResolvedValue::Integer(10)
        );
    }

    // validator -----------------------------------------------------------

    fn required(name: &str) -> ParameterPolicy {
        ParameterPolicy {
            name: name.into(),
            kind: ParameterType::Date,
            required: true,
            default: None,
            fill_when_missing: false,
            user_may_override: true,
            hard_cap: None,
        }
    }

    fn defaulted(name: &str) -> ParameterPolicy {
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

    #[test]
    fn validator_accepts_required_with_default() {
        assert!(validate_policies(&[defaulted("from_date")], &["from_date"]).is_ok());
    }

    #[test]
    fn validator_rejects_query_required_with_no_default() {
        let mut p = defaulted("from_date");
        p.required = false;
        p.default = None;
        assert!(matches!(
            validate_policies(&[p], &["from_date"]),
            Err(PolicyValidationError::QueryRequiredParamHasNoDefault { .. })
        ));
    }

    #[test]
    fn validator_rejects_hard_cap_on_non_integer() {
        let mut p = required("from_date");
        p.hard_cap = Some(10);
        assert!(matches!(
            validate_policies(&[p], &["from_date"]),
            Err(PolicyValidationError::HardCapOnNonIntegerType { .. })
        ));
    }

    #[test]
    fn validator_rejects_office_ids_override() {
        let mut p = ParameterPolicy {
            name: "office_ids".into(),
            kind: ParameterType::IntegerArray,
            required: false,
            default: Some(DefaultExpr::AuthorizedScope),
            fill_when_missing: true,
            user_may_override: true,
            hard_cap: None,
        };
        p.user_may_override = true;
        assert_eq!(
            validate_policies(&[p], &[]),
            Err(PolicyValidationError::OfficeIdsMustNotAllowOverride)
        );
    }

    #[test]
    fn validator_rejects_duplicate_names() {
        assert!(matches!(
            validate_policies(&[required("from_date"), required("from_date")], &[]),
            Err(PolicyValidationError::DuplicateParameterName { .. })
        ));
    }
}
