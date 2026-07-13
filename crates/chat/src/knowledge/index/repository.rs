use anyhow::Result;
use pgvector::Vector;
use serde_json::Value;
use sqlx::{AssertSqlSafe, FromRow, PgPool, Postgres, SqlSafeStr, Transaction};
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
    pub source_type: String,
    pub source_id: String,
    pub title: String,
    pub retrieval_text: String,
    pub metadata_json: Value,
    pub distance: f64,
}

#[derive(Debug, Clone, FromRow)]
pub struct LatestCatalogIndex {
    pub id: Uuid,
    pub content_hash: String,
    pub status: String,
    pub embedding_model: Option<String>,
    pub embedding_dimensions: Option<i32>,
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

    pub async fn latest_embedded_catalog(&self) -> Result<Option<LatestCatalogIndex>> {
        let row = sqlx::query_as::<_, LatestCatalogIndex>(
            r#"
            SELECT id, content_hash, status, embedding_model, embedding_dimensions
            FROM knowledge_catalog_versions
            WHERE status = 'embedded'
            ORDER BY synced_at DESC NULLS LAST, created_at DESC
            LIMIT 1
            "#,
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    pub async fn search_capabilities(
        &self,
        embedding: Vec<f32>,
        allow_all_capabilities: bool,
        allowed_capabilities: &[String],
        limit: i64,
    ) -> Result<Vec<RetrievedKnowledgeCandidate>> {
        if !allow_all_capabilities && allowed_capabilities.is_empty() {
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
                    source_type,
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
                  AND ($2 OR source_id = ANY($3))
            )
            SELECT
                source_type,
                source_id,
                title,
                retrieval_text,
                metadata_json,
                distance
            FROM ranked
            WHERE row_number = 1
            ORDER BY distance
            LIMIT $4
            "#,
        )
        .bind(embedding)
        .bind(allow_all_capabilities)
        .bind(allowed_capabilities)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows)
    }

    /// Non-capability context (data_area, domain, query) from the latest indexed
    /// catalog version. Returned alongside capability candidates for audit and
    /// future LLM planner context — never drives execution.
    pub async fn search_context(
        &self,
        embedding: Vec<f32>,
        limit: i64,
    ) -> Result<Vec<RetrievedKnowledgeCandidate>> {
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
                    source_type,
                    source_id,
                    title,
                    retrieval_text,
                    metadata_json,
                    (embedding <=> $1) AS distance,
                    row_number() OVER (PARTITION BY source_type, source_id ORDER BY embedding <=> $1) AS row_number
                FROM knowledge_index
                WHERE catalog_version_id = (SELECT id FROM latest_catalog)
                  AND embedding IS NOT NULL
                  AND source_type <> 'capability'
            )
            SELECT
                source_type,
                source_id,
                title,
                retrieval_text,
                metadata_json,
                distance
            FROM ranked
            WHERE row_number = 1
            ORDER BY distance
            LIMIT $2
            "#,
        )
        .bind(embedding)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows)
    }

    pub async fn search_hybrid_by_source_type(
        &self,
        source_type: &str,
        embedding: Vec<f32>,
        keyword_terms: &[String],
        allowed_source_ids: Option<&[String]>,
        metadata_filter: &std::collections::BTreeMap<String, String>,
        limit: i64,
    ) -> Result<Vec<RetrievedKnowledgeCandidate>> {
        let metadata_keys: Vec<String> = metadata_filter.keys().cloned().collect();
        let keyword_terms = keyword_terms
            .iter()
            .map(|term| term.trim().to_lowercase())
            .filter(|term| term.len() >= 2)
            .collect::<Vec<_>>();
        let has_keyword_terms = !keyword_terms.is_empty();
        let sql = build_hybrid_sql(
            allowed_source_ids.is_some(),
            &metadata_keys,
            has_keyword_terms,
        );
        let mut query =
            sqlx::query_as::<_, RetrievedKnowledgeCandidate>(AssertSqlSafe(sql).into_sql_str())
                .bind(Vector::from(embedding))
                .bind(source_type);

        if let Some(ids) = allowed_source_ids {
            query = query.bind(ids.to_vec());
        }
        for key in &metadata_keys {
            query = query.bind(metadata_filter.get(key).cloned().unwrap_or_default());
        }
        if has_keyword_terms {
            query = query.bind(keyword_terms);
        }

        let rows = query.bind(limit).fetch_all(&self.pool).await?;
        Ok(rows)
    }
}

pub(crate) fn build_hybrid_sql(
    has_allowed_ids: bool,
    metadata_keys: &[String],
    has_keyword_terms: bool,
) -> String {
    let mut param_idx = 3;
    let allowed_param = if has_allowed_ids {
        let value = Some(param_idx);
        param_idx += 1;
        value
    } else {
        None
    };
    let metadata_params = metadata_keys
        .iter()
        .map(|key| {
            let current = param_idx;
            param_idx += 1;
            (key, current)
        })
        .collect::<Vec<_>>();
    let keyword_param = if has_keyword_terms {
        let value = Some(param_idx);
        param_idx += 1;
        value
    } else {
        None
    };
    let keyword_hits = keyword_param.map_or_else(
        || "0".to_string(),
        |idx| {
            format!(
                "(SELECT count(*)::float8 FROM unnest(${idx}::text[]) AS term WHERE lower(retrieval_text) LIKE '%' || term || '%')"
            )
        },
    );
    // Blueprint Step 7 weighted rerank:
    //   final = 0.45*semantic + 0.35*keyword + 0.15*metadata + 0.05*freshness
    // metadata_score = 1.0 (rows already pass metadata WHERE filters).
    // freshness_score decays over 90 days from embedded_at.
    // Sort key is (1 - final), so ORDER BY ASC still surfaces best matches first.
    let semantic_score = "GREATEST(1.0 - ((embedding <=> $1) / 2.0), 0.0)";
    let keyword_score = format!("LEAST({keyword_hits}, 3.0) / 3.0");
    let freshness_score = "GREATEST(1.0 - LEAST(EXTRACT(EPOCH FROM (now() - COALESCE(embedded_at, now()))) / 7776000.0, 1.0), 0.0)";
    let hybrid_distance = format!(
        "1.0 - (0.45 * {semantic_score} + 0.35 * ({keyword_score}) + 0.15 * 1.0 + 0.05 * {freshness_score})"
    );
    let mut sql = String::from(
        format!(
            r#"
        WITH latest_catalog AS (
            SELECT id
            FROM knowledge_catalog_versions
            WHERE status = 'embedded'
            ORDER BY synced_at DESC NULLS LAST, created_at DESC
            LIMIT 1
        ), ranked AS (
            SELECT
                source_type,
                source_id,
                title,
                retrieval_text,
                metadata_json,
                (embedding <=> $1) AS vector_distance,
                {keyword_hits} AS keyword_hits,
                {hybrid_distance} AS hybrid_distance,
                row_number() OVER (PARTITION BY source_type, source_id ORDER BY {hybrid_distance}) AS row_number
            FROM knowledge_index
            WHERE catalog_version_id = (SELECT id FROM latest_catalog)
              AND embedding IS NOT NULL
              AND source_type = $2
        "#
        )
        .as_str(),
    );

    if let Some(param) = allowed_param {
        sql.push_str(&format!(
            "\n              AND source_id = ANY(${param}::text[])"
        ));
    }
    for (key, param) in metadata_params {
        sql.push_str(&format!(
            "\n              AND metadata_json->>'{key}' = ${param}"
        ));
    }
    sql.push_str(&format!(
        r#"
        )
        SELECT source_type, source_id, title, retrieval_text, metadata_json, hybrid_distance AS distance
        FROM ranked
        WHERE row_number = 1
        ORDER BY hybrid_distance
        LIMIT ${param_idx}
        "#
    ));
    sql
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

#[cfg(test)]
mod tests {
    #[test]
    fn hybrid_sql_includes_source_type_and_allowed_source_id_filter() {
        let sql = super::build_hybrid_sql(true, &["domain".into(), "office_scope".into()], false);

        assert!(sql.contains("source_type = $2"));
        assert!(sql.contains("source_id = ANY($3::text[])"));
        assert!(sql.contains("metadata_json->>'domain' = $"));
        assert!(sql.contains("metadata_json->>'office_scope' = $"));
        assert!(sql.contains("ORDER BY hybrid_distance"));
    }

    #[test]
    fn hybrid_sql_without_allowed_ids_skips_source_id_filter() {
        let sql = super::build_hybrid_sql(false, &[], false);

        assert!(!sql.contains("source_id = ANY"));
        assert!(sql.contains("source_type = $2"));
    }

    #[test]
    fn hybrid_sql_with_keywords_boosts_keyword_matches() {
        let sql = super::build_hybrid_sql(true, &[], true);

        assert!(sql.contains("$4::text[]"));
        assert!(sql.contains("keyword_hits"));
        assert!(sql.contains("hybrid_distance"));
        assert!(sql.contains("ORDER BY hybrid_distance"));
    }

    #[test]
    fn hybrid_sql_uses_blueprint_weighted_rerank() {
        let sql = super::build_hybrid_sql(false, &[], true);

        assert!(sql.contains("0.45"), "semantic weight missing");
        assert!(sql.contains("0.35"), "keyword weight missing");
        assert!(sql.contains("0.15"), "metadata weight missing");
        assert!(sql.contains("0.05"), "freshness weight missing");
        assert!(
            sql.contains("embedded_at"),
            "freshness term must use embedded_at"
        );
    }
}
