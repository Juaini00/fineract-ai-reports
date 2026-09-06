use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogDocument {
    pub source_path: PathBuf,
    pub source_type: String,
    pub source_id: String,
    pub title: String,
    pub body: String,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Default, Clone)]
pub struct CatalogIndexPipeline;

impl CatalogIndexPipeline {
    pub fn ingest_paths(root: impl AsRef<Path>) -> Result<Vec<CatalogDocument>> {
        let root = root.as_ref();
        let mut documents = Vec::new();
        for path in walk(root)? {
            if !is_indexable(&path) {
                continue;
            }
            documents.extend(document_for(root, &path)?);
        }
        Ok(dedup(documents))
    }

    pub fn ingest_catalog(root: impl AsRef<Path>) -> Result<Vec<CatalogDocument>> {
        Self::ingest_paths(root)
    }
}

fn walk(root: &Path) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    if !root.exists() {
        return Ok(out);
    }
    for entry in fs::read_dir(root)? {
        let path = entry?.path();
        if path.is_dir() {
            out.extend(walk(&path)?);
        } else {
            out.push(path);
        }
    }
    Ok(out)
}

fn is_indexable(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|v| v.to_str()),
        Some("yaml" | "yml" | "sql" | "md")
    )
}

fn source_type(path: &Path) -> Option<&'static str> {
    let s = path.to_string_lossy();
    if s.contains("/capabilities/") {
        Some("capability")
    } else if s.contains("/queries/") || path.extension().and_then(|v| v.to_str()) == Some("sql") {
        Some("query")
    } else if s.contains("/domains/") {
        Some("domain")
    } else if s.contains("/schema/") {
        Some("schema")
    } else if s.contains("/metrics/") {
        Some("metric")
    } else if s.contains("/policies/") {
        Some("policy")
    } else if s.contains("/responses/") {
        Some("response")
    } else if s.contains("/docs/") || s.ends_with(".md") {
        Some("doc")
    } else {
        None
    }
}

fn document_for(root: &Path, path: &Path) -> Result<Vec<CatalogDocument>> {
    reject_client_rows(path)?;
    let Some(source_type) = source_type(path) else {
        return Ok(Vec::new());
    };
    let body = fs::read_to_string(path)?;
    let rel = path.strip_prefix(root).unwrap_or(path);
    let source_id = rel
        .with_extension("")
        .to_string_lossy()
        .replace(['/', '\\'], ".");
    let title = path
        .file_stem()
        .and_then(|v| v.to_str())
        .unwrap_or(&source_id)
        .replace('_', " ");
    Ok(chunks(&body)
        .into_iter()
        .enumerate()
        .map(|(i, body)| CatalogDocument {
            source_path: rel.to_path_buf(),
            source_type: source_type.into(),
            source_id: if i == 0 {
                source_id.clone()
            } else {
                format!("{source_id}#{i}")
            },
            title: title.clone(),
            body,
            metadata: serde_json::json!({"source_type": source_type, "chunk": i}),
        })
        .collect())
}

fn reject_client_rows(path: &Path) -> Result<()> {
    let s = path.to_string_lossy().to_lowercase();
    if ["fineract_rows", "export", "client_data", "row_data"]
        .iter()
        .any(|needle| s.contains(needle))
    {
        return Err(anyhow!(
            "catalog index rejects Fineract row/client export paths"
        ));
    }
    Ok(())
}

fn chunks(body: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    for line in body.lines() {
        if current.len() + line.len() > 1800 && !current.is_empty() {
            out.push(current.trim().to_string());
            current.clear();
        }
        current.push_str(line);
        current.push('\n');
    }
    if !current.trim().is_empty() {
        out.push(current.trim().to_string());
    }
    out
}

fn dedup(docs: Vec<CatalogDocument>) -> Vec<CatalogDocument> {
    let mut seen = HashSet::new();
    docs.into_iter()
        .filter(|doc| seen.insert(format!("{}:{}", doc.source_type, doc.body)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    /// SI-9: Fineract transactional rows are never indexed into vector storage.
    /// `reject_client_rows` refuses any path that looks like a client-row or
    /// export dump, while ordinary catalog knowledge paths pass through.
    #[test]
    fn reject_client_rows_refuses_fineract_row_and_export_paths() {
        for forbidden in [
            "knowledge/fineract_rows/clients.yaml",
            "data/export/clients.csv",
            "dump/client_data.json",
            "tmp/row_data.parquet",
        ] {
            assert!(
                reject_client_rows(Path::new(forbidden)).is_err(),
                "must reject Fineract row/export path: {forbidden}"
            );
        }
        // A normal catalog knowledge path is accepted.
        assert!(
            reject_client_rows(Path::new(
                "knowledge/capabilities/savings/activity_list.yaml"
            ))
            .is_ok()
        );
    }
}
