use anyhow::{Result, bail};

use crate::assistant::legacy_pipeline::model::{ParsedIntent, ResolvedConstraints};

pub fn resolve_constraints(parsed: &ParsedIntent) -> Result<ResolvedConstraints> {
    if parsed.constraints.from_date.is_none() {
        bail!("from_date is required");
    }
    if parsed.constraints.to_date.is_none() {
        bail!("to_date is required");
    }

    Ok(ResolvedConstraints {
        from_date: parsed.constraints.from_date.clone(),
        to_date: parsed.constraints.to_date.clone(),
        quantity: parsed.constraints.quantity.clone(),
        currency_code: parsed.constraints.currency_code.clone(),
        product_ids: parsed.constraints.product_ids.clone(),
        office_scope: "authorized_scope".to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assistant::legacy_pipeline::model::{
        ParsedConstraints, ParsedIntent, ParsedIntentKind, QuantityConstraint,
    };

    fn parsed(quantity: QuantityConstraint) -> ParsedIntent {
        ParsedIntent {
            intent: ParsedIntentKind::Report,
            domain: Some("savings".to_string()),
            entities: Vec::new(),
            constraints: ParsedConstraints {
                from_date: Some("2026-07-01".to_string()),
                to_date: Some("2026-07-07".to_string()),
                quantity: Some(quantity),
                currency_code: None,
                product_ids: None,
            },
            requires_retrieval: true,
            confidence: 0.9,
        }
    }

    #[test]
    fn resolves_all_without_limit_value() {
        let resolved = resolve_constraints(&parsed(QuantityConstraint::All)).unwrap();
        assert_eq!(resolved.quantity, Some(QuantityConstraint::All));
        assert_eq!(resolved.office_scope, "authorized_scope");
    }

    #[test]
    fn rejects_missing_date_range() {
        let mut input = parsed(QuantityConstraint::Default);
        input.constraints.from_date = None;
        let error = resolve_constraints(&input).unwrap_err();
        assert!(error.to_string().contains("from_date is required"));
    }
}
