use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GeneratedAnswer {
    pub message: String,
    #[serde(default)]
    pub citations: Vec<String>,
}

pub fn parse_generated_answer(content: &str) -> Result<GeneratedAnswer> {
    serde_json::from_str(content).map_err(anyhow::Error::from)
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
    if citation == "answer_plan.coverage" {
        return structured.pointer("/answer_plan/coverage").is_some();
    }
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
    false
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
}
