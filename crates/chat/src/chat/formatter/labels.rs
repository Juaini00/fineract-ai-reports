use std::collections::HashMap;

use serde_json::Value;

use crate::knowledge::model::KnowledgeCatalog;

pub struct ResponseText {
    empty_result: String,
    field_labels: HashMap<String, String>,
}

impl ResponseText {
    pub fn from_catalog(catalog: &KnowledgeCatalog) -> Self {
        let Some(response) = catalog
            .responses
            .iter()
            .find(|response| response.id == "reporting_responses")
        else {
            return Self::default();
        };

        let empty_result = response
            .content
            .get("templates")
            .and_then(|value| value.get("empty_result"))
            .and_then(Value::as_str)
            .unwrap_or("No data was found for the requested parameters.")
            .to_string();
        let field_labels = response
            .content
            .get("field_labels")
            .and_then(Value::as_object)
            .map(|labels| {
                labels
                    .iter()
                    .filter_map(|(key, value)| {
                        value.as_str().map(|label| (key.clone(), label.to_string()))
                    })
                    .collect()
            })
            .unwrap_or_default();

        Self {
            empty_result,
            field_labels,
        }
    }

    pub fn empty_result(&self) -> String {
        self.empty_result.clone()
    }

    pub fn field_label(&self, field: &str) -> String {
        self.field_labels
            .get(field)
            .cloned()
            .unwrap_or_else(|| fallback_label(field))
    }
}

impl Default for ResponseText {
    fn default() -> Self {
        Self {
            empty_result: "No data was found for the requested parameters.".to_string(),
            field_labels: HashMap::new(),
        }
    }
}

fn fallback_label(field: &str) -> String {
    let mut label = field.replace('_', " ");
    if let Some(first) = label.get_mut(0..1) {
        first.make_ascii_uppercase();
    }
    label
}
