use anyhow::{Result, bail};
use serde_json::Value;

/// Resolver output is a bounded, typed projection. The runner persists this
/// value directly; callers must never pass raw provider rows here.
pub fn typed_resolver_output(rows: Vec<Value>, row_cap: u32) -> Result<Value> {
    if rows.len() > row_cap as usize {
        bail!("resolver output exceeds its approved row cap");
    }
    Ok(serde_json::json!({ "row_count": rows.len(), "rows": rows }))
}
