use std::fmt;

use chrono::{DateTime, Utc};
use sqlx::{FromRow, PgConnection, PgPool};
use uuid::Uuid;

use super::canonical_state::{
    ConstraintContracts, EffectiveConstraints, FactObservation, MergeError, OriginalIntent,
    PlannerInputSnapshot, merge_observations,
};

const SCHEMA_VERSION: i32 = 1;

#[derive(Debug)]
pub enum CanonicalStateRepositoryError {
    Conflict(&'static str),
    InvalidSchemaVersion(i32),
    InvalidJson(serde_json::Error),
    Merge(MergeError),
    Database(sqlx::Error),
}

impl fmt::Display for CanonicalStateRepositoryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Conflict(kind) => write!(f, "conflicting immutable {kind}"),
            Self::InvalidSchemaVersion(version) => {
                write!(f, "unsupported schema version {version}")
            }
            Self::InvalidJson(error) => write!(f, "malformed canonical JSON: {error}"),
            Self::Merge(error) => write!(f, "cannot derive effective constraints: {error}"),
            Self::Database(error) => write!(f, "canonical state database error: {error}"),
        }
    }
}

impl std::error::Error for CanonicalStateRepositoryError {}
impl From<sqlx::Error> for CanonicalStateRepositoryError {
    fn from(value: sqlx::Error) -> Self {
        Self::Database(value)
    }
}
impl From<serde_json::Error> for CanonicalStateRepositoryError {
    fn from(value: serde_json::Error) -> Self {
        Self::InvalidJson(value)
    }
}
impl From<MergeError> for CanonicalStateRepositoryError {
    fn from(value: MergeError) -> Self {
        Self::Merge(value)
    }
}

pub type CanonicalStateResult<T> = Result<T, CanonicalStateRepositoryError>;

#[derive(Clone)]
pub struct CanonicalStateRepository {
    pool: PgPool,
}

impl CanonicalStateRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn insert_original_intent(
        &self,
        value: &OriginalIntent,
    ) -> CanonicalStateResult<OriginalIntent> {
        let mut connection = self.pool.acquire().await?;
        insert_original(&mut connection, value).await
    }

    pub async fn get_original_intent(
        &self,
        job_id: Uuid,
    ) -> CanonicalStateResult<Option<OriginalIntent>> {
        let row = sqlx::query_as::<_, OriginalRow>("SELECT id, job_id, schema_version, raw_message_id, document_json, extraction_provenance_json, created_at FROM assistant_original_intents WHERE job_id = $1")
            .bind(job_id).fetch_optional(&self.pool).await?;
        row.map(original_from_row).transpose()
    }

    pub async fn append_observations(
        &self,
        job_id: Uuid,
        values: &[FactObservation],
    ) -> CanonicalStateResult<Vec<FactObservation>> {
        if values.iter().any(|value| value.job_id != job_id) {
            return Err(CanonicalStateRepositoryError::Conflict("observation job"));
        }
        let mut tx = self.pool.begin().await?;
        let mut saved = Vec::with_capacity(values.len());
        for value in values {
            saved.push(insert_observation(&mut tx, value).await?);
        }
        tx.commit().await?;
        Ok(saved)
    }

    pub async fn list_observations(
        &self,
        job_id: Uuid,
    ) -> CanonicalStateResult<Vec<FactObservation>> {
        let rows = sqlx::query_as::<_, ObservationRow>("SELECT id, job_id, sequence, source_kind, source_id, field_path, typed_value_json, confidence, extractor_version, observed_at FROM assistant_fact_observations WHERE job_id = $1 ORDER BY sequence, id")
            .bind(job_id).fetch_all(&self.pool).await?;
        rows.into_iter().map(observation_from_row).collect()
    }

    pub async fn insert_effective_constraints(
        &self,
        value: &EffectiveConstraints,
    ) -> CanonicalStateResult<EffectiveConstraints> {
        let mut connection = self.pool.acquire().await?;
        insert_effective(&mut connection, value).await
    }

    pub async fn get_effective_constraints(
        &self,
        job_id: Uuid,
        revision: i64,
    ) -> CanonicalStateResult<Option<EffectiveConstraints>> {
        let row = sqlx::query_as::<_, EffectiveRow>("SELECT id, job_id, revision, schema_version, values_json, provenance_json, created_at FROM assistant_effective_constraints WHERE job_id = $1 AND revision = $2")
            .bind(job_id).bind(revision).fetch_optional(&self.pool).await?;
        row.map(effective_from_row).transpose()
    }

    pub async fn insert_planner_snapshot(
        &self,
        value: &PlannerInputSnapshot,
    ) -> CanonicalStateResult<PlannerInputSnapshot> {
        let mut connection = self.pool.acquire().await?;
        insert_planner(&mut connection, value).await
    }

    pub async fn get_planner_snapshot(
        &self,
        id: Uuid,
        job_id: Uuid,
    ) -> CanonicalStateResult<Option<PlannerInputSnapshot>> {
        let row = sqlx::query_as::<_, PlannerRow>("SELECT id, job_id, revision, original_intent_id, effective_constraints_id, capability_catalog_version, principal_projection_json, reference_instant, timezone, selected_capability_id, normalized_parameters_json, created_at FROM assistant_planner_input_snapshots WHERE id = $1 AND job_id = $2")
            .bind(id).bind(job_id).fetch_optional(&self.pool).await?;
        row.map(planner_from_row).transpose()
    }

    pub async fn insert_initial_state(
        &self,
        intent: &OriginalIntent,
        observations: &[FactObservation],
        effective: &EffectiveConstraints,
    ) -> CanonicalStateResult<(OriginalIntent, Vec<FactObservation>, EffectiveConstraints)> {
        if observations.iter().any(|item| item.job_id != intent.job_id)
            || effective.job_id != intent.job_id
        {
            return Err(CanonicalStateRepositoryError::Conflict("initial state job"));
        }
        let mut tx = self.pool.begin().await?;
        let saved_intent = insert_original(&mut tx, intent).await?;
        let mut saved_observations = Vec::with_capacity(observations.len());
        for observation in observations {
            saved_observations.push(insert_observation(&mut tx, observation).await?);
        }
        let saved_effective = insert_effective(&mut tx, effective).await?;
        tx.commit().await?;
        Ok((saved_intent, saved_observations, saved_effective))
    }

    pub async fn derive_and_insert_effective(
        &self,
        job_id: Uuid,
        revision: i64,
        contracts: &ConstraintContracts,
    ) -> CanonicalStateResult<EffectiveConstraints> {
        let mut tx = self.pool.begin().await?;
        let rows = sqlx::query_as::<_, ObservationRow>("SELECT id, job_id, sequence, source_kind, source_id, field_path, typed_value_json, confidence, extractor_version, observed_at FROM assistant_fact_observations WHERE job_id = $1 ORDER BY sequence, id")
            .bind(job_id).fetch_all(&mut *tx).await?;
        let observations = rows
            .into_iter()
            .map(observation_from_row)
            .collect::<CanonicalStateResult<Vec<_>>>()?;
        let mut effective = merge_observations(job_id, revision, &observations, contracts)?;
        effective.id = Uuid::new_v4();
        let saved = insert_effective(&mut tx, &effective).await?;
        tx.commit().await?;
        Ok(saved)
    }
}

async fn insert_original(
    db: &mut PgConnection,
    value: &OriginalIntent,
) -> CanonicalStateResult<OriginalIntent> {
    let mut value = value.clone();
    value.created_at = postgres_time(value.created_at);
    schema(value.schema_version)?;
    sqlx::query("INSERT INTO assistant_original_intents (id, job_id, schema_version, raw_message_id, document_json, extraction_provenance_json, created_at) VALUES ($1,$2,$3,$4,$5,$6,$7) ON CONFLICT DO NOTHING")
        .bind(value.id).bind(value.job_id).bind(value.schema_version).bind(value.raw_message_id).bind(serde_json::to_value(&value)?).bind(serde_json::to_value(&value.extraction_provenance)?).bind(value.created_at).execute(&mut *db).await?;
    let row = sqlx::query_as::<_, OriginalRow>("SELECT id, job_id, schema_version, raw_message_id, document_json, extraction_provenance_json, created_at FROM assistant_original_intents WHERE id = $1 OR job_id = $2")
        .bind(value.id).bind(value.job_id).fetch_one(&mut *db).await?;
    exact(original_from_row(row)?, &value, "original intent")
}

async fn insert_observation(
    db: &mut PgConnection,
    value: &FactObservation,
) -> CanonicalStateResult<FactObservation> {
    let mut value = value.clone();
    value.observed_at = postgres_time(value.observed_at);
    let source_kind = json_name(&value.source_kind)?;
    let field_path = json_name(&value.field_path)?;
    sqlx::query("INSERT INTO assistant_fact_observations (id, job_id, sequence, source_kind, source_id, field_path, typed_value_json, confidence, extractor_version, observed_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10) ON CONFLICT DO NOTHING")
        .bind(value.id).bind(value.job_id).bind(value.sequence).bind(&source_kind).bind(&value.source_id).bind(&field_path).bind(serde_json::to_value(&value.typed_value)?).bind(value.confidence).bind(&value.extractor_version).bind(value.observed_at).execute(&mut *db).await?;
    let row = sqlx::query_as::<_, ObservationRow>("SELECT id, job_id, sequence, source_kind, source_id, field_path, typed_value_json, confidence, extractor_version, observed_at FROM assistant_fact_observations WHERE id=$1 OR (job_id=$2 AND sequence=$3) OR (job_id=$2 AND source_kind=$4 AND source_id=$5 AND field_path=$6) LIMIT 1")
        .bind(value.id).bind(value.job_id).bind(value.sequence).bind(source_kind).bind(&value.source_id).bind(field_path).fetch_one(&mut *db).await?;
    exact(observation_from_row(row)?, &value, "fact observation")
}

async fn insert_effective(
    db: &mut PgConnection,
    value: &EffectiveConstraints,
) -> CanonicalStateResult<EffectiveConstraints> {
    let mut value = value.clone();
    value.created_at = postgres_time(value.created_at);
    schema(value.schema_version)?;
    sqlx::query("INSERT INTO assistant_effective_constraints (id, job_id, revision, schema_version, values_json, provenance_json, created_at) VALUES ($1,$2,$3,$4,$5,$6,$7) ON CONFLICT DO NOTHING")
        .bind(value.id).bind(value.job_id).bind(value.revision).bind(value.schema_version).bind(serde_json::to_value(&value.values)?).bind(serde_json::to_value(&value.winning_observation_ids)?).bind(value.created_at).execute(&mut *db).await?;
    let row = sqlx::query_as::<_, EffectiveRow>("SELECT id, job_id, revision, schema_version, values_json, provenance_json, created_at FROM assistant_effective_constraints WHERE id=$1 OR (job_id=$2 AND revision=$3) LIMIT 1")
        .bind(value.id).bind(value.job_id).bind(value.revision).fetch_one(&mut *db).await?;
    exact(effective_from_row(row)?, &value, "effective constraints")
}

async fn insert_planner(
    db: &mut PgConnection,
    value: &PlannerInputSnapshot,
) -> CanonicalStateResult<PlannerInputSnapshot> {
    let mut value = value.clone();
    value.reference_instant = postgres_time(value.reference_instant);
    value.created_at = postgres_time(value.created_at);
    sqlx::query("INSERT INTO assistant_planner_input_snapshots (id, job_id, revision, original_intent_id, effective_constraints_id, capability_catalog_version, principal_projection_json, reference_instant, timezone, selected_capability_id, normalized_parameters_json, created_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12) ON CONFLICT DO NOTHING")
        .bind(value.id).bind(value.job_id).bind(value.revision).bind(value.original_intent_id).bind(value.effective_constraints_id).bind(value.capability_catalog_version).bind(serde_json::to_value(&value.principal_projection)?).bind(value.reference_instant).bind(&value.timezone).bind(&value.selected_capability_id).bind(&value.normalized_parameters).bind(value.created_at).execute(&mut *db).await?;
    let row = sqlx::query_as::<_, PlannerRow>("SELECT id, job_id, revision, original_intent_id, effective_constraints_id, capability_catalog_version, principal_projection_json, reference_instant, timezone, selected_capability_id, normalized_parameters_json, created_at FROM assistant_planner_input_snapshots WHERE id=$1 OR (job_id=$2 AND revision=$3) LIMIT 1")
        .bind(value.id).bind(value.job_id).bind(value.revision).fetch_one(&mut *db).await?;
    exact(planner_from_row(row)?, &value, "planner snapshot")
}

fn schema(version: i32) -> CanonicalStateResult<()> {
    if version == SCHEMA_VERSION {
        Ok(())
    } else {
        Err(CanonicalStateRepositoryError::InvalidSchemaVersion(version))
    }
}
fn exact<T: PartialEq>(stored: T, expected: &T, kind: &'static str) -> CanonicalStateResult<T> {
    if &stored == expected {
        Ok(stored)
    } else {
        Err(CanonicalStateRepositoryError::Conflict(kind))
    }
}
fn json_name<T: serde::Serialize>(value: &T) -> CanonicalStateResult<String> {
    Ok(serde_json::to_value(value)?
        .as_str()
        .ok_or_else(|| serde_json::Error::io(std::io::Error::other("expected string enum")))?
        .to_owned())
}
fn postgres_time(value: DateTime<Utc>) -> DateTime<Utc> {
    DateTime::from_timestamp_micros(value.timestamp_micros()).expect("valid DateTime timestamp")
}

#[derive(FromRow)]
struct OriginalRow {
    id: Uuid,
    job_id: Uuid,
    schema_version: i32,
    raw_message_id: Uuid,
    document_json: serde_json::Value,
    extraction_provenance_json: serde_json::Value,
    created_at: DateTime<Utc>,
}
fn original_from_row(r: OriginalRow) -> CanonicalStateResult<OriginalIntent> {
    schema(r.schema_version)?;
    let value: OriginalIntent = serde_json::from_value(r.document_json)?;
    let provenance = serde_json::from_value(r.extraction_provenance_json)?;
    let row_value = OriginalIntent {
        id: r.id,
        job_id: r.job_id,
        schema_version: r.schema_version,
        raw_message_id: r.raw_message_id,
        extraction_provenance: provenance,
        created_at: r.created_at,
        ..value.clone()
    };
    exact(row_value, &value, "original intent row")
}

#[derive(FromRow)]
struct ObservationRow {
    id: Uuid,
    job_id: Uuid,
    sequence: i64,
    source_kind: String,
    source_id: String,
    field_path: String,
    typed_value_json: serde_json::Value,
    confidence: Option<f32>,
    extractor_version: String,
    observed_at: DateTime<Utc>,
}
fn observation_from_row(r: ObservationRow) -> CanonicalStateResult<FactObservation> {
    Ok(FactObservation {
        id: r.id,
        job_id: r.job_id,
        sequence: r.sequence,
        source_kind: serde_json::from_value(serde_json::Value::String(r.source_kind))?,
        source_id: r.source_id,
        field_path: serde_json::from_value(serde_json::Value::String(r.field_path))?,
        typed_value: serde_json::from_value(r.typed_value_json)?,
        confidence: r.confidence,
        extractor_version: r.extractor_version,
        observed_at: r.observed_at,
    })
}

#[derive(FromRow)]
struct EffectiveRow {
    id: Uuid,
    job_id: Uuid,
    revision: i64,
    schema_version: i32,
    values_json: serde_json::Value,
    provenance_json: serde_json::Value,
    created_at: DateTime<Utc>,
}
fn effective_from_row(r: EffectiveRow) -> CanonicalStateResult<EffectiveConstraints> {
    schema(r.schema_version)?;
    Ok(EffectiveConstraints {
        id: r.id,
        job_id: r.job_id,
        revision: r.revision,
        schema_version: r.schema_version,
        values: serde_json::from_value(r.values_json)?,
        winning_observation_ids: serde_json::from_value(r.provenance_json)?,
        created_at: r.created_at,
    })
}

#[derive(FromRow)]
struct PlannerRow {
    id: Uuid,
    job_id: Uuid,
    revision: i64,
    original_intent_id: Uuid,
    effective_constraints_id: Uuid,
    capability_catalog_version: Uuid,
    principal_projection_json: serde_json::Value,
    reference_instant: DateTime<Utc>,
    timezone: String,
    selected_capability_id: String,
    normalized_parameters_json: serde_json::Value,
    created_at: DateTime<Utc>,
}
fn planner_from_row(r: PlannerRow) -> CanonicalStateResult<PlannerInputSnapshot> {
    Ok(PlannerInputSnapshot {
        id: r.id,
        job_id: r.job_id,
        revision: r.revision,
        original_intent_id: r.original_intent_id,
        effective_constraints_id: r.effective_constraints_id,
        capability_catalog_version: r.capability_catalog_version,
        principal_projection: serde_json::from_value(r.principal_projection_json)?,
        reference_instant: r.reference_instant,
        timezone: r.timezone,
        selected_capability_id: r.selected_capability_id,
        normalized_parameters: r.normalized_parameters_json,
        created_at: r.created_at,
    })
}
