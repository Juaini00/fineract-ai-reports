use anyhow::Result;
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;

use crate::knowledge::catalog::loader::KnowledgeLoader;
use crate::knowledge::catalog::validator::KnowledgeValidator;
use crate::knowledge::index::repository::KnowledgeRepository;
use crate::knowledge::retrieval::{
    RetrievalDocument, RetrievalDocumentBuilder, RetrievalSourceType,
};

#[derive(Debug, Clone)]
pub struct KnowledgeSyncSummary {
    pub catalog_version_id: Uuid,
    pub content_hash: String,
    pub document_count: usize,
}

pub struct KnowledgeSyncService {
    loader: KnowledgeLoader,
    repository: KnowledgeRepository,
}

impl KnowledgeSyncService {
    pub fn new(loader: KnowledgeLoader, pool: PgPool) -> Self {
        Self {
            loader,
            repository: KnowledgeRepository::new(pool),
        }
    }

    pub async fn sync(&self) -> Result<KnowledgeSyncSummary> {
        let catalog = self.loader.load()?;
        KnowledgeValidator::validate(&catalog)?;

        let documents = RetrievalDocumentBuilder::build(&catalog);
        let content_hash = catalog_content_hash(&documents);
        let catalog_version_id = self
            .repository
            .replace_indexed_catalog_version("local", &content_hash, &documents)
            .await?;

        Ok(KnowledgeSyncSummary {
            catalog_version_id,
            content_hash,
            document_count: documents.len(),
        })
    }
}

pub fn catalog_content_hash(documents: &[RetrievalDocument]) -> String {
    let mut hasher = Sha256::new();

    for document in documents {
        hasher.update(document_content_hash(document));
        hasher.update(b"\n");
    }

    format!("{:x}", hasher.finalize())
}

pub fn document_content_hash(document: &RetrievalDocument) -> String {
    let mut hasher = Sha256::new();

    hasher.update(retrieval_source_type_name(&document.source_type));
    hasher.update(b"\n");
    hasher.update(&document.source_id);
    hasher.update(b"\n");
    hasher.update(&document.title);
    hasher.update(b"\n");
    hasher.update(&document.retrieval_text);
    hasher.update(b"\n");
    hasher.update(
        serde_json::to_string(&document.metadata_json)
            .expect("retrieval metadata must serialize as JSON"),
    );

    format!("{:x}", hasher.finalize())
}

pub fn retrieval_source_type_name(source_type: &RetrievalSourceType) -> &'static str {
    match source_type {
        RetrievalSourceType::DataArea => "data_area",
        RetrievalSourceType::Domain => "domain",
        RetrievalSourceType::Capability => "capability",
        RetrievalSourceType::Query => "query",
    }
}
