use anyhow::{Result, bail};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;

use crate::knowledge::catalog::loader::KnowledgeLoader;
use crate::knowledge::catalog::validator::KnowledgeValidator;
use crate::knowledge::embedding::VoyageEmbeddingClient;
use crate::knowledge::index::repository::{IndexedRetrievalDocument, KnowledgeRepository};
use crate::knowledge::retrieval::{
    RetrievalDocument, RetrievalDocumentBuilder, RetrievalSourceType,
};

#[derive(Debug, Clone)]
pub struct KnowledgeSyncSummary {
    pub catalog_version_id: Uuid,
    pub content_hash: String,
    pub document_count: usize,
    pub embedding_model: Option<String>,
}

pub struct KnowledgeSyncService {
    loader: KnowledgeLoader,
    repository: KnowledgeRepository,
    embedding_client: Option<VoyageEmbeddingClient>,
    embedding_model: Option<String>,
    embedding_dimensions: Option<i32>,
}

impl KnowledgeSyncService {
    pub fn new(loader: KnowledgeLoader, pool: PgPool) -> Self {
        Self {
            loader,
            repository: KnowledgeRepository::new(pool),
            embedding_client: None,
            embedding_model: None,
            embedding_dimensions: None,
        }
    }

    pub fn with_embeddings(
        loader: KnowledgeLoader,
        pool: PgPool,
        embedding_client: VoyageEmbeddingClient,
        embedding_model: String,
        embedding_dimensions: i32,
    ) -> Self {
        Self {
            loader,
            repository: KnowledgeRepository::new(pool),
            embedding_client: Some(embedding_client),
            embedding_model: Some(embedding_model),
            embedding_dimensions: Some(embedding_dimensions),
        }
    }

    /// Syncs only when the on-disk catalog hash differs from the indexed one.
    /// Without this, editing knowledge YAML leaves the vector index pinned to
    /// an older catalog version: the new capability is authorized (in-memory
    /// catalog) but invisible to embedding retrieval, so it can never win
    /// ranking against indexed competitors.
    pub async fn sync_if_stale(&self) -> Result<Option<KnowledgeSyncSummary>> {
        let catalog = self.loader.load()?;
        KnowledgeValidator::validate(&catalog)?;
        let documents = RetrievalDocumentBuilder::build(&catalog);
        // An empty catalog is never a legitimate sync: it means the loader
        // found nothing (wrong CATALOG_PATH, wrong working directory), and
        // syncing it would replace a good index with zero documents and
        // silently disable all retrieval. Refuse instead of destroying it.
        if documents.is_empty() {
            bail!(
                "knowledge catalog loaded 0 documents; refusing to replace the retrieval index \
                 (check CATALOG_PATH and the process working directory)"
            );
        }
        let content_hash = catalog_content_hash(&documents);

        let indexed = self.repository.latest_catalog_version().await?;
        if indexed.is_some_and(|version| {
            version.content_hash == content_hash && version.status == "embedded"
        }) {
            return Ok(None);
        }
        self.sync_documents(documents, content_hash).await.map(Some)
    }

    pub async fn sync(&self) -> Result<KnowledgeSyncSummary> {
        let catalog = self.loader.load()?;
        KnowledgeValidator::validate(&catalog)?;

        let documents = RetrievalDocumentBuilder::build(&catalog);
        let content_hash = catalog_content_hash(&documents);
        self.sync_documents(documents, content_hash).await
    }

    async fn sync_documents(
        &self,
        documents: Vec<RetrievalDocument>,
        content_hash: String,
    ) -> Result<KnowledgeSyncSummary> {
        let indexed_documents = self.indexed_documents(documents).await?;
        let catalog_version_id = self
            .repository
            .replace_indexed_catalog_version(
                "local",
                &content_hash,
                &indexed_documents,
                self.embedding_model.as_deref(),
                self.embedding_dimensions,
            )
            .await?;

        Ok(KnowledgeSyncSummary {
            catalog_version_id,
            content_hash,
            document_count: indexed_documents.len(),
            embedding_model: self.embedding_model.clone(),
        })
    }

    async fn indexed_documents(
        &self,
        documents: Vec<RetrievalDocument>,
    ) -> Result<Vec<IndexedRetrievalDocument>> {
        let Some(client) = self.embedding_client.as_ref() else {
            return Ok(documents
                .into_iter()
                .map(|document| IndexedRetrievalDocument {
                    document,
                    embedding: None,
                })
                .collect());
        };

        let inputs = documents
            .iter()
            .map(|document| document.retrieval_text.clone())
            .collect::<Vec<_>>();
        let embeddings = client.embed_documents(&inputs).await?;

        Ok(documents
            .into_iter()
            .zip(embeddings)
            .map(|(document, embedding)| IndexedRetrievalDocument {
                document,
                embedding: Some(embedding),
            })
            .collect())
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
        RetrievalSourceType::Schema => "schema",
        RetrievalSourceType::Metric => "metric",
        RetrievalSourceType::Capability => "capability",
        RetrievalSourceType::Query => "query",
        RetrievalSourceType::Policy => "policy",
        RetrievalSourceType::Response => "response",
    }
}
