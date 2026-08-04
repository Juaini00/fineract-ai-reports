use anyhow::{Result, bail};

use crate::assistant::workflow::{Cardinality, NodeId, NodeRunStatus, WorkflowNodeRun};

pub fn cardinality_for(runs: &[WorkflowNodeRun], source: &NodeId) -> Result<Cardinality> {
    let run = runs
        .iter()
        .rev()
        .find(|run| &run.node_id == source && run.status == NodeRunStatus::Completed)
        .ok_or_else(|| anyhow::anyhow!("branch source has not completed"))?;
    // `source` is either the data node a branch decides on (row-count output)
    // or, when called for an edge leaving a `CardinalityBranch` node itself,
    // the branch node's own completed run — whose output is its already-made
    // `{"cardinality": ...}` decision, not a row count. Prefer the decision
    // when present so an edge condition re-derives the same answer the branch
    // recorded, instead of misreading the branch node's `rows_returned` (which
    // is always 0) as a row count of zero.
    if let Some(decision) = run
        .output_json
        .as_ref()
        .and_then(|output| output.get("cardinality"))
        .and_then(serde_json::Value::as_str)
    {
        return match decision {
            "zero" => Ok(Cardinality::Zero),
            "one" => Ok(Cardinality::One),
            "many" => Ok(Cardinality::Many),
            _ => bail!("branch cardinality decision is invalid"),
        };
    }
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
