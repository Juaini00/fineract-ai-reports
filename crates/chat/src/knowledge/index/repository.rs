use anyhow::Result;
use pgvector::Vector;
use serde_json::Value;
use sqlx::{FromRow, PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::knowledge::index::sync::{document_content_hash, retrieval_source_type_name};
use crate::knowledge::retrieval::RetrievalDocument;

pub struct IndexedRetrievalDocument {
    pub document: RetrievalDocument,
    pub embedding: Option<Vec<f32>>,
}

#[derive(Clone)]
pub struct KnowledgeRepository {
    pool: PgPool,
}

#[derive(Debug, FromRow)]
struct CatalogVersionIdRow {
    id: Uuid,
}

#[derive(Debug, Clone, FromRow)]
pub struct RetrievedKnowledgeCandidate {
    pub source_id: String,
    pub title: String,
    pub retrieval_text: String,
    pub metadata_json: Value,
    pub distance: f64,
}

impl KnowledgeRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn replace_indexed_catalog_version(
        &self,
        version: &str,
        content_hash: &str,
        documents: &[IndexedRetrievalDocument],
        embedding_model: Option<&str>,
        embedding_dimensions: Option<i32>,
    ) -> Result<Uuid> {
        let mut tx = self.pool.begin().await?;
        let catalog_version_id = upsert_catalog_version(
            &mut tx,
            version,
            content_hash,
            documents,
            embedding_model,
            embedding_dimensions,
        )
        .await?;

        sqlx::query("DELETE FROM knowledge_index WHERE catalog_version_id = $1")
            .bind(catalog_version_id)
            .execute(&mut *tx)
            .await?;

        for document in documents {
            insert_knowledge_index_document(&mut tx, catalog_version_id, document, embedding_model)
                .await?;
        }

        tx.commit().await?;
        Ok(catalog_version_id)
    }

    pub async fn search_capabilities(
        &self,
        embedding: Vec<f32>,
        allowed_capabilities: &[String],
        limit: i64,
    ) -> Result<Vec<RetrievedKnowledgeCandidate>> {
        if allowed_capabilities.is_empty() {
            return Ok(Vec::new());
        }

        let embedding = Vector::from(embedding);
        let rows = sqlx::query_as::<_, RetrievedKnowledgeCandidate>(
            r#"
            WITH latest_catalog AS (
                SELECT id
                FROM knowledge_catalog_versions
                WHERE status IN ('embedded', 'indexed')
                ORDER BY synced_at DESC NULLS LAST, created_at DESC
                LIMIT 1
            ), ranked AS (
                SELECT
                    source_id,
                    title,
                    retrieval_text,
                    metadata_json,
                    (embedding <=> $1) AS distance,
                    row_number() OVER (PARTITION BY source_id ORDER BY embedding <=> $1) AS row_number
                FROM knowledge_index
                WHERE catalog_version_id = (SELECT id FROM latest_catalog)
                  AND embedding IS NOT NULL
                  AND source_type = 'capability'
                  AND source_id = ANY($2)
            )
            SELECT
                source_id,
                title,
                retrieval_text,
                metadata_json,
                distance
            FROM ranked
            WHERE row_number = 1
            ORDER BY distance
            LIMIT $3
            "#,
        )
        .bind(embedding)
        .bind(allowed_capabilities)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows)
    }
}

async fn upsert_catalog_version(
    tx: &mut Transaction<'_, Postgres>,
    version: &str,
    content_hash: &str,
    documents: &[IndexedRetrievalDocument],
    embedding_model: Option<&str>,
    embedding_dimensions: Option<i32>,
) -> Result<Uuid> {
    let id = Uuid::new_v4();
    let status = if embedding_model.is_some() {
        "embedded"
    } else {
        "indexed"
    };
    let metadata_json = serde_json::json!({
        "embedding_status": status,
        "source": "knowledge_yaml",
    });

    let row = sqlx::query_as::<_, CatalogVersionIdRow>(
        r#"
        INSERT INTO knowledge_catalog_versions (
            id,
            version,
            content_hash,
            status,
            document_count,
            embedding_model,
            embedding_dimensions,
            metadata_json,
            synced_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, now())
        ON CONFLICT (content_hash) DO UPDATE
        SET
            version = EXCLUDED.version,
            status = EXCLUDED.status,
            document_count = EXCLUDED.document_count,
            embedding_model = EXCLUDED.embedding_model,
            embedding_dimensions = EXCLUDED.embedding_dimensions,
            metadata_json = EXCLUDED.metadata_json,
            synced_at = now()
        RETURNING id
        "#,
    )
    .bind(id)
    .bind(version)
    .bind(content_hash)
    .bind(status)
    .bind(documents.len() as i32)
    .bind(embedding_model)
    .bind(embedding_dimensions)
    .bind(metadata_json)
    .fetch_one(&mut **tx)
    .await?;

    Ok(row.id)
}

async fn insert_knowledge_index_document(
    tx: &mut Transaction<'_, Postgres>,
    catalog_version_id: Uuid,
    indexed: &IndexedRetrievalDocument,
    embedding_model: Option<&str>,
) -> Result<()> {
    let document = &indexed.document;
    let id = Uuid::new_v4();
    let source_type = retrieval_source_type_name(&document.source_type);
    let content_hash = document_content_hash(document);
    let metadata_json: Value = document.metadata_json.clone();
    let embedding = indexed.embedding.clone().map(Vector::from);

    sqlx::query(
        r#"
        INSERT INTO knowledge_index (
            id,
            catalog_version_id,
            source_type,
            source_id,
            source_path,
            title,
            retrieval_text,
            metadata_json,
            content_hash,
            embedding,
            embedding_model,
            embedded_at
        )
        VALUES ($1, $2, $3, $4, NULL, $5, $6, $7, $8, $9, $10, CASE WHEN $9 IS NULL THEN NULL ELSE now() END)
        "#,
    )
    .bind(id)
    .bind(catalog_version_id)
    .bind(source_type)
    .bind(&document.source_id)
    .bind(&document.title)
    .bind(&document.retrieval_text)
    .bind(metadata_json)
    .bind(content_hash)
    .bind(embedding)
    .bind(embedding_model)
    .execute(&mut **tx)
    .await?;

    Ok(())
}
