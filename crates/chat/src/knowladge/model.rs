use std::path::PathBuf;

use serde::Deserialize;

#[derive(Debug, Clone)]
pub struct KnowledgeCatalog {
    pub root_path: PathBuf,
    pub query_path: PathBuf,
    pub data_areas: Vec<DataAreasKnowledge>,
    pub domains: Vec<DomainKnowledge>,
    pub capabilities: Vec<CapabilityKnowledge>,
    pub queries: Vec<QueryKnowledge>,
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
    pub data_areas: Vec<String>,

    #[serde(default)]
    pub supported_intents: Vec<String>,

    #[serde(default)]
    pub unsupported_intents: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CapabilityKnowledge {
    pub id: String,
    pub status: String,
    pub domain: String,
    pub query_id: String,

    #[serde(default)]
    pub data_areas: Vec<String>,

    #[serde(default)]
    pub metrics: Vec<String>,

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
