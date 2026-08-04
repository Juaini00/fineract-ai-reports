use anyhow::{Result, bail};

use crate::assistant::workflow::{Cardinality, NodeId, NodeRunStatus, WorkflowNodeRun};

pub fn cardinality_for(runs: &[WorkflowNodeRun], source: &NodeId) -> Result<Cardinality> {
    let run = runs
        .iter()
        .rev()
        .find(|run| &run.node_id == source && run.status == NodeRunStatus::Completed)
        .ok_or_else(|| anyhow::anyhow!("branch source has not completed"))?;
    let rows = run
        .output_json
        .as_ref()
        .and_then(|output| output.get("row_count").and_then(serde_json::Value::as_i64))
        .unwrap_or(i64::from(run.rows_returned));
    match rows {
        0 => Ok(Cardinality::Zero),
        1 => Ok(Cardinality::One),
        _ if rows > 1 => Ok(Cardinality::Many),
        _ => bail!("branch row count is invalid"),
    }
}
