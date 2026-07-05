//! Multi-section bucketed rendering for the `savings.activity_list` capability.
//!
//! The raw executor result is a flat list of transactions. Users asked for a
//! single response that also breaks that list down into deposits / withdrawals
//! / charges sections plus per-week and per-2-day aggregations. That grouping
//! is a pure formatter concern — no schema or planner change.

use std::collections::BTreeMap;

use chrono::{Datelike, NaiveDate};
use rust_decimal::Decimal;
use serde_json::Value;

/// Which activity bucket a raw `transaction_type` belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Bucket {
    Deposit,
    Withdrawal,
    Charge,
    Other,
}

impl Bucket {
    fn from_transaction_type(t: &str) -> Self {
        match t {
            "deposit" => Bucket::Deposit,
            "withdrawal" => Bucket::Withdrawal,
            // Fees and taxes are user-visible "charges paid" activity.
            "withdrawal_fee" | "annual_fee" | "withhold_tax" => Bucket::Charge,
            _ => Bucket::Other,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Bucket::Deposit => "Deposits",
            Bucket::Withdrawal => "Withdrawals",
            Bucket::Charge => "Charges paid",
            Bucket::Other => "Other activity",
        }
    }
}

#[derive(Debug, Default, Clone)]
struct BucketTotals {
    count: usize,
    amount: Decimal,
}

impl BucketTotals {
    fn add(&mut self, amount: Decimal) {
        self.count += 1;
        self.amount += amount;
    }
}

/// Render the bucketed multi-section response. Returns `None` if rows are
/// empty (caller falls back to the standard empty-result text).
pub fn render(rows: &[Value]) -> Option<String> {
    if rows.is_empty() {
        return None;
    }

    let currency = rows
        .iter()
        .find_map(|row| row.get("currency_code").and_then(Value::as_str))
        .unwrap_or("")
        .to_string();

    // Bucket the raw rows and precompute per-bucket totals + per-day totals.
    let mut per_bucket_rows: BTreeMap<i32, Vec<&Value>> = BTreeMap::new();
    let mut per_bucket_totals: BTreeMap<i32, BucketTotals> = BTreeMap::new();
    let mut per_day: BTreeMap<NaiveDate, BTreeMap<i32, Decimal>> = BTreeMap::new();

    for row in rows {
        let Some(t) = row.get("transaction_type").and_then(Value::as_str) else {
            continue;
        };
        let Some(date_str) = row.get("transaction_date").and_then(Value::as_str) else {
            continue;
        };
        let Ok(date) = NaiveDate::parse_from_str(date_str, "%Y-%m-%d") else {
            continue;
        };
        let amount = row
            .get("amount")
            .and_then(|value| match value {
                Value::String(s) => s.parse::<Decimal>().ok(),
                Value::Number(n) => n.as_f64().and_then(|f| Decimal::try_from(f).ok()),
                _ => None,
            })
            .unwrap_or(Decimal::ZERO);

        let bucket = Bucket::from_transaction_type(t);
        let key = bucket as i32;
        per_bucket_rows.entry(key).or_default().push(row);
        per_bucket_totals.entry(key).or_default().add(amount);
        *per_day.entry(date).or_default().entry(key).or_default() += amount;
    }

    let mut sections: Vec<String> = Vec::new();
    sections.push(format!("Report returned {} activity row(s).", rows.len()));

    for bucket in [
        Bucket::Deposit,
        Bucket::Withdrawal,
        Bucket::Charge,
        Bucket::Other,
    ] {
        let key = bucket as i32;
        let Some(bucket_rows) = per_bucket_rows.get(&key) else {
            continue;
        };
        let totals = per_bucket_totals.get(&key).cloned().unwrap_or_default();
        let mut section = format!(
            "\n### {} ({} row(s), total: {}{})",
            bucket.label(),
            totals.count,
            format_currency(&currency),
            totals.amount,
        );
        for (index, row) in bucket_rows.iter().take(50).enumerate() {
            section.push_str(&format!("\n{}. {}", index + 1, format_row_line(row)));
        }
        if bucket_rows.len() > 50 {
            section.push_str(&format!(
                "\n... and {} more row(s).",
                bucket_rows.len() - 50
            ));
        }
        sections.push(section);
    }

    // Weekly aggregation — group by ISO week Monday.
    let mut per_week: BTreeMap<NaiveDate, BTreeMap<i32, Decimal>> = BTreeMap::new();
    for (day, per_bucket) in &per_day {
        let days_from_monday = day.weekday().num_days_from_monday() as i64;
        let week_start = *day - chrono::Duration::days(days_from_monday);
        let entry = per_week.entry(week_start).or_default();
        for (bucket, amount) in per_bucket {
            *entry.entry(*bucket).or_default() += *amount;
        }
    }
    if !per_week.is_empty() {
        let mut section = String::from("\n### Weekly aggregation");
        for (week_start, per_bucket) in &per_week {
            let week_end = *week_start + chrono::Duration::days(6);
            section.push_str(&format!(
                "\n- {} to {}: {}",
                week_start,
                week_end,
                render_bucket_amounts(per_bucket, &currency),
            ));
        }
        sections.push(section);
    }

    // 2-day aggregation — bucket by floor(days-since-epoch / 2). Deterministic,
    // parity-based split anchored on the earliest day in the result set.
    if let Some(earliest) = per_day.keys().next().copied() {
        let mut per_2day: BTreeMap<i64, BTreeMap<i32, Decimal>> = BTreeMap::new();
        for (day, per_bucket) in &per_day {
            let bucket_index = (*day - earliest).num_days() / 2;
            let entry = per_2day.entry(bucket_index).or_default();
            for (bucket, amount) in per_bucket {
                *entry.entry(*bucket).or_default() += *amount;
            }
        }
        let mut section = String::from("\n### 2-day aggregation");
        for (index, per_bucket) in &per_2day {
            let start = earliest + chrono::Duration::days(index * 2);
            let end = start + chrono::Duration::days(1);
            section.push_str(&format!(
                "\n- {} to {}: {}",
                start,
                end,
                render_bucket_amounts(per_bucket, &currency),
            ));
        }
        sections.push(section);
    }

    Some(sections.join("\n"))
}

fn render_bucket_amounts(per_bucket: &BTreeMap<i32, Decimal>, currency: &str) -> String {
    let mut parts: Vec<String> = Vec::new();
    for bucket in [
        Bucket::Deposit,
        Bucket::Withdrawal,
        Bucket::Charge,
        Bucket::Other,
    ] {
        if let Some(amount) = per_bucket.get(&(bucket as i32)) {
            parts.push(format!(
                "{} {}{}",
                bucket.label().to_lowercase(),
                format_currency(currency),
                amount,
            ));
        }
    }
    if parts.is_empty() {
        "no activity".to_string()
    } else {
        parts.join(", ")
    }
}

fn format_currency(currency: &str) -> String {
    if currency.is_empty() {
        String::new()
    } else {
        format!("{currency} ")
    }
}

fn format_row_line(row: &Value) -> String {
    let date = row
        .get("transaction_date")
        .and_then(Value::as_str)
        .unwrap_or("");
    let amount = row
        .get("amount")
        .map(|v| {
            v.as_str()
                .map(String::from)
                .unwrap_or_else(|| v.to_string())
        })
        .unwrap_or_default();
    let currency = row
        .get("currency_code")
        .and_then(Value::as_str)
        .unwrap_or("");
    let office = row.get("office_name").and_then(Value::as_str).unwrap_or("");
    let product = row
        .get("product_name")
        .and_then(Value::as_str)
        .unwrap_or("");
    let mut line = format!("{date} — {}{amount}", format_currency(currency));
    if !product.is_empty() {
        line.push_str(&format!(" ({product}"));
        if !office.is_empty() {
            line.push_str(&format!(", office: {office}"));
        }
        line.push(')');
    } else if !office.is_empty() {
        line.push_str(&format!(" (office: {office})"));
    }
    line
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn empty_rows_returns_none() {
        assert!(render(&[]).is_none());
    }

    #[test]
    fn buckets_deposits_withdrawals_charges_and_computes_totals() {
        let rows = vec![
            json!({
                "transaction_date": "2026-05-05",
                "transaction_type": "deposit",
                "amount": "100.00",
                "currency_code": "IDR",
                "office_name": "Head Office",
                "product_name": "Basic Savings",
            }),
            json!({
                "transaction_date": "2026-05-05",
                "transaction_type": "withdrawal",
                "amount": "40.00",
                "currency_code": "IDR",
                "office_name": "Head Office",
                "product_name": "Basic Savings",
            }),
            json!({
                "transaction_date": "2026-05-07",
                "transaction_type": "withdrawal_fee",
                "amount": "1.50",
                "currency_code": "IDR",
                "office_name": "Head Office",
                "product_name": "Basic Savings",
            }),
        ];
        let out = render(&rows).expect("render");
        assert!(out.contains("Deposits (1 row(s), total: IDR 100"), "{out}");
        assert!(
            out.contains("Withdrawals (1 row(s), total: IDR 40"),
            "{out}"
        );
        assert!(
            out.contains("Charges paid (1 row(s), total: IDR 1.5"),
            "{out}"
        );
        assert!(out.contains("### Weekly aggregation"), "{out}");
        assert!(out.contains("### 2-day aggregation"), "{out}");
    }
}
