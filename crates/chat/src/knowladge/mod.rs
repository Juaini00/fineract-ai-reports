pub mod loader;
pub mod model;
pub mod validator;

#[cfg(test)]
mod tests {
    use crate::knowladge::{loader::KnowledgeLoader, validator::KnowledgeValidator};

    #[test]
    fn load_and_validate_project_knowledge_catalog() {
        let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let workspace_root = manifest_dir
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");

        let catalog = KnowledgeLoader::new(
            workspace_root.join("knowledge"),
            workspace_root.join("queries"),
        )
        .load()
        .expect("Load Knowledge catalog");

        KnowledgeValidator::validate(&catalog).expect("Validate Knowledge catalog");

        assert!(!catalog.data_areas.is_empty());
        assert!(!catalog.domains.is_empty());
        assert!(!catalog.capabilities.is_empty());
        assert!(!catalog.queries.is_empty());
    }
}
