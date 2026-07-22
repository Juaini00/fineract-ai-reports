use serde_json::Value;

use crate::assistant::response::{AssistantResponse, ResponseTable};

pub trait ResponseRenderer {
    fn render(&self, response: &AssistantResponse) -> String;
}

pub struct MarkdownRenderer;

impl ResponseRenderer for MarkdownRenderer {
    fn render(&self, response: &AssistantResponse) -> String {
        let mut out = String::new();
        if let Some(title) = &response.title {
            out.push_str("# ");
            out.push_str(title);
            out.push_str("\n\n");
        }
        out.push_str(&response.message);
        for section in &response.sections {
            out.push_str("\n\n## ");
            out.push_str(&section.title);
            out.push('\n');
            out.push_str(&section.body);
        }
        if let Some(table) = &response.table {
            render_table(&mut out, table);
        }
        if !response.cards.is_empty() {
            out.push_str("\n\n## Metrics");
            for card in &response.cards {
                out.push_str("\n- **");
                out.push_str(&card.label);
                out.push_str("**: ");
                out.push_str(&card.value);
                if let Some(unit) = &card.unit {
                    out.push(' ');
                    out.push_str(unit);
                }
            }
        }
        if let Some(clarification) = &response.clarification {
            out.push_str("\n\n## Question\n");
            out.push_str(&clarification.question);
            if !clarification.options.is_empty() {
                out.push_str("\n\n## Options");
                for option in &clarification.options {
                    out.push_str("\n- ");
                    out.push_str(display_label(&option.label, &option.id));
                    if let Some(description) = option
                        .description
                        .as_deref()
                        .filter(|text| !text.is_empty())
                    {
                        out.push_str(": ");
                        out.push_str(description);
                    }
                    render_fields(&mut out, &option.fields, "Details for this option");
                }
            }
            render_fields(&mut out, &clarification.fields, "Required details");
        } else if !response.options.is_empty() {
            // Legacy responses have only the deprecated top-level projection.
            out.push_str("\n\n## Options");
            for option in &response.options {
                out.push_str("\n- ");
                out.push_str(&option.label);
                if let Some(description) = &option.description {
                    out.push_str(": ");
                    out.push_str(description);
                }
            }
        }
        if !response.warnings.is_empty() {
            out.push_str("\n\n## Warnings");
            for warning in &response.warnings {
                out.push_str("\n- ");
                out.push_str(&warning.message);
            }
        }
        if !response.actions.is_empty() {
            out.push_str("\n\n## Actions");
            for action in &response.actions {
                out.push_str("\n- ");
                out.push_str(&action.label);
            }
        }
        out
    }
}

fn render_fields(
    out: &mut String,
    fields: &[crate::assistant::clarification::ClarificationField],
    heading: &str,
) {
    if fields.is_empty() {
        return;
    }
    out.push_str("\n\n## ");
    out.push_str(heading);
    for field in fields {
        out.push_str("\n- ");
        out.push_str(display_label(&field.label, &field.key));
        if field.required {
            out.push_str(" (required)");
        }
        if let Some(help_text) = field.help_text.as_deref().filter(|text| !text.is_empty()) {
            out.push_str(": ");
            out.push_str(help_text);
        }
        for error in &field.errors {
            if !error.is_empty() {
                out.push_str("\n  - Error: ");
                out.push_str(error);
            }
        }
    }
}

fn display_label<'a>(label: &'a str, fallback: &'a str) -> &'a str {
    if label.trim().is_empty() {
        fallback
    } else {
        label
    }
}

fn render_table(out: &mut String, table: &ResponseTable) {
    let visible: Vec<_> = table
        .columns
        .iter()
        .filter(|column| !column.hidden)
        .collect();
    if visible.is_empty() {
        return;
    }
    out.push_str("\n\n");
    out.push('|');
    for column in &visible {
        out.push_str(&column.label);
        out.push('|');
    }
    out.push_str("\n|");
    for _ in &visible {
        out.push_str("---|");
    }
    for row in &table.rows {
        out.push_str("\n|");
        for column in &visible {
            out.push_str(&cell(row, &column.key));
            out.push('|');
        }
    }
}

fn cell(row: &Value, key: &str) -> String {
    match row.get(key) {
        Some(Value::String(value)) => value.clone(),
        Some(Value::Number(value)) => value.to_string(),
        Some(Value::Bool(value)) => value.to_string(),
        Some(Value::Null) | None => String::new(),
        Some(other) => other.to_string(),
    }
}
