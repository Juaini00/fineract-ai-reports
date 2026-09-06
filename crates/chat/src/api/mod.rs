use std::sync::Arc;

use app_core::api::AppState;
use app_core::auth::service::AuthService;
use axum::{Router, extract::FromRef};

use crate::assistant::llm::planner_client::LlmPlannerClient;
use crate::assistant::temporal::{
    AuditingBusinessDateProvider, BusinessDateProvider, FineractBusinessDateProvider,
};
use crate::audit::spawn_audit_worker;
use crate::conversation::repository::{MessageRepository, SessionRepository};
use crate::conversation::service::{MessageService, SessionService};
use crate::job::{JobRepository, JobService};
use crate::knowledge::catalog::{
    loader::KnowledgeLoader,
    validator::{KnowledgeValidator, validate_runtime},
};
use crate::knowledge::embedding::VoyageEmbeddingClient;
use crate::knowledge::index::sync::KnowledgeSyncService;
use crate::knowledge::model::KnowledgeCatalog;
use crate::management::outbox::spawn_outbox_dispatcher;

pub mod dto;
pub mod handlers;
pub mod routes;

#[derive(Clone)]
pub struct ChatServices {
    pub sessions: SessionService,
    pub messages: MessageService,
    pub jobs: JobService,
}

#[derive(Clone)]
pub struct ChatAppState {
    pub core: AppState,
    pub chat: ChatServices,
    pub catalog: Arc<KnowledgeCatalog>,
}

impl ChatAppState {
    pub async fn new(core: AppState) -> anyhow::Result<Self> {
        // The vector index must track the on-disk catalog: a stale index makes
        // newly approved capabilities unreachable by embedding retrieval while
        // still appearing authorized. `sync_if_stale` no-ops when hashes match,
        // so this costs one catalog load per boot, not a re-embed.
        {
            let loader =
                KnowledgeLoader::new(&core.config.catalog.path, &core.config.catalog.query_path);
            let embedding_client = VoyageEmbeddingClient::new(&core.config.voyage_ai)?;
            let sync = KnowledgeSyncService::with_embeddings(
                loader,
                core.pools.app.clone(),
                embedding_client,
                core.config.voyage_ai.embedding_model.clone(),
                core.config.voyage_ai.embedding_dimensions,
            );
            let summary = if core.config.catalog.sync_on_startup {
                Some(sync.sync().await?)
            } else {
                sync.sync_if_stale().await?
            };
            match summary {
                Some(summary) => tracing::info!(
                    catalog_version_id = %summary.catalog_version_id,
                    document_count = summary.document_count,
                    embedding_model = summary.embedding_model.as_deref().unwrap_or("none"),
                    "knowledge catalog synced"
                ),
                None => tracing::debug!("knowledge catalog index already current"),
            }
        }

        let catalog =
            KnowledgeLoader::new(&core.config.catalog.path, &core.config.catalog.query_path)
                .load()?;
        KnowledgeValidator::validate(&catalog)?;
        if core.config.catalog.validate_on_startup {
            validate_runtime(&catalog, &core.pools.fineract).await?;
            tracing::info!("approved SQL validated against fineract at startup");
        } else {
            tracing::debug!(
                "skipping runtime SQL validation at startup (CATALOG_VALIDATE_ON_STARTUP=false)"
            );
        }
        let catalog = Arc::new(catalog);

        let pool = core.pools.app.clone();
        let session_repo = SessionRepository::new(pool.clone());
        let message_repo = MessageRepository::new(pool.clone());
        let job_repo = JobRepository::new(pool, session_repo.clone(), message_repo.clone());
        let runtime_embedding_client = VoyageEmbeddingClient::new(&core.config.voyage_ai)?;
        let llm_planner = LlmPlannerClient::new(&core.config.llm)?;
        let audit = spawn_audit_worker(core.pools.app.clone());
        spawn_outbox_dispatcher(core.pools.app.clone());
        let business_date: Arc<dyn BusinessDateProvider> =
            Arc::new(AuditingBusinessDateProvider::new(
                FineractBusinessDateProvider::new(core.pools.fineract.clone()),
                core.pools.app.clone(),
            ));

        let chat = ChatServices {
            sessions: SessionService::new(session_repo),
            messages: MessageService::new(message_repo.clone()),
            jobs: JobService::new(
                job_repo,
                message_repo.clone(),
                core.pools.app.clone(),
                core.pools.fineract.clone(),
                catalog.clone(),
                runtime_embedding_client,
                llm_planner,
                core.config.llm.clone(),
                core.config.embedding.clone(),
                core.config.chat_features.clone(),
                core.config.query.clone(),
                core.config.redis.url.clone(),
                core.pools.redis.clone(),
                audit,
                business_date,
            ),
        };

        Ok(Self {
            core,
            chat,
            catalog,
        })
    }
}

impl FromRef<ChatAppState> for AuthService {
    fn from_ref(state: &ChatAppState) -> Self {
        state.core.auth_service.clone()
    }
}

pub fn router(state: ChatAppState) -> Router {
    Router::new()
        .merge(routes::session::router())
        .merge(routes::job::router())
        .merge(routes::catalog::router())
        .merge(routes::management::router())
        .with_state(state)
}
