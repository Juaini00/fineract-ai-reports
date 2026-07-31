//! Phase A acceptance: every approved capability, converted to a degenerate
//! dataset and composed, must reproduce its legacy SQL exactly. This is the
//! oracle that lets later phases merge datasets without guessing whether
//! behaviour changed.

use std::path::{Path, PathBuf};

use chat::knowledge::catalog::loader::KnowledgeLoader;
use chat::knowledge::dataset::compose::compose;
use chat::knowledge::dataset::legacy::{LEGACY_SHAPE_ID, degenerate_dataset};
use chat::knowledge::model::{KnowledgeCatalog, QueryKnowledge};

fn repo_root() -> PathBuf {
    // crates/chat -> repository root
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crate is two levels below the repository root")
        .to_path_buf()
}

fn load_catalog() -> KnowledgeCatalog {
    let root = repo_root();
    KnowledgeLoader::new(root.join("knowledge"), root.join("queries"))
        .load()
        .expect("catalog loads")
}

fn read_sql(query: &QueryKnowledge) -> String {
    let path = repo_root().join(&query.sql_file);
    std::fs::read_to_string(&path).unwrap_or_else(|err| panic!("read {}: {err}", path.display()))
}

#[test]
fn every_approved_capability_composes_to_its_legacy_sql() {
    let catalog = load_catalog();
    let approved: Vec<_> = catalog
        .capabilities
        .iter()
        .filter(|capability| capability.status == "approved_mvp")
        .collect();

    assert!(
        approved.len() >= 30,
        "expected the full approved catalog, found {}",
        approved.len()
    );

    for capability in approved {
        let query = catalog
            .queries
            .iter()
            .find(|query| query.id == capability.query_id)
            .unwrap_or_else(|| panic!("capability {} has no query", capability.id));

        let dataset = degenerate_dataset(capability, query);
        let source = read_sql(query);

        let composed = compose(&dataset, LEGACY_SHAPE_ID, None, &source, None)
            .unwrap_or_else(|err| panic!("compose {}: {err}", capability.id));

        assert_eq!(
            composed.sql, source,
            "capability {} composed SQL differs from its legacy SQL",
            capability.id
        );
        assert!(
            composed.filter_binds.is_empty(),
            "capability {} must bind no filters in Phase A",
            capability.id
        );
    }
}

#[test]
fn every_derived_dataset_keeps_the_full_output_contract() {
    let catalog = load_catalog();

    for capability in catalog
        .capabilities
        .iter()
        .filter(|capability| capability.status == "approved_mvp")
    {
        let query = catalog
            .queries
            .iter()
            .find(|query| query.id == capability.query_id)
            .unwrap_or_else(|| panic!("capability {} has no query", capability.id));

        let dataset = degenerate_dataset(capability, query);

        let declared: Vec<&str> = query
            .output_fields
            .iter()
            .map(|field| field.name.as_str())
            .collect();
        let core: Vec<String> = dataset.core_field_names();

        assert_eq!(
            core, declared,
            "capability {} must render every declared column in Phase A",
            capability.id
        );
        assert_eq!(dataset.parameters, query.parameters);
        assert_eq!(dataset.timeout_ms, query.timeout_ms);
    }
}
