//! Gateway prompt construction. Produces a deterministic Markdown prompt for
//! Layer 1 (LLM) that carries only safe surfaces: user message, recent turns,
//! and a per-capability summary drawn from the visible catalogue. Never leaks
//! SQL, parameter internals, or PII policy (spec §4.2).

use crate::knowledge::model::CapabilityKnowledge;

/// Safe per-capability projection for the gateway prompt. Built from
/// `CapabilityKnowledge` by `capability_summary`; the type is deliberately
/// small so the prompt never touches parameter policies or SQL.
pub struct CapabilitySummary<'a> {
    pub id: &'a str,
    pub display_name: Option<&'a str>,
    pub description: Option<&'a str>,
    pub use_when: Option<String>,
}

/// Project a `CapabilityKnowledge` to the safe prompt view. `use_when` falls
/// back to the first example when no explicit hint is present in YAML.
pub fn capability_summary(capability: &CapabilityKnowledge) -> CapabilitySummary<'_> {
    CapabilitySummary {
        id: capability.id.as_str(),
        display_name: capability.display_name.as_deref(),
        description: capability.description.as_deref(),
        use_when: capability.examples.first().cloned(),
    }
}

pub fn build_gateway_prompt(
    user_message: &str,
    catalog_summary: &[CapabilitySummary<'_>],
    history_summary: Option<&str>,
) -> String {
    let mut out = String::new();
    out.push_str("# User message\n");
    out.push_str(user_message.trim());
    out.push_str("\n\n");
    if let Some(history) = history_summary
        && !history.trim().is_empty()
    {
        out.push_str("# Recent turns\n");
        out.push_str(history.trim());
        out.push_str("\n\n");
    }
    out.push_str("# Visible capabilities\n");
    for summary in catalog_summary {
        out.push_str("- `");
        out.push_str(summary.id);
        out.push('`');
        if let Some(name) = summary.display_name {
            out.push_str(" — ");
            out.push_str(name);
        }
        out.push('\n');
        if let Some(description) = summary.description {
            out.push_str("  ");
            out.push_str(description);
            out.push('\n');
        }
        if let Some(use_when) = summary.use_when.as_deref() {
            out.push_str("  use when: ");
            out.push_str(use_when);
            out.push('\n');
        }
    }
    out.push_str(
        "\n# Output\n\
         Return a single JSON object matching the `LlmGatewayExtraction` schema. \
         Never emit an absolute date the user did not type verbatim. Never emit a \
         `capability_id` outside the list above. Every `entities.value` must appear \
         verbatim in the user message.\n",
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn summary<'a>(
        id: &'a str,
        display_name: Option<&'a str>,
        description: Option<&'a str>,
        use_when: Option<&'a str>,
    ) -> CapabilitySummary<'a> {
        CapabilitySummary {
            id,
            display_name,
            description,
            use_when: use_when.map(str::to_string),
        }
    }

    #[test]
    fn prompt_carries_capability_ids_and_never_leaks_sql_or_parameter_internals() {
        let cats = vec![
            summary(
                "savings_deposit_top_n",
                Some("Top deposits"),
                Some("Rank savings deposits by amount."),
                Some("Top 10 deposits this month"),
            ),
            summary("client_lifecycle_summary", None, None, None),
        ];
        let prompt = build_gateway_prompt(
            "Top 10 deposits this month",
            &cats,
            Some("previous turn: user asked about deposits"),
        );
        assert!(prompt.contains("savings_deposit_top_n"));
        assert!(prompt.contains("client_lifecycle_summary"));
        assert!(prompt.contains("previous turn"));
        let upper = prompt.to_ascii_uppercase();
        for banned in ["SELECT ", "FROM ", "WHERE ", "JOIN "] {
            assert!(
                !upper.contains(banned),
                "prompt leaked SQL keyword {banned}"
            );
        }
        for banned in [
            "parameter_policies",
            "hard_cap",
            "sensitivity",
            "office_ids",
            "authorized_scope",
        ] {
            assert!(
                !prompt.contains(banned),
                "prompt leaked parameter/policy internal `{banned}`"
            );
        }
    }
}
