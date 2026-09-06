//! Grammar for declared SQL expressions.
//!
//! `filters[].expr` and `order_by[].expr` are concatenated into executable SQL.
//! They are authored, not user-supplied, but concatenation makes them a trust
//! boundary regardless: this module is what stops a mistyped or malicious
//! declaration from extending the statement.

/// Accepts a comma-separated list of ordering terms. Each term is a bare or
/// table-qualified identifier, optionally `ASC`/`DESC`, optionally
/// `NULLS FIRST`/`NULLS LAST`. Everything else is rejected.
pub fn validate_sql_expr(expr: &str) -> Result<(), String> {
    if expr.trim().is_empty() {
        return Err("expression is empty".into());
    }
    for forbidden in [";", "--", "/*", "*/", "'", "\"", "(", ")"] {
        if expr.contains(forbidden) {
            return Err(format!("expression contains forbidden token `{forbidden}`"));
        }
    }
    for term in expr.split(',') {
        validate_term(term)?;
    }
    Ok(())
}

fn validate_term(term: &str) -> Result<(), String> {
    let mut words = term.split_whitespace();
    let Some(identifier) = words.next() else {
        return Err("expression has an empty term".into());
    };
    validate_identifier(identifier)?;

    let mut rest: Vec<&str> = words.collect();
    if matches!(
        rest.first()
            .map(|word| word.to_ascii_uppercase())
            .as_deref(),
        Some("ASC") | Some("DESC")
    ) {
        rest.remove(0);
    }
    match rest.len() {
        0 => Ok(()),
        2 if rest[0].eq_ignore_ascii_case("NULLS")
            && (rest[1].eq_ignore_ascii_case("FIRST") || rest[1].eq_ignore_ascii_case("LAST")) =>
        {
            Ok(())
        }
        _ => Err(format!("unexpected tokens in term `{}`", term.trim())),
    }
}

fn validate_identifier(identifier: &str) -> Result<(), String> {
    let parts: Vec<&str> = identifier.split('.').collect();
    if parts.len() > 2 {
        return Err(format!("identifier `{identifier}` has too many qualifiers"));
    }
    for part in parts {
        let mut chars = part.chars();
        let valid_start = chars
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_');
        if !valid_start || !chars.all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return Err(format!("invalid identifier `{identifier}`"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_declared_expression_forms() {
        for expr in [
            "sac.charge_due_date",
            "amount",
            "sac.created_on_utc DESC",
            "sac.created_on_utc DESC, sac.id DESC",
            "sac.charge_due_date ASC NULLS LAST",
            "sac.charge_due_date ASC NULLS LAST, sac.id DESC",
        ] {
            assert!(validate_sql_expr(expr).is_ok(), "should accept: {expr}");
        }
    }

    #[test]
    fn rejects_anything_that_could_extend_the_statement() {
        for expr in [
            "sac.id; DROP TABLE m_client",
            "sac.id -- comment",
            "sac.id /* comment */",
            "(SELECT 1)",
            "count(sac.id)",
            "sac.id DESC UNION SELECT 1",
            "sac.id'",
            "sac.id\nDROP",
            "a.b.c",
            "",
            "   ",
            ",",
            "sac.id,,sac.name",
            "sac.id SIDEWAYS",
            "sac.id DESC NULLS SOMEWHERE",
            "1",
            "sac.1id",
        ] {
            assert!(validate_sql_expr(expr).is_err(), "should reject: {expr}");
        }
    }
}
