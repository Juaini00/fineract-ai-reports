use anyhow::{Result, bail};
use serde_json::Value;

/// Validates the already-approved repository response before it becomes a
/// typed node output. SQL itself remains in `execution::repository`.
pub fn typed_query_output(result: Value, row_cap: u32) -> Result<Value> {
    let rows = result
        .get("rows")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow::anyhow!("approved query result has no rows"))?;
    if rows.len() > row_cap as usize {
        bail!("approved query exceeded node row cap");
    }
    Ok(result)
}
