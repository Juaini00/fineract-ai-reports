use std::path::PathBuf;

use serde::Deserialize;
use swiftide::indexing::{EmbedMode, Metadata, TextNode};

#[derive(Debug, Deserialize)]
struct CapabilitySnippet {
    id: String,
    domain: String,
    title: String,
    description: String,
    query: String,
}

#[derive(Debug, PartialEq, Eq)]
struct CurrentKnowledgeShape {
    source: String,
    content: String,
    metadata_keys: Vec<String>,
}

fn main() -> anyhow::Result<()> {
    let snippets = [
        r#"
id: savings.balance.summary
domain: savings
title: Savings balance summary
description: Summarize current savings balances for authorized offices.
query: savings_balance_summary.sql
"#,
        r#"
id: client.active.count
domain: client
title: Active client count
description: Count active clients in authorized offices.
query: client_active_count.sql
"#,
    ];

    let current: Vec<_> = snippets
        .iter()
        .map(|yaml| -> anyhow::Result<_> {
            let capability: CapabilitySnippet = serde_yaml::from_str(yaml)?;
            Ok(CurrentKnowledgeShape {
                source: capability.id.clone(),
                content: format!(
                    "{}\n{}\n{}",
                    capability.title, capability.description, capability.query
                ),
                metadata_keys: vec!["domain".to_string(), "query".to_string()],
            })
        })
        .collect::<anyhow::Result<_>>()?;

    let swiftide: Vec<_> = snippets
        .iter()
        .map(|yaml| -> anyhow::Result<_> {
            let capability: CapabilitySnippet = serde_yaml::from_str(yaml)?;
            let mut metadata = Metadata::default();
            metadata.insert("domain", capability.domain);
            metadata.insert("query", capability.query.clone());

            let mut node = TextNode::new(format!(
                "{}\n{}\n{}",
                capability.title, capability.description, capability.query
            ));
            node.path = PathBuf::from(&capability.id);
            node.embed_mode = EmbedMode::SingleWithMetadata;
            node.with_metadata(metadata);

            Ok(CurrentKnowledgeShape {
                source: node.path.display().to_string(),
                content: node.as_embeddables().remove(0).1,
                metadata_keys: node.metadata.keys().map(str::to_string).collect(),
            })
        })
        .collect::<anyhow::Result<_>>()?;

    assert_eq!(current.len(), swiftide.len());
    assert!(
        swiftide
            .iter()
            .all(|shape| shape.content.contains("domain:"))
    );
    assert!(
        swiftide
            .iter()
            .all(|shape| shape.content.contains("query:"))
    );
    println!("swiftide Phase 0 node shape: {swiftide:#?}");
    Ok(())
}
