use serde::Serialize;

#[derive(Serialize)]
pub(crate) struct HealthResponse {
    pub(crate) status: &'static str,
}

#[derive(Serialize)]
pub(crate) struct ReadyResponse {
    pub(crate) status: &'static str,
    pub(crate) checks: crate::db::ReadinessChecks,
}
