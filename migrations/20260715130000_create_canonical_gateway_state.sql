ALTER TABLE chat_messages
    ADD CONSTRAINT uq_chat_messages_job_id_id UNIQUE (job_id, id);

CREATE TABLE assistant_original_intents (
    id UUID PRIMARY KEY,
    job_id UUID NOT NULL REFERENCES chat_jobs(id) ON DELETE CASCADE,
    schema_version INTEGER NOT NULL CHECK (schema_version > 0),
    raw_message_id UUID NOT NULL,
    document_json JSONB NOT NULL,
    extraction_provenance_json JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT uq_assistant_original_intents_job UNIQUE (job_id),
    CONSTRAINT uq_assistant_original_intents_job_id UNIQUE (job_id, id),
    CONSTRAINT fk_assistant_original_intents_message_job
        FOREIGN KEY (job_id, raw_message_id)
        REFERENCES chat_messages(job_id, id)
);

CREATE TABLE assistant_fact_observations (
    id UUID PRIMARY KEY,
    job_id UUID NOT NULL REFERENCES chat_jobs(id) ON DELETE CASCADE,
    sequence BIGINT NOT NULL CHECK (sequence > 0),
    source_kind TEXT NOT NULL CHECK (source_kind IN (
        'original_request', 'clarification', 'deterministic_resolver',
        'approved_default', 'llm_advisory'
    )),
    source_id TEXT NOT NULL CHECK (length(trim(source_id)) > 0),
    field_path TEXT NOT NULL CHECK (length(trim(field_path)) > 0),
    typed_value_json JSONB NOT NULL,
    confidence REAL NULL CHECK (confidence BETWEEN 0.0 AND 1.0),
    extractor_version TEXT NOT NULL CHECK (length(trim(extractor_version)) > 0),
    observed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT uq_assistant_fact_observations_job_sequence UNIQUE (job_id, sequence),
    CONSTRAINT uq_assistant_fact_observations_source_field
        UNIQUE (job_id, source_kind, source_id, field_path)
);

CREATE TABLE assistant_effective_constraints (
    id UUID PRIMARY KEY,
    job_id UUID NOT NULL REFERENCES chat_jobs(id) ON DELETE CASCADE,
    revision BIGINT NOT NULL CHECK (revision >= 0),
    schema_version INTEGER NOT NULL CHECK (schema_version > 0),
    values_json JSONB NOT NULL,
    provenance_json JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT uq_assistant_effective_constraints_job_revision UNIQUE (job_id, revision),
    CONSTRAINT uq_assistant_effective_constraints_job_id UNIQUE (job_id, id)
);

CREATE TABLE assistant_planner_input_snapshots (
    id UUID PRIMARY KEY,
    job_id UUID NOT NULL REFERENCES chat_jobs(id) ON DELETE CASCADE,
    revision BIGINT NOT NULL CHECK (revision >= 0),
    original_intent_id UUID NOT NULL,
    effective_constraints_id UUID NOT NULL,
    capability_catalog_version UUID NOT NULL REFERENCES knowledge_catalog_versions(id),
    principal_projection_json JSONB NOT NULL,
    reference_instant TIMESTAMPTZ NOT NULL,
    timezone TEXT NOT NULL CHECK (length(trim(timezone)) > 0),
    selected_capability_id TEXT NOT NULL CHECK (length(trim(selected_capability_id)) > 0),
    normalized_parameters_json JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT uq_assistant_planner_input_snapshots_job_revision UNIQUE (job_id, revision),
    CONSTRAINT fk_assistant_planner_snapshots_original_job
        FOREIGN KEY (job_id, original_intent_id)
        REFERENCES assistant_original_intents(job_id, id),
    CONSTRAINT fk_assistant_planner_snapshots_effective_job
        FOREIGN KEY (job_id, effective_constraints_id)
        REFERENCES assistant_effective_constraints(job_id, id)
);

CREATE INDEX idx_assistant_effective_constraints_job_revision_desc
    ON assistant_effective_constraints (job_id, revision DESC);
CREATE INDEX idx_assistant_planner_input_snapshots_job_revision_desc
    ON assistant_planner_input_snapshots (job_id, revision DESC);
