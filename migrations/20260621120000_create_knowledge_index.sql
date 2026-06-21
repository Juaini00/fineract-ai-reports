CREATE EXTENSION IF NOT EXISTS vector;

CREATE TABLE IF NOT EXISTS knowledge_catalog_versions (
    id UUID PRIMARY KEY,
    version TEXT NOT NULL,
    content_hash TEXT NOT NULL UNIQUE,
    status TEXT NOT NULL,
    document_count INTEGER NOT NULL DEFAULT 0,
    embedding_model TEXT NULL,
    embedding_dimensions INTEGER NULL,
    metadata_json JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    synced_at TIMESTAMPTZ NULL,
    CONSTRAINT chk_knowledge_catalog_versions_status CHECK (status IN ('loaded', 'validated', 'indexed', 'embedded', 'failed')),
    CONSTRAINT chk_knowledge_catalog_versions_document_count CHECK (document_count >= 0),
    CONSTRAINT chk_knowledge_catalog_versions_embedding_dimensions CHECK (embedding_dimensions IS NULL OR embedding_dimensions > 0)
);

CREATE TABLE IF NOT EXISTS knowledge_index (
    id UUID PRIMARY KEY,
    catalog_version_id UUID NOT NULL REFERENCES knowledge_catalog_versions(id) ON DELETE CASCADE,
    source_type TEXT NOT NULL,
    source_id TEXT NOT NULL,
    source_path TEXT NULL,
    title TEXT NOT NULL,
    retrieval_text TEXT NOT NULL,
    metadata_json JSONB NOT NULL DEFAULT '{}'::jsonb,
    content_hash TEXT NOT NULL,
    embedding vector(1024) NULL,
    embedding_model TEXT NULL,
    embedded_at TIMESTAMPTZ NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT chk_knowledge_index_source_type CHECK (source_type IN ('data_area', 'domain', 'capability', 'query', 'schema', 'metric', 'policy', 'response')),
    CONSTRAINT chk_knowledge_index_retrieval_text_not_empty CHECK (length(trim(retrieval_text)) > 0)
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_knowledge_index_catalog_source
    ON knowledge_index (catalog_version_id, source_type, source_id);

CREATE INDEX IF NOT EXISTS idx_knowledge_index_catalog_version_id
    ON knowledge_index (catalog_version_id);

CREATE INDEX IF NOT EXISTS idx_knowledge_index_source
    ON knowledge_index (source_type, source_id);

CREATE INDEX IF NOT EXISTS idx_knowledge_index_content_hash
    ON knowledge_index (content_hash);

CREATE INDEX IF NOT EXISTS idx_knowledge_index_metadata_gin
    ON knowledge_index USING GIN (metadata_json);

CREATE INDEX IF NOT EXISTS idx_knowledge_index_embedding_cosine
    ON knowledge_index USING ivfflat (embedding vector_cosine_ops)
    WITH (lists = 100)
    WHERE embedding IS NOT NULL;
