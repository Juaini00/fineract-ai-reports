use std::fs::read_dir;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::knowledge::model::{
    CapabilityKnowledge, DataAreasKnowledge, DomainKnowledge, KnowledgeCatalog, QueryKnowledge,
};

pub struct KnowledgeLoader {
    root_path: PathBuf,
    query_path: PathBuf,
}

impl KnowledgeLoader {
    pub fn new(root_path: impl Into<PathBuf>, query_path: impl Into<PathBuf>) -> Self {
        Self {
            root_path: root_path.into(),
            query_path: query_path.into(),
        }
    }

    pub fn load(&self) -> Result<KnowledgeCatalog> {
        let data_areas = self.load_yaml_dir::<DataAreasKnowledge>("data-scope/areas")?;
        let domains = self.load_yaml_dir::<DomainKnowledge>("domains")?;
        let capabilities = self.load_yaml_dir::<CapabilityKnowledge>("capabilities")?;
        let queries = self.load_yaml_dir::<QueryKnowledge>("queries")?;

        Ok(KnowledgeCatalog {
            root_path: self.root_path.clone(),
            query_path: self.query_path.clone(),
            data_areas,
            domains,
            capabilities,
            queries,
        })
    }

    fn load_yaml_dir<T>(&self, relative_dir: &str) -> Result<Vec<T>>
    where
        T: serde::de::DeserializeOwned,
    {
        let dir = self.root_path.join(relative_dir);
        let mut items = Vec::new();

        if !dir.exists() {
            return Ok(items);
        }

        for path in collect_yaml_files(&dir)? {
            let content = std::fs::read_to_string(&path)
                .with_context(|| format!("failed to read {}", path.display()))?;
            let item = serde_yaml::from_str::<T>(&content)
                .with_context(|| format!("failed to parse {}", path.display()))?;
            items.push(item);
        }

        Ok(items)
    }
}

fn collect_yaml_files(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();

    collect_yaml_files_recursive(dir, &mut files)?;

    files.sort();
    Ok(files)
}

fn collect_yaml_files_recursive(dir: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    for entry in read_dir(dir).with_context(|| format!("failed to read {}", dir.display()))? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            collect_yaml_files_recursive(&path, files)?;
            continue;
        }

        if path.extension().and_then(|extension| extension.to_str()) == Some("yaml") {
            files.push(path);
        }
    }

    Ok(())
}
