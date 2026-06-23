use serde::Serialize;
use serde_json::json;

use crate::knowledge::model::{
    CapabilityKnowledge, DataAreasKnowledge, DomainKnowledge, KnowledgeCatalog, QueryKnowledge,
};

#[derive(Debug, Clone, Serialize)]
pub struct RetrievalDocument {
    pub source_type: RetrievalSourceType,
    pub source_id: String,
    pub title: String,
    pub retrieval_text: String,
    pub metadata_json: serde_json::Value,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RetrievalSourceType {
    DataArea,
    Domain,
    Capability,
    Query,
}

pub struct RetrievalDocumentBuilder;

impl RetrievalDocumentBuilder {
    pub fn build(catalog: &KnowledgeCatalog) -> Vec<RetrievalDocument> {
        let mut documents = Vec::new();

        documents.extend(catalog.data_areas.iter().map(build_data_area_document));
        documents.extend(catalog.domains.iter().map(build_domain_document));
        documents.extend(catalog.capabilities.iter().map(build_capability_document));
        documents.extend(catalog.queries.iter().map(build_query_document));

        documents
    }
}

fn build_data_area_document(area: &DataAreasKnowledge) -> RetrievalDocument {
    let title = format!("Data area {}", area.id);
    let retrieval_text = compact_lines([
        format!("Data area {}.", area.id),
        format!("Status {}.", area.status),
        optional_list("Included tables", &area.included_tables),
        optional_list("Conditional tables", &area.conditional_tables),
        optional_list("Excluded tables", &area.excluded_tables),
        optional_list("Allowed domains", &area.allowed_domains),
    ]);

    RetrievalDocument {
        source_type: RetrievalSourceType::DataArea,
        source_id: area.id.clone(),
        title,
        retrieval_text,
        metadata_json: json!({
            "status": area.status,
            "included_tables": area.included_tables,
            "conditional_tables": area.conditional_tables,
            "excluded_tables": area.excluded_tables,
            "allowed_domains": area.allowed_domains,
        }),
    }
}

fn build_domain_document(domain: &DomainKnowledge) -> RetrievalDocument {
    let title = format!("Domain {}", domain.id);
    let retrieval_text = compact_lines([
        format!("Domain {}.", domain.id),
        format!("Status {}.", domain.status),
        optional_list("Data areas", &domain.data_areas),
        optional_list("Supported intents", &domain.supported_intents),
        optional_list("Unsupported intents", &domain.unsupported_intents),
    ]);

    RetrievalDocument {
        source_type: RetrievalSourceType::Domain,
        source_id: domain.id.clone(),
        title,
        retrieval_text,
        metadata_json: json!({
            "status": domain.status,
            "data_areas": domain.data_areas,
            "supported_intents": domain.supported_intents,
            "unsupported_intents": domain.unsupported_intents,
        }),
    }
}

fn build_capability_document(capability: &CapabilityKnowledge) -> RetrievalDocument {
    let title = format!("Capability {}", capability.id);
    let retrieval_text = compact_lines([
        format!("Capability {}", capability.id),
        format!("Status {}", capability.status),
        format!("Domain {}", capability.domain),
        format!("Query {}", capability.query_id),
        optional_list("Data areas", &capability.data_areas),
        optional_list("Metrics", &capability.metrics),
        optional_list("Examples", &capability.examples),
        optional_list("Required parameters", &capability.required_parameters),
        optional_list("Optional parameters", &capability.optional_parameters),
    ]);

    RetrievalDocument {
        source_type: RetrievalSourceType::Capability,
        source_id: capability.id.clone(),
        title,
        retrieval_text,
        metadata_json: json!({
            "status": capability.status,
            "domain": capability.domain,
            "query_id": capability.query_id,
            "output_mode": capability.output_mode,
            "data_areas": capability.data_areas,
            "metrics": capability.metrics,
            "examples": capability.examples,
            "required_parameters": capability.required_parameters,
            "optional_parameters": capability.optional_parameters,
        }),
    }
}

fn build_query_document(query: &QueryKnowledge) -> RetrievalDocument {
    let title = format!("Query {}", query.id);
    let parameter_names = query
        .parameters
        .iter()
        .map(|param| param.name.clone())
        .collect::<Vec<_>>();
    let output_field_names = query
        .output_fields
        .iter()
        .map(|field| field.name.clone())
        .collect::<Vec<_>>();

    let retrieval_text = compact_lines([
        format!("Query {}", query.id),
        format!("Database: {}", query.database),
        format!("SQL file {}", query.sql_file),
        optional_list("Data areas", &query.data_areas),
        optional_list("Tables", &query.tables),
        optional_list("Metrics", &query.metrics),
        optional_list("Parameters", &parameter_names),
        optional_list("Output fields", &output_field_names),
    ]);

    RetrievalDocument {
        source_type: RetrievalSourceType::Query,
        source_id: query.id.clone(),
        title,
        retrieval_text,
        metadata_json: json!({
            "database": query.database,
            "sql_file": query.sql_file,
            "data_areas": query.data_areas,
            "tables": query.tables,
            "metrics": query.metrics,
            "parameters": parameter_names,
            "output_fields": output_field_names,
        }),
    }
}

fn optional_list(label: &str, values: &[String]) -> String {
    if values.is_empty() {
        String::new()
    } else {
        format!("{label}: {}.", values.join(", "))
    }
}

fn compact_lines(lines: impl IntoIterator<Item = String>) -> String {
    lines
        .into_iter()
        .filter(|line| !line.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}
