use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LayeredRetrievalPlan {
    pub domain: String,
    pub capability: String,
    pub query: String,
    pub keyword: String,
    #[serde(default)]
    pub graph_hint: Option<String>,
    pub confidence: f32,
}

#[derive(Debug, Deserialize)]
struct RawLayeredResponse {
    layers: RawLayers,
    confidence: f32,
}

#[derive(Debug, Deserialize)]
struct RawLayers {
    domain: String,
    capability: String,
    query: String,
    keyword: String,
    #[serde(default)]
    graph_hint: Option<String>,
}

pub fn parse_layered_retrieval_response(content: &str) -> anyhow::Result<LayeredRetrievalPlan> {
    let raw: RawLayeredResponse = serde_json::from_str(content)?;
    if !(0.0..=1.0).contains(&raw.confidence) {
        anyhow::bail!("layered retrieval plan confidence must be in [0,1]");
    }
    for (name, value) in [
        ("domain", &raw.layers.domain),
        ("capability", &raw.layers.capability),
        ("query", &raw.layers.query),
        ("keyword", &raw.layers.keyword),
    ] {
        if value.trim().is_empty() {
            anyhow::bail!("layered retrieval plan field {name} must be non-empty");
        }
    }

    Ok(LayeredRetrievalPlan {
        domain: raw.layers.domain,
        capability: raw.layers.capability,
        query: raw.layers.query,
        keyword: raw.layers.keyword,
        graph_hint: raw.layers.graph_hint,
        confidence: raw.confidence,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid_layered_plan_json() {
        let json = r#"{
          "layers": {
            "domain":"client activation",
            "capability":"monthly count of client activations",
            "query":"monthly_breakdown output for client onboarding",
            "keyword":"client activation monthly",
            "graph_hint":"client -> activation_date"
          },
          "confidence":0.83
        }"#;

        let plan = parse_layered_retrieval_response(json).expect("parse");

        assert_eq!(plan.domain, "client activation");
        assert_eq!(plan.capability, "monthly count of client activations");
        assert_eq!(
            plan.graph_hint.as_deref(),
            Some("client -> activation_date")
        );
        assert!((plan.confidence - 0.83).abs() < 1e-4);
    }

    #[test]
    fn rejects_missing_layer_field() {
        let json = r#"{"layers":{"domain":"x","capability":"y","query":"z"},"confidence":0.7}"#;

        assert!(parse_layered_retrieval_response(json).is_err());
    }

    #[test]
    fn rejects_out_of_range_confidence() {
        let json = r#"{"layers":{"domain":"x","capability":"y","query":"z","keyword":"k"},"confidence":1.5}"#;

        assert!(parse_layered_retrieval_response(json).is_err());
    }
}
