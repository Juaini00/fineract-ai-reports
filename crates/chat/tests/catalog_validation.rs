//! Pure integration test: no DB, no HTTP.
//! Loads the real `knowledge/` + `queries/` from the workspace and runs the
//! same validator startup uses. This is the fastest guardrail against any
//! YAML/SQL drift and doesn't need Postgres or Fineract.

use chat::knowledge::catalog::loader::KnowledgeLoader;
use chat::knowledge::catalog::validator::KnowledgeValidator;
use chat::knowledge::retrieval::RetrievalDocumentBuilder;

#[test]
fn real_catalog_loads_and_passes_validation() {
    // Arrange
    let workspace_root = workspace_root();

    // Act
    let catalog = KnowledgeLoader::new(
        workspace_root.join("knowledge"),
        workspace_root.join("queries"),
    )
    .load()
    .expect("load knowledge catalog");
    KnowledgeValidator::validate(&catalog).expect("validate knowledge catalog");

    // Assert — every runtime category must be populated
    assert!(!catalog.data_areas.is_empty(), "data_areas empty");
    assert!(!catalog.domains.is_empty(), "domains empty");
    assert!(!catalog.metrics.is_empty(), "metrics empty");
    assert!(!catalog.capabilities.is_empty(), "capabilities empty");
    assert!(!catalog.queries.is_empty(), "queries empty");
    assert!(!catalog.policies.is_empty(), "policies empty");
    assert!(!catalog.responses.is_empty(), "responses empty");
}

#[test]
fn every_approved_capability_maps_to_an_approved_query() {
    let workspace_root = workspace_root();
    let catalog = KnowledgeLoader::new(
        workspace_root.join("knowledge"),
        workspace_root.join("queries"),
    )
    .load()
    .expect("load knowledge catalog");
    KnowledgeValidator::validate(&catalog).expect("validate knowledge catalog");

    for capability in catalog
        .capabilities
        .iter()
        .filter(|c| c.status == "approved_mvp")
    {
        let query = catalog
            .queries
            .iter()
            .find(|q| q.id == capability.query_id)
            .unwrap_or_else(|| {
                panic!(
                    "approved capability {} references unknown query_id {}",
                    capability.id, capability.query_id
                )
            });
        assert_eq!(
            query.database, "fineract",
            "capability {} targets non-fineract database",
            capability.id
        );
    }
}

#[test]
fn retrieval_documents_cover_all_capabilities() {
    let workspace_root = workspace_root();
    let catalog = KnowledgeLoader::new(
        workspace_root.join("knowledge"),
        workspace_root.join("queries"),
    )
    .load()
    .expect("load knowledge catalog");
    KnowledgeValidator::validate(&catalog).expect("validate knowledge catalog");

    let documents = RetrievalDocumentBuilder::build(&catalog);
    assert!(!documents.is_empty());

    for capability in &catalog.capabilities {
        assert!(
            documents.iter().any(|d| d.source_id == capability.id),
            "capability {} missing from retrieval documents",
            capability.id
        );
    }
}

fn workspace_root() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root")
}
