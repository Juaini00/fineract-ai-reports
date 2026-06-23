use std::sync::Arc;

use app_core::api::AppState;
use app_core::auth::service::AuthService;
use axum::{Router, extract::FromRef};

use crate::chat::repository::{JobRepository, MessageRepository, SessionRepository};
use crate::chat::service::{JobService, MessageService, SessionService};
use crate::knowledge::catalog::{loader::KnowledgeLoader, validator::KnowledgeValidator};
use crate::knowledge::embedding::VoyageEmbeddingClient;
use crate::knowledge::index::sync::KnowledgeSyncService;
use crate::knowledge::model::KnowledgeCatalog;

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
        if core.config.catalog.sync_on_startup {
            let loader =
                KnowledgeLoader::new(&core.config.catalog.path, &core.config.catalog.query_path);
            let embedding_client = VoyageEmbeddingClient::new(&core.config.voyage_ai)?;
            let summary = KnowledgeSyncService::with_embeddings(
                loader,
                core.pools.app.clone(),
                embedding_client,
                core.config.voyage_ai.embedding_model.clone(),
                core.config.voyage_ai.embedding_dimensions,
            )
            .sync()
            .await?;

            tracing::info!(
                catalog_version_id = %summary.catalog_version_id,
                document_count = summary.document_count,
                embedding_model = summary.embedding_model.as_deref().unwrap_or("none"),
                "knowledge catalog synced"
            );
        }

        let catalog =
            KnowledgeLoader::new(&core.config.catalog.path, &core.config.catalog.query_path)
                .load()?;
        KnowledgeValidator::validate(&catalog)?;
        let catalog = Arc::new(catalog);

        let pool = core.pools.app.clone();
        let session_repo = SessionRepository::new(pool.clone());
        let message_repo = MessageRepository::new(pool.clone());
        let job_repo = JobRepository::new(pool, session_repo.clone());
        let runtime_embedding_client = VoyageEmbeddingClient::new(&core.config.voyage_ai)?;

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
        .with_state(state)
}
