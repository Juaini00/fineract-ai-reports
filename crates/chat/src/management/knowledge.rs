use std::sync::Arc;

use crate::api::dto::management::{
    ExecutionMode, KnowledgeDetailResponse, KnowledgeItemResponse, KnowledgeKind, KnowledgeQuery,
    KnowledgeStatus, ParameterResponse,
};
use crate::knowledge::model::{CapabilityKnowledge, KnowledgeCatalog};

/// Read boundary for the safe, in-memory catalog projection. This deliberately
/// does not expose query SQL or generic catalog documents.
#[derive(Clone)]
pub struct CatalogKnowledgeRepository {
    catalog: Arc<KnowledgeCatalog>,
}

impl CatalogKnowledgeRepository {
    pub fn new(catalog: Arc<KnowledgeCatalog>) -> Self {
        Self { catalog }
    }

    fn capabilities(&self) -> impl Iterator<Item = &CapabilityKnowledge> {
        self.catalog.capabilities.iter()
    }

    fn capability(&self, id: &str) -> Option<&CapabilityKnowledge> {
        self.catalog
            .capabilities
            .iter()
            .find(|capability| capability.id == id)
    }

    fn catalog(&self) -> &KnowledgeCatalog {
        &self.catalog
    }
}

#[derive(Clone)]
pub struct KnowledgeService {
    repository: CatalogKnowledgeRepository,
}

impl KnowledgeService {
    pub fn new(catalog: Arc<KnowledgeCatalog>) -> Self {
        Self {
            repository: CatalogKnowledgeRepository::new(catalog),
        }
    }

    pub fn list(&self, query: &KnowledgeQuery) -> Result<KnowledgeList, KnowledgeLookupError> {
        if matches!(query.kind, Some(KnowledgeKind::Reference)) {
            return Ok(self.disabled());
        }

        let mut items: Vec<_> = self
            .repository
            .capabilities()
            .map(inventory_item)
            .filter(|item| matches_filter(item, query))
            .collect();
        items.sort_by(|left, right| left.id.cmp(&right.id));

        let start = match query.cursor.as_ref() {
            Some(cursor) => items
                .iter()
                .position(|item| item.id == cursor.as_str())
                .map(|index| index + 1)
                .ok_or(KnowledgeLookupError::InvalidCursor)?,
            None => 0,
        };
        let limit = query.limit.unwrap_or(50) as usize;
        let next_cursor = items
            .get(start + limit)
            .map(|_| items[start + limit - 1].id.clone());
        let items = items.into_iter().skip(start).take(limit).collect();

        Ok(KnowledgeList {
            items,
            next_cursor,
            catalog_version: catalog_version(self.repository.catalog()),
            index_version: None,
            reference_knowledge_status: None,
        })
    }

    pub fn detail(&self, public_id: &str) -> Option<KnowledgeDetailResponse> {
        let id = public_id.strip_prefix("catalog:")?;
        let capability = self.repository.capability(id)?;
        let query = self
            .repository
            .catalog()
            .queries
            .iter()
            .find(|query| query.id == capability.query_id)?;

        Some(KnowledgeDetailResponse {
            id: public_id.to_string(),
            kind: KnowledgeKind::Catalog,
            title: title(capability),
            status: availability(&capability.status),
            execution_mode: execution_mode(&capability.status),
            domain_id: capability.domain.clone(),
            data_area_ids: capability.data_areas.clone(),
            parameters: query
                .parameters
                .iter()
                .map(|parameter| ParameterResponse {
                    name: parameter.name.clone(),
                    kind: parameter.kind.clone(),
                    required: parameter.required,
                })
                .collect(),
            output_fields: query
                .output_fields
                .iter()
                .map(|field| crate::api::dto::management::OutputFieldResponse {
                    name: field.name.clone(),
                    sensitivity: field.sensitivity.clone(),
                })
                .collect(),
            limitations: Vec::new(),
        })
    }

    pub fn catalog_version(&self) -> String {
        catalog_version(self.repository.catalog())
    }
}

pub struct KnowledgeList {
    pub items: Vec<KnowledgeItemResponse>,
    pub next_cursor: Option<String>,
    pub catalog_version: String,
    pub index_version: Option<String>,
    pub reference_knowledge_status: Option<&'static str>,
}

impl KnowledgeService {
    fn disabled(&self) -> KnowledgeList {
        KnowledgeList {
            items: Vec::new(),
            next_cursor: None,
            catalog_version: self.catalog_version(),
            index_version: None,
            reference_knowledge_status: Some("disabled"),
        }
    }
}

pub enum KnowledgeLookupError {
    InvalidCursor,
}

fn inventory_item(capability: &CapabilityKnowledge) -> KnowledgeItemResponse {
    KnowledgeItemResponse {
        id: format!("catalog:{}", capability.id),
        kind: KnowledgeKind::Catalog,
        title: title(capability),
        status: availability(&capability.status),
        execution_mode: execution_mode(&capability.status),
        domain_id: capability.domain.clone(),
    }
}

fn matches_filter(item: &KnowledgeItemResponse, query: &KnowledgeQuery) -> bool {
    query.status.is_none_or(|status| item.status == status)
        && query
            .domain_id
            .as_ref()
            .is_none_or(|domain_id| item.domain_id == *domain_id)
}

fn title(capability: &CapabilityKnowledge) -> String {
    capability
        .display_name
        .clone()
        .unwrap_or_else(|| capability.id.clone())
}

fn availability(status: &str) -> KnowledgeStatus {
    match status {
        "approved_mvp" => KnowledgeStatus::Available,
        "deferred" => KnowledgeStatus::Deferred,
        _ => KnowledgeStatus::Unavailable,
    }
}

fn execution_mode(status: &str) -> ExecutionMode {
    if status == "approved_mvp" {
        ExecutionMode::ApprovedCatalogQuery
    } else {
        ExecutionMode::CatalogMetadataOnly
    }
}

fn catalog_version(catalog: &KnowledgeCatalog) -> String {
    // This is a safe identity derived from the validated in-memory catalog;
    // no catalog source files or SQL are returned to the caller.
    crate::knowledge::index::sync::catalog_content_hash(
        &crate::knowledge::retrieval::RetrievalDocumentBuilder::build(catalog),
    )
}
