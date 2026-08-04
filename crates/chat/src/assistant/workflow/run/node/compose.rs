use anyhow::{Result, bail};
use serde_json::Value;

use crate::assistant::workflow::Composition;

/// Composition is deterministic: callers supply only policy-filtered typed
/// outputs from completed node runs, never a model response or raw SQL rows.
pub fn compose(mode: Composition, sources: Vec<Value>) -> Result<Value> {
    match mode {
        Composition::Single if sources.len() == 1 => {
            Ok(sources.into_iter().next().expect("one source checked"))
        }
        Composition::Comparison | Composition::Grouped if !sources.is_empty() => {
            Ok(serde_json::json!({ "sources": sources }))
        }
        _ => bail!("composition sources do not satisfy the workflow contract"),
    }
}
