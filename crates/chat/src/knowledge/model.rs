use std::path::PathBuf;

use serde::Deserialize;

#[derive(Debug, Clone)]
pub struct KnowledgeCatalog {
    pub root_path: PathBuf,
    pub query_path: PathBuf,
    pub data_areas: Vec<DataAreasKnowledge>,
    pub domains: Vec<DomainKnowledge>,
    pub schemas: Vec<GenericKnowledge>,
    pub metrics: Vec<GenericKnowledge>,
    pub capabilities: Vec<CapabilityKnowledge>,
    pub queries: Vec<QueryKnowledge>,
    pub policies: Vec<GenericKnowledge>,
    pub responses: Vec<GenericKnowledge>,
    pub classification: ClassificationPolicy,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ClassificationPolicy {
    pub min_gap: f32,
    pub min_floor: f32,
    pub others_key: String,
    pub others_label: String,
}

impl Default for ClassificationPolicy {
    fn default() -> Self {
        Self {
            min_gap: 0.05,
            min_floor: 0.40,
            others_key: "other_activity".to_string(),
            others_label: "Others — let me describe it in my own words".to_string(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct GenericKnowledge {
    pub id: String,

    #[serde(default)]
    pub status: Option<String>,

    #[serde(default)]
    pub domain: Option<String>,

    #[serde(default)]
    pub data_areas: Vec<String>,

    #[serde(default)]
    pub checks: Vec<serde_json::Value>,

    #[serde(flatten)]
    pub content: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DataAreasKnowledge {
    pub id: String,
    pub status: String,

    #[serde(default)]
    pub included_tables: Vec<String>,

    #[serde(default)]
    pub conditional_tables: Vec<String>,

    #[serde(default)]
    pub excluded_tables: Vec<String>,

    #[serde(default)]
    pub allowed_domains: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DomainKnowledge {
    pub id: String,
    pub status: String,

    #[serde(default)]
    pub display_name: Option<String>,

    #[serde(default)]
    pub description: Option<String>,

    #[serde(default)]
    pub data_areas: Vec<String>,

    #[serde(default)]
    pub concepts: Vec<DomainConcept>,

    #[serde(default)]
    pub supported_intents: Vec<String>,

    #[serde(default)]
    pub unsupported_intents: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DomainConcept {
    pub id: String,

    #[serde(default)]
    pub meaning: Option<String>,

    #[serde(default)]
    pub synonyms: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CapabilityKnowledge {
    pub id: String,
    pub status: String,
    pub domain: String,
    pub query_id: String,
    pub output_mode: String,

    #[serde(default)]
    pub display_name: Option<String>,

    #[serde(default)]
    pub description: Option<String>,

    #[serde(default)]
    pub data_areas: Vec<String>,

    #[serde(default)]
    pub metrics: Vec<String>,

    #[serde(default)]
    pub examples: Vec<String>,

    #[serde(default)]
    pub required_parameters: Vec<String>,

    #[serde(default)]
    pub optional_parameters: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct QueryKnowledge {
    pub id: String,
    pub database: String,
    pub sql_file: String,

    #[serde(default)]
    pub data_areas: Vec<String>,

    #[serde(default)]
    pub tables: Vec<String>,

    #[serde(default)]
    pub metrics: Vec<String>,

    #[serde(default)]
    pub parameters: Vec<QueryParameter>,

    #[serde(default)]
    pub output_fields: Vec<QueryOutputField>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct QueryParameter {
    pub name: String,

    #[serde(rename = "type")]
    pub kind: String,

    pub required: bool,

    #[serde(default)]
    pub source: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct QueryOutputField {
    pub name: String,

    #[serde(rename = "type")]
    pub kind: String,

    pub sensitivity: String,
}
