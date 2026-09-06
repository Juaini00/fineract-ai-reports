use std::{fs, path::PathBuf};

use chat::assistant::CatalogIndexPipeline;

#[test]
fn indexes_project_owned_sources_only() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let docs = CatalogIndexPipeline::ingest_catalog(&root).unwrap();
    assert!(docs.iter().any(|doc| doc.source_type == "capability"));
    assert!(docs.iter().any(|doc| doc.source_type == "query"));
    assert!(docs.iter().all(|doc| matches!(
        doc.source_type.as_str(),
        "capability" | "query" | "domain" | "schema" | "metric" | "policy" | "response" | "doc"
    )));
    assert!(
        docs.iter()
            .all(|doc| !doc.source_path.to_string_lossy().contains("fineract_rows"))
    );
}

#[test]
fn rejects_fineract_row_exports() {
    let root = std::env::temp_dir().join(format!("catalog-index-reject-{}", uuid::Uuid::new_v4()));
    let dir = root.join("fineract_rows");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("clients.yaml"), "client_id, account_no\n1, A1").unwrap();
    let error = CatalogIndexPipeline::ingest_paths(&root).unwrap_err();
    assert!(error.to_string().contains("rejects"));
    let _ = fs::remove_dir_all(root);
}
