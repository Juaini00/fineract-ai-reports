use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct ValidateCatalogResponse {
    pub valid: bool,
    pub data_areas: usize,
    pub domains: usize,
    pub capabilities: usize,
    pub queries: usize,
}

#[derive(Debug, Serialize)]
pub struct CatalogCapabilityItem {
    pub id: String,
    pub status: String,
    pub domain: String,
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub data_areas: Vec<String>,
    pub required_parameters: Vec<String>,
    pub optional_parameters: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct CatalogCapabilitiesResponse {
    pub allowed_capabilities: Vec<String>,
    pub capabilities: Vec<CatalogCapabilityItem>,
}
