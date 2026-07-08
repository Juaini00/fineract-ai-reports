use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GeneratedAnswer {
    pub message: String,
    #[serde(default)]
    pub citations: Vec<String>,
}

pub fn parse_generated_answer(content: &str) -> Result<GeneratedAnswer> {
    serde_json::from_str(content).with_context(|| {
        let tail_start = content.len().saturating_sub(120);
        format!(
            "invalid generated answer JSON from LLM (content_len={}) head={:?} tail={:?}",
            content.len(),
            content.chars().take(120).collect::<String>(),
            &content[tail_start..],
        )
    })
}

pub fn validate_grounded_answer(structured: &Value, answer: &GeneratedAnswer) -> Result<()> {
    if answer.message.trim().is_empty() {
        bail!("generated answer message is empty");
    }
    for citation in &answer.citations {
        if !citation_exists(structured, citation) {
            bail!("generated answer citation does not exist: {citation}");
        }
    }
    Ok(())
}

fn citation_exists(structured: &Value, citation: &str) -> bool {
    if let Some(index) = citation
        .strip_prefix("structured.rows[")
        .and_then(|rest| rest.strip_suffix(']'))
        .and_then(|value| value.parse::<usize>().ok())
    {
        return structured
            .pointer("/structured/rows")
            .and_then(Value::as_array)
            .is_some_and(|rows| index < rows.len());
    }
    let pointer: String = citation
        .split('.')
        .map(|segment| format!("/{segment}"))
        .collect();
    structured.pointer(&pointer).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_citations_to_existing_result_paths() {
        let structured = serde_json::json!({
            "answer_plan": { "coverage": { "returned_rows": 1 } },
            "structured": { "rows": [{ "transaction_id": 1 }] }
        });
        let answer = GeneratedAnswer {
            message: "One transaction.".to_string(),
            citations: vec![
                "structured.rows[0]".to_string(),
                "answer_plan.coverage".to_string(),
            ],
        };
        validate_grounded_answer(&structured, &answer).unwrap();
    }

    #[test]
    fn rejects_missing_row_citation() {
        let structured = serde_json::json!({ "structured": { "rows": [] } });
        let answer = GeneratedAnswer {
            message: "Missing.".to_string(),
            citations: vec!["structured.rows[0]".to_string()],
        };
        assert!(validate_grounded_answer(&structured, &answer).is_err());
    }

    #[test]
    fn parse_error_includes_content_length_hint() {
        let error = parse_generated_answer("{\"message\":\"unfinished").unwrap_err();

        assert!(error.to_string().contains("invalid generated answer JSON"));
        assert!(error.to_string().contains("content_len=22"));
    }
}
