use serde::Serialize;
use serde_json::json;

use crate::assistant::clarification::humanize_id;
use crate::knowledge::model::{
    CapabilityKnowledge, DataAreasKnowledge, DomainKnowledge, GenericKnowledge, KnowledgeCatalog,
    QueryKnowledge,
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
    Schema,
    Metric,
    Capability,
    Query,
    Policy,
    Response,
}

pub struct RetrievalDocumentBuilder;

impl RetrievalDocumentBuilder {
    pub fn build(catalog: &KnowledgeCatalog) -> Vec<RetrievalDocument> {
        let mut documents = Vec::new();

        documents.extend(catalog.data_areas.iter().map(build_data_area_document));
        documents.extend(catalog.domains.iter().map(build_domain_document));
        documents.extend(
            catalog
                .schemas
                .iter()
                .map(|item| build_generic_document(RetrievalSourceType::Schema, "Schema", item)),
        );
        documents.extend(
            catalog
                .metrics
                .iter()
                .map(|item| build_generic_document(RetrievalSourceType::Metric, "Metric", item)),
        );
        documents.extend(catalog.capabilities.iter().map(|capability| {
            let domain = catalog.domains.iter().find(|d| d.id == capability.domain);
            build_capability_document(capability, domain)
        }));
        documents.extend(catalog.queries.iter().map(build_query_document));
        documents.extend(
            catalog
                .policies
                .iter()
                .map(|item| build_generic_document(RetrievalSourceType::Policy, "Policy", item)),
        );
        documents.extend(
            catalog.responses.iter().map(|item| {
                build_generic_document(RetrievalSourceType::Response, "Response", item)
            }),
        );

        documents
    }
}

fn build_generic_document(
    source_type: RetrievalSourceType,
    label: &str,
    item: &GenericKnowledge,
) -> RetrievalDocument {
    let title = format!("{label} {}", item.id);
    let content = item
        .content
        .iter()
        .map(|(key, value)| format!("{key}: {value}"))
        .collect::<Vec<_>>();
    let retrieval_text = compact_lines([
        title.clone(),
        optional_value("Status", item.status.as_deref()),
        optional_value("Domain", item.domain.as_deref()),
        optional_list("Data areas", &item.data_areas),
        optional_list("Content", &content),
    ]);

    RetrievalDocument {
        source_type,
        source_id: item.id.clone(),
        title,
        retrieval_text,
        metadata_json: json!({
            "status": item.status,
            "domain": item.domain,
            "data_areas": item.data_areas,
        }),
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
    let display_name_line = domain
        .display_name
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .map(|value| format!("Display name: {value}."))
        .unwrap_or_default();
    let description_line = domain
        .description
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .map(|value| format!("Description: {value}."))
        .unwrap_or_default();
    let concept_line = concept_line(&domain.concepts);
    let retrieval_text = compact_lines([
        format!("Domain {}.", domain.id),
        display_name_line,
        description_line,
        format!("Status {}.", domain.status),
        optional_list("Data areas", &domain.data_areas),
        concept_line,
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
            "display_name": domain.display_name,
            "data_areas": domain.data_areas,
            "supported_intents": domain.supported_intents,
            "unsupported_intents": domain.unsupported_intents,
        }),
    }
}

fn concept_line(concepts: &[crate::knowledge::model::DomainConcept]) -> String {
    if concepts.is_empty() {
        return String::new();
    }
    let mut parts = Vec::new();
    for concept in concepts {
        parts.push(concept.id.clone());
        if let Some(meaning) = concept.meaning.as_deref()
            && !meaning.trim().is_empty()
        {
            parts.push(meaning.to_string());
        }
        parts.extend(concept.synonyms.iter().cloned());
    }
    format!("Concepts: {}.", parts.join(", "))
}

fn build_capability_document(
    capability: &CapabilityKnowledge,
    domain: Option<&DomainKnowledge>,
) -> RetrievalDocument {
    let title = capability
        .display_name
        .clone()
        .unwrap_or_else(|| humanize_id(&capability.id));
    let concept_synonyms: Vec<String> = domain
        .map(|d| {
            d.concepts
                .iter()
                .flat_map(|c| c.synonyms.iter().cloned())
                .collect()
        })
        .unwrap_or_default();

    let retrieval_text = compact_lines([
        format!("Capability {}", capability.id),
        format!(
            "Display name {}",
            capability.display_name.as_deref().unwrap_or(&capability.id)
        ),
        capability.description.clone().unwrap_or_default(),
        format!("Status {}", capability.status),
        format!("Domain {}", capability.domain),
        format!("Query {}", capability.query_id),
        optional_list("Data areas", &capability.data_areas),
        optional_list("Metrics", &capability.metrics),
        optional_list("Examples", &capability.examples),
        optional_list("Domain concepts", &concept_synonyms),
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
            "display_name": capability.display_name,
            "description": capability.description,
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

fn optional_value(label: &str, value: Option<&str>) -> String {
    value
        .filter(|value| !value.trim().is_empty())
        .map(|value| format!("{label}: {value}."))
        .unwrap_or_default()
}

fn compact_lines(lines: impl IntoIterator<Item = String>) -> String {
    lines
        .into_iter()
        .filter(|line| !line.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn capability_with(id: &str, display_name: Option<&str>) -> CapabilityKnowledge {
        CapabilityKnowledge {
            id: id.to_string(),
            status: "active".to_string(),
            domain: "test_domain".to_string(),
            query_id: "test_query".to_string(),
            output_mode: "table".to_string(),
            request_shape: Default::default(),
            display_name: display_name.map(str::to_string),
            description: None,
            data_areas: Vec::new(),
            metrics: Vec::new(),
            examples: Vec::new(),
            required_parameters: Vec::new(),
            optional_parameters: Vec::new(),
        }
    }

    #[test]
    fn build_capability_document_humanizes_id_when_display_name_missing() {
        let capability = capability_with("client_lifecycle_summary", None);

        let document = build_capability_document(&capability, None);

        assert_eq!(document.title, "Client Lifecycle Summary");
    }

    #[test]
    fn build_capability_document_uses_display_name_when_present() {
        let capability = capability_with("client_lifecycle_summary", Some("Client Lifecycle"));

        let document = build_capability_document(&capability, None);

        assert_eq!(document.title, "Client Lifecycle");
    }
}
