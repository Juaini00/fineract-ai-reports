use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct ValidateCatalogResponse {
    pub valid: bool,
    pub data_areas: usize,
    pub domains: usize,
    pub capabilities: usize,
    pub queries: usize,
}
