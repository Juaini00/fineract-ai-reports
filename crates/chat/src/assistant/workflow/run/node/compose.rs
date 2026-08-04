use anyhow::{Result, bail};
use serde_json::Value;

use crate::assistant::workflow::verify::sensitivity_rank;
use crate::assistant::workflow::{Composition, NodeOutputSlot};
use crate::knowledge::model::Sensitivity;

/// One source node's completed output plus the sensitivity declared for each
/// of its named output fields, so composition can drop what the principal
/// isn't allowed to see before merging.
pub struct ComposeSource {
    pub output: Value,
    pub output_slots: Vec<NodeOutputSlot>,
}

pub struct ComposeOutcome {
    pub value: Value,
    /// Field names dropped for exceeding `max_visible`, never their values —
    /// this is what the caller records in audit.
    pub dropped_fields: Vec<String>,
}

/// Composition is deterministic: callers supply only policy-filtered typed
/// outputs from completed node runs, never a model response or raw SQL rows.
pub fn compose(
    mode: Composition,
    sources: Vec<ComposeSource>,
    max_visible: Sensitivity,
) -> Result<ComposeOutcome> {
    let mut dropped_fields = Vec::new();
    let filtered: Vec<Value> = sources
        .into_iter()
        .map(|source| filter_sensitivity(source, max_visible, &mut dropped_fields))
        .collect();
    let value = match mode {
        Composition::Single if filtered.len() == 1 => {
            filtered.into_iter().next().expect("one source checked")
        }
        Composition::Comparison | Composition::Grouped if !filtered.is_empty() => {
            serde_json::json!({ "sources": filtered })
        }
        _ => bail!("composition sources do not satisfy the workflow contract"),
    };
    Ok(ComposeOutcome {
        value,
        dropped_fields,
    })
}

fn filter_sensitivity(
    source: ComposeSource,
    max_visible: Sensitivity,
    dropped: &mut Vec<String>,
) -> Value {
    let Value::Object(mut object) = source.output else {
        return source.output;
    };
    for slot in &source.output_slots {
        if sensitivity_rank(slot.sensitivity) > sensitivity_rank(max_visible)
            && object.remove(&slot.name).is_some()
        {
            dropped.push(slot.name.clone());
        }
    }
    Value::Object(object)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assistant::workflow::Cardinality;
    use crate::knowledge::catalog::parameter_policy::ParameterType;

    fn slot(name: &str, sensitivity: Sensitivity) -> NodeOutputSlot {
        NodeOutputSlot {
            name: name.into(),
            kind: ParameterType::String,
            sensitivity,
            cardinality: Cardinality::One,
        }
    }

    #[test]
    fn compose_drops_fields_above_visible_sensitivity_and_reports_them() {
        let source = ComposeSource {
            output: serde_json::json!({ "office_name": "Nairobi", "national_id": "SECRET" }),
            output_slots: vec![
                slot("office_name", Sensitivity::PublicBusiness),
                slot("national_id", Sensitivity::Pii),
            ],
        };
        let outcome = compose(Composition::Single, vec![source], Sensitivity::FilterOnly).unwrap();
        assert_eq!(
            outcome.value,
            serde_json::json!({ "office_name": "Nairobi" })
        );
        assert_eq!(outcome.dropped_fields, vec!["national_id".to_string()]);
    }

    #[test]
    fn compose_keeps_pii_fields_when_principal_may_view_pii() {
        let source = ComposeSource {
            output: serde_json::json!({ "national_id": "SECRET" }),
            output_slots: vec![slot("national_id", Sensitivity::Pii)],
        };
        let outcome = compose(Composition::Single, vec![source], Sensitivity::Pii).unwrap();
        assert_eq!(
            outcome.value,
            serde_json::json!({ "national_id": "SECRET" })
        );
        assert!(outcome.dropped_fields.is_empty());
    }
}
