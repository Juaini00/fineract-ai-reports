use axum::{Router, routing::get};

use crate::api::ChatAppState;
use crate::api::handlers::management;

pub fn router() -> Router<ChatAppState> {
    Router::new()
        .route("/management/status", get(management::status))
        .route("/management/dashboard", get(management::dashboard))
        .route("/management/audit", get(management::audit))
        .route("/management/llm-usage", get(management::llm_usage))
        .route(
            "/management/audit/jobs/{job_id}",
            get(management::audit_job),
        )
        .route("/management/knowledge", get(management::knowledge))
        .route(
            "/management/knowledge/{id}",
            get(management::knowledge_detail),
        )
}
