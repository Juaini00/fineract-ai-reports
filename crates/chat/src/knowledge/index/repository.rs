use anyhow::Result;
use serde_json::Value;
use sqlx::{FromRow, PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::knowledge::index::sync::{document_content_hash, retrieval_source_type_name};
use crate::knowledge::retrieval::RetrievalDocument;

#[derive(Clone)]
pub struct KnowledgeRepository {
    pool: PgPool,
}

#[derive(Debug, FromRow)]
struct CatalogVersionIdRow {
    id: Uuid,
}

impl KnowledgeRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn replace_indexed_catalog_version(
        &self,
        version: &str,
        content_hash: &str,
        documents: &[RetrievalDocument],
    ) -> Result<Uuid> {
        let mut tx = self.pool.begin().await?;
        let catalog_version_id =
            upsert_catalog_version(&mut tx, version, content_hash, documents).await?;

        sqlx::query("DELETE FROM knowledge_index WHERE catalog_version_id = $1")
            .bind(catalog_version_id)
            .execute(&mut *tx)
            .await?;

        for document in documents {
            insert_knowledge_index_document(&mut tx, catalog_version_id, document).await?;
        }

        tx.commit().await?;
        Ok(catalog_version_id)
    }
}

async fn upsert_catalog_version(
    tx: &mut Transaction<'_, Postgres>,
    version: &str,
    content_hash: &str,
    documents: &[RetrievalDocument],
) -> Result<Uuid> {
    let id = Uuid::new_v4();
    let metadata_json = serde_json::json!({
        "embedding_status": "pending",
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
        VALUES ($1, $2, $3, 'indexed', $4, NULL, NULL, $5, now())
        ON CONFLICT (content_hash) DO UPDATE
        SET
            version = EXCLUDED.version,
            status = 'indexed',
            document_count = EXCLUDED.document_count,
            embedding_model = NULL,
            embedding_dimensions = NULL,
            metadata_json = EXCLUDED.metadata_json,
            synced_at = now()
        RETURNING id
        "#,
    )
    .bind(id)
    .bind(version)
    .bind(content_hash)
    .bind(documents.len() as i32)
    .bind(metadata_json)
    .fetch_one(&mut **tx)
    .await?;

    Ok(row.id)
}

async fn insert_knowledge_index_document(
    tx: &mut Transaction<'_, Postgres>,
    catalog_version_id: Uuid,
    document: &RetrievalDocument,
) -> Result<()> {
    let id = Uuid::new_v4();
    let source_type = retrieval_source_type_name(&document.source_type);
    let content_hash = document_content_hash(document);
    let metadata_json: Value = document.metadata_json.clone();

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
        VALUES ($1, $2, $3, $4, NULL, $5, $6, $7, $8, NULL, NULL, NULL)
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
    .execute(&mut **tx)
    .await?;

    Ok(())
}
