//! Guards a capability's prose against its approved SQL.
//!
//! The reranker decides coverage from a capability's `title` and `description`.
//! Those are prose; the rows a capability actually returns are decided by the
//! SQL underneath it. When the two disagree, the disagreement is invisible —
//! nothing executes, so nothing fails, and the user is told the catalog cannot
//! answer a question it can in fact answer.
//!
//! That is not hypothetical. `client_list_recent` was titled "Recently Activated
//! Clients" while its SQL restricted nothing by date — it selected every active
//! client in scope and merely sorted by `activation_date DESC`. Asked to "show
//! me all clients from <office>", the reranker read the title, concluded the
//! capability only returned recent clients, and answered `unsupported`. The one
//! capability that could have answered was rejected on its adjective.
//!
//! So: a word that promises the *rows are restricted* must be backed by a SQL
//! construct that restricts them. A word that only describes *sort order* is
//! fine, but must say so — the qualifier is what tells both a reader and the
//! reranker that no rows were withheld.

use anyhow::{Result, bail};

use crate::knowledge::model::{CapabilityKnowledge, QueryKnowledge};

/// Words that promise the result set is narrowed to the newest rows.
const RECENCY_WORDS: &[&str] = &[
    "recent",
    "recently",
    "latest",
    "newest",
    "terbaru",
    "paling baru",
];

/// Words that promise the result set is narrowed to the extreme rows.
const RANKING_WORDS: &[&str] = &[
    "top ",
    "highest",
    "largest",
    "biggest",
    "terbesar",
    "tertinggi",
];

/// Words that promise the rows are drawn at random.
const SAMPLING_WORDS: &[&str] = &["random", "sample", "acak"];

/// Phrases that demote a claim from "these rows are restricted" to "these rows
/// are ordered". Their presence is what makes a recency or ranking word honest
/// on a capability that returns everything in scope.
const ORDERING_QUALIFIERS: &[&str] = &[
    "order",
    "ordered",
    "sort",
    "sorted",
    "first",
    "urut",
    "diurutkan",
];

/// Fails when a capability's prose claims a narrowing its query cannot perform.
///
/// Checked per field rather than over the concatenation: the reranker weights
/// the title heavily and reads it on its own, so a title that overclaims is not
/// rescued by a description that quietly corrects it three lines later.
pub fn validate_prose_claims(
    capability: &CapabilityKnowledge,
    query: Option<&QueryKnowledge>,
    sql: &str,
) -> Result<()> {
    let fields: [(&str, Option<&str>); 2] = [
        ("display_name", capability.display_name.as_deref()),
        ("description", capability.description.as_deref()),
    ];

    for (field_name, value) in fields {
        let Some(text) = value.map(str::to_lowercase) else {
            continue;
        };
        if text.trim().is_empty() {
            continue;
        }
        let text = without_field_names(&text, query);
        let qualified = contains_any(&text, ORDERING_QUALIFIERS);

        if contains_any(&text, RECENCY_WORDS) && !qualified && !restricts_by_date(query) {
            bail!(
                "capability {} {field_name} claims recency but query {} restricts no date: \
                 declare a date parameter, or say the rows are only ordered \
                 (e.g. \"newest first\")",
                capability.id,
                capability.query_id,
            );
        }

        if contains_any(&text, RANKING_WORDS) && !qualified && !orders_descending(sql) {
            bail!(
                "capability {} {field_name} claims a ranking but query {} has no ORDER BY ... DESC",
                capability.id,
                capability.query_id,
            );
        }

        if contains_any(&text, SAMPLING_WORDS) && !samples_randomly(sql) {
            bail!(
                "capability {} {field_name} claims random sampling but query {} does not use random()",
                capability.id,
                capability.query_id,
            );
        }
    }

    Ok(())
}

fn contains_any(text: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| text.contains(needle))
}

/// Blanks out any run of words that spells one of the query's own field names.
///
/// `savings_account_activity_lookup` matches on a `latest_transaction_amount`
/// parameter and says so in prose. "Latest" there names a column — the newest
/// transaction *per account* — and claims nothing about which rows come back.
/// A word that identifies a field is not a promise about the result set, so it
/// must not be read as one.
fn without_field_names(text: &str, query: Option<&QueryKnowledge>) -> String {
    let Some(query) = query else {
        return text.to_string();
    };
    let mut stripped = text.to_string();
    let names = query
        .parameters
        .iter()
        .map(|parameter| parameter.name.as_str())
        .chain(query.output_fields.iter().map(|field| field.name.as_str()));
    for name in names {
        let humanized = name.replace('_', " ");
        if humanized.trim().is_empty() {
            continue;
        }
        stripped = stripped.replace(&humanized, " ");
        stripped = stripped.replace(name, " ");
    }
    stripped
}

/// A user-supplied date bound is the only thing that actually narrows rows to a
/// period. `authorized_scope` parameters are excluded for the same reason
/// `retrieval::sufficiency` excludes them: they are the authorization boundary,
/// bound on every request, and never a response to what the user asked for.
fn restricts_by_date(query: Option<&QueryKnowledge>) -> bool {
    query.is_some_and(|query| {
        query.parameters.iter().any(|parameter| {
            parameter.source.as_deref() != Some("authorized_scope")
                && (parameter.name.contains("date") || parameter.name.ends_with("_start"))
        })
    })
}

fn orders_descending(sql: &str) -> bool {
    let sql = sql.to_lowercase();
    sql.contains("order by") && sql.contains("desc")
}

fn samples_randomly(sql: &str) -> bool {
    sql.to_lowercase().contains("random()")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::knowledge::model::QueryParameter;

    fn capability(display_name: &str, description: &str) -> CapabilityKnowledge {
        CapabilityKnowledge {
            id: "test_capability".into(),
            status: "approved_mvp".into(),
            domain: "client".into(),
            query_id: "client.test".into(),
            output_mode: "list".into(),
            request_shape: Default::default(),
            display_name: Some(display_name.into()),
            description: Some(description.into()),
            data_areas: vec![],
            metrics: vec![],
            examples: vec![],
            supported_intents: Vec::new(),
            unsupported_intents: Vec::new(),
            continuation: false,
            required_parameters: vec![],
            optional_parameters: vec![],
            defaults: Default::default(),
            guards: Default::default(),
            dataset_recipe: None,
            parameter_policies: vec![],
        }
    }

    fn query(parameter_names: &[&str]) -> QueryKnowledge {
        QueryKnowledge {
            id: "client.test".into(),
            database: "fineract".into(),
            sql_file: "test.sql".into(),
            data_areas: vec![],
            tables: vec![],
            metrics: vec![],
            parameters: parameter_names
                .iter()
                .map(|name| QueryParameter {
                    name: (*name).into(),
                    kind: "string".into(),
                    required: false,
                    source: None,
                })
                .collect(),
            output_fields: vec![],
            timeout_ms: None,
        }
    }

    const SORTED_SQL: &str = "SELECT id FROM m_client ORDER BY activation_date DESC LIMIT $2;";

    /// The exact defect this module exists for.
    #[test]
    fn recency_title_over_a_query_that_restricts_no_date_is_rejected() {
        let capability = capability(
            "Recently Activated Clients",
            "List recently activated clients in the caller's authorized office scope.",
        );
        let error = validate_prose_claims(&capability, Some(&query(&["office_name"])), SORTED_SQL)
            .expect_err("recency claim with no date parameter must fail");
        assert!(
            error.to_string().contains("claims recency"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn recency_word_qualified_as_ordering_is_accepted() {
        let capability = capability(
            "Client List by Office",
            "List active clients in scope, newest activation first.",
        );
        validate_prose_claims(&capability, Some(&query(&["office_name"])), SORTED_SQL)
            .expect("an ordering qualifier makes the recency word honest");
    }

    #[test]
    fn recency_claim_backed_by_a_date_parameter_is_accepted() {
        let capability = capability("Recent Transactions", "Transactions in the chosen period.");
        validate_prose_claims(
            &capability,
            Some(&query(&["from_date", "to_date"])),
            SORTED_SQL,
        )
        .expect("a date parameter genuinely restricts rows");
    }

    #[test]
    fn ranking_claim_without_descending_order_is_rejected() {
        let capability = capability("Top Clients by Balance", "The largest savers.");
        let error = validate_prose_claims(
            &capability,
            Some(&query(&["office_name"])),
            "SELECT id FROM m_client;",
        )
        .expect_err("ranking claim with no ORDER BY DESC must fail");
        assert!(
            error.to_string().contains("claims a ranking"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn sampling_claim_without_random_is_rejected() {
        let capability = capability("Random Client Sample", "A random draw of clients.");
        let error = validate_prose_claims(&capability, Some(&query(&[])), SORTED_SQL)
            .expect_err("sampling claim with no random() must fail");
        assert!(
            error.to_string().contains("claims random sampling"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn sampling_claim_backed_by_random_is_accepted() {
        let capability = capability("Random Client Sample", "A random draw of clients.");
        validate_prose_claims(
            &capability,
            Some(&query(&[])),
            "SELECT id FROM m_client ORDER BY random() LIMIT $2;",
        )
        .expect("random() backs the claim");
    }
}
