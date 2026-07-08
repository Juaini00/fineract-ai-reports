use crate::knowledge::catalog::{loader::KnowledgeLoader, validator::KnowledgeValidator};
use crate::knowledge::index::sync::{catalog_content_hash, document_content_hash};
use crate::knowledge::retrieval::RetrievalDocumentBuilder;

#[test]
fn load_and_validate_project_knowledge_catalog() {
    let workspace_root = workspace_root();

    let catalog = KnowledgeLoader::new(
        workspace_root.join("knowledge"),
        workspace_root.join("queries"),
    )
    .load()
    .expect("Load Knowledge catalog");

    KnowledgeValidator::validate(&catalog).expect("Validate Knowledge catalog");

    assert!(!catalog.data_areas.is_empty());
    assert!(!catalog.domains.is_empty());
    assert!(!catalog.schemas.is_empty());
    assert!(!catalog.metrics.is_empty());
    assert!(!catalog.capabilities.is_empty());
    assert!(!catalog.queries.is_empty());
    assert!(!catalog.policies.is_empty());
    assert!(!catalog.responses.is_empty());
}

#[test]
fn builds_retrieval_documents_from_valid_catalog() {
    let workspace_root = workspace_root();

    let catalog = KnowledgeLoader::new(
        workspace_root.join("knowledge"),
        workspace_root.join("queries"),
    )
    .load()
    .expect("Load Knowledge catalog");

    KnowledgeValidator::validate(&catalog).expect("Validate Knowledge catalog");

    let documents = RetrievalDocumentBuilder::build(&catalog);

    assert!(!documents.is_empty());

    let capability_doc = documents
        .iter()
        .find(|document| document.source_id == "savings_deposit_total")
        .expect("savings_deposit_total retrieval document");

    assert!(
        capability_doc
            .retrieval_text
            .contains("Capability savings_deposit_total")
    );
    assert!(capability_doc.retrieval_text.contains("Domain savings"));
    assert!(
        capability_doc
            .retrieval_text
            .contains("Query savings.deposit_total")
    );

    assert!(
        documents
            .iter()
            .any(|document| document.source_id == "savings.deposit_amount")
    );
    assert!(
        documents
            .iter()
            .any(|document| document.source_id == "pii_policy")
    );
    assert!(
        documents
            .iter()
            .any(|document| document.source_id == "reporting_responses")
    );
}

fn workspace_root() -> std::path::PathBuf {
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));

    manifest_dir
        .parent()
        .and_then(|path| path.parent())
        .expect("workspace root")
        .to_path_buf()
}

#[test]
fn retrieval_document_hash_is_deterministic() {
    let document = crate::knowledge::retrieval::RetrievalDocument {
        source_type: crate::knowledge::retrieval::RetrievalSourceType::Capability,
        source_id: "savings_deposit_total".to_string(),
        title: "Capability savings_deposit_total".to_string(),
        retrieval_text: "Capability savings_deposit_total\nDomain savings".to_string(),
        metadata_json: serde_json::json!({
            "domain": "savings",
            "query_id": "savings.deposit_total"
        }),
    };

    assert_eq!(
        document_content_hash(&document),
        document_content_hash(&document)
    );
}

#[test]
fn catalog_hash_changes_when_retrieval_text_changes() {
    let mut documents = vec![crate::knowledge::retrieval::RetrievalDocument {
        source_type: crate::knowledge::retrieval::RetrievalSourceType::Query,
        source_id: "savings.deposit_total".to_string(),
        title: "Query savings.deposit_total".to_string(),
        retrieval_text: "Query savings.deposit_total".to_string(),
        metadata_json: serde_json::json!({"database": "fineract"}),
    }];

    let before = catalog_content_hash(&documents);
    documents[0].retrieval_text.push_str(" updated");

    assert_ne!(before, catalog_content_hash(&documents));
}

#[test]
fn classification_policy_reads_lqr_block_with_defaults() {
    let yaml = r#"
min_gap: 0.05
min_floor: 0.40
others_key: other_activity
others_label: "Others"
lqr:
  domain_min_floor: 0.55
  domain_min_gap: 0.10
  retry_budget: 2
  score_aggregation: min
"#;

    let policy: crate::knowledge::model::ClassificationPolicy =
        serde_yaml::from_str(yaml).expect("parse policy");

    assert_eq!(policy.lqr.domain_min_floor, 0.55);
    assert_eq!(policy.lqr.domain_min_gap, 0.10);
    assert_eq!(policy.lqr.capability_min_floor, 0.40);
    assert_eq!(policy.lqr.capability_min_gap, 0.05);
    assert_eq!(policy.lqr.retry_budget, 2);
    assert!(matches!(
        policy.lqr.score_aggregation,
        crate::knowledge::model::ScoreAggregation::Min
    ));
}

#[test]
fn classification_policy_defaults_lqr_when_missing() {
    let yaml = r#"
min_gap: 0.05
min_floor: 0.40
others_key: other_activity
others_label: "Others"
"#;

    let policy: crate::knowledge::model::ClassificationPolicy =
        serde_yaml::from_str(yaml).expect("parse policy");

    assert_eq!(policy.lqr.domain_min_floor, 0.55);
    assert_eq!(policy.lqr.retry_budget, 2);
}
