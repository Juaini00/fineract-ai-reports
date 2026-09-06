use std::collections::{BTreeMap, BTreeSet};

use app_core::auth::model::PrincipalContext;
use chrono::NaiveDate;
use serde_json::Value;
use uuid::Uuid;

use crate::{
    assistant::{
        ClarificationField, ClarificationFieldType, ClarificationKind, ClarificationPayload,
        ConstraintField, ConstraintPatch, LimitMode, OTHER_CLARIFICATION_OPTION_ID, TypedFactValue,
    },
    job::model::ValidatedClarificationSubmission,
    knowledge::model::KnowledgeCatalog,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClarificationValidationError {
    pub fields: Vec<String>,
}

impl ClarificationValidationError {
    fn field(field: impl Into<String>) -> Self {
        Self {
            fields: vec![field.into()],
        }
    }
}

/// Which canonical fact a free-text answer to `input_id` becomes.
///
/// A text clarification field always covers exactly one query parameter (the
/// input registry rejects anything else), so its first declared binding is the
/// field the answer belongs in.
fn text_target<'a>(catalog: &'a KnowledgeCatalog, input_id: &str) -> Option<&'a ConstraintField> {
    let input = catalog
        .parameter_inputs
        .iter()
        .find(|input| input.id == input_id)?;
    catalog.binding_fields(input.parameters.first()?).first()
}

/// Shape the typed value the way the field's own contract expects. Everything a
/// text clarification collects is a name or an exact literal, so the variant
/// follows from the field, not from parsing the answer.
fn text_fact(field: &ConstraintField, text: &str) -> TypedFactValue {
    let text = text.to_owned();
    match field {
        ConstraintField::Office => TypedFactValue::Office(text),
        ConstraintField::Product => TypedFactValue::Product(text),
        ConstraintField::ChargeType => TypedFactValue::ChargeType(text),
        ConstraintField::AccountNumber => TypedFactValue::AccountNumber(text),
        ConstraintField::CurrencyCode => TypedFactValue::CurrencyCode(text),
        ConstraintField::TransactionAmount => TypedFactValue::Decimal(text),
        _ => TypedFactValue::PersonName(text),
    }
}

pub fn validate_submission(
    catalog: &KnowledgeCatalog,
    payload: &ClarificationPayload,
    principal: &PrincipalContext,
    clarification_id: Option<Uuid>,
    clarification_revision: Option<u32>,
    option_id: Option<String>,
    message: Option<String>,
    answers: BTreeMap<String, Value>,
) -> Result<ValidatedClarificationSubmission, ClarificationValidationError> {
    let structured =
        clarification_id.is_some() || clarification_revision.is_some() || !answers.is_empty();
    let source_message = message.unwrap_or_default().trim().to_owned();
    let selected_option_id = option_id
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty());
    if !structured {
        if source_message.is_empty() {
            return Err(ClarificationValidationError::field("message"));
        }
        return Ok(ValidatedClarificationSubmission {
            clarification_id: None,
            clarification_revision: None,
            selected_option_id: selected_option_id.clone(),
            source_message: source_message.clone(),
            display_message: selected_option_id.unwrap_or(source_message),
            answers,
            constraint_patch: ConstraintPatch::new(),
        });
    }
    let Some(id) = clarification_id else {
        return Err(ClarificationValidationError::field("clarification_id"));
    };
    let Some(revision) = clarification_revision else {
        return Err(ClarificationValidationError::field(
            "clarification_revision",
        ));
    };
    // The repository compares this identity under its job-memory lock, preventing
    // a concurrent response from accepting a stale payload.

    let selected = match payload.kind {
        ClarificationKind::SelectOption => {
            let Some(option_id) = selected_option_id.as_deref() else {
                return Err(ClarificationValidationError::field("option_id"));
            };
            let Some(option) = payload.options.iter().find(|option| option.id == option_id) else {
                return Err(ClarificationValidationError::field("option_id"));
            };
            if option.id != OTHER_CLARIFICATION_OPTION_ID
                && !principal.capability_ids.iter().any(|id| id == &option.id)
            {
                return Err(ClarificationValidationError::field("option_id"));
            }
            Some(option)
        }
        _ => {
            if selected_option_id.is_some() {
                return Err(ClarificationValidationError::field("option_id"));
            }
            None
        }
    };
    if selected.is_some_and(|option| option.id == OTHER_CLARIFICATION_OPTION_ID)
        && source_message.is_empty()
    {
        return Err(ClarificationValidationError::field("message"));
    }

    let fields: Vec<&ClarificationField> = payload
        .fields
        .iter()
        .chain(selected.into_iter().flat_map(|option| option.fields.iter()))
        .collect();
    let offered: BTreeSet<_> = fields.iter().map(|field| field.key.as_str()).collect();
    if answers.keys().any(|key| !offered.contains(key.as_str())) {
        return Err(ClarificationValidationError::field("answers"));
    }
    let mut patch = ConstraintPatch::new();
    for field in fields {
        match answers.get(&field.key) {
            Some(value) => {
                validate_field(field, text_target(catalog, &field.key), value, &mut patch)?
            }
            None if field.required && field.default_value.is_none() => {
                return Err(ClarificationValidationError::field(format!(
                    "answers.{}",
                    field.key
                )));
            }
            None => {}
        }
    }
    let display_message = if !source_message.is_empty() {
        source_message.clone()
    } else if let Some(option) = selected {
        option.label.clone()
    } else {
        "Clarification response".to_owned()
    };
    Ok(ValidatedClarificationSubmission {
        clarification_id: Some(id),
        clarification_revision: Some(revision),
        selected_option_id,
        source_message,
        display_message,
        answers,
        constraint_patch: patch,
    })
}

fn validate_field(
    field: &ClarificationField,
    target: Option<&ConstraintField>,
    value: &Value,
    patch: &mut ConstraintPatch,
) -> Result<(), ClarificationValidationError> {
    match field.field_type {
        ClarificationFieldType::DateRange => {
            let Some(object) = value.as_object() else {
                return Err(ClarificationValidationError::field(format!(
                    "answers.{}",
                    field.key
                )));
            };
            if object.len() != 2 {
                return Err(ClarificationValidationError::field(format!(
                    "answers.{}",
                    field.key
                )));
            }
            let (Some(from), Some(to)) = (
                object.get("from").and_then(Value::as_str),
                object.get("to").and_then(Value::as_str),
            ) else {
                return Err(ClarificationValidationError::field(format!(
                    "answers.{}",
                    field.key
                )));
            };
            let (Ok(from_date), Ok(to_date)) = (
                NaiveDate::parse_from_str(from, "%Y-%m-%d"),
                NaiveDate::parse_from_str(to, "%Y-%m-%d"),
            ) else {
                return Err(ClarificationValidationError::field(format!(
                    "answers.{}",
                    field.key
                )));
            };
            if from_date > to_date
                || field
                    .validation
                    .max_range_days
                    .is_some_and(|max| (to_date - from_date).num_days() > i64::from(max))
            {
                return Err(ClarificationValidationError::field(format!(
                    "answers.{}",
                    field.key
                )));
            }
            patch.insert(
                ConstraintField::FromDate,
                TypedFactValue::Date(from.to_owned()),
            );
            patch.insert(ConstraintField::ToDate, TypedFactValue::Date(to.to_owned()));
        }
        ClarificationFieldType::Integer => {
            let Some(integer) = value.as_i64() else {
                return Err(ClarificationValidationError::field(format!(
                    "answers.{}",
                    field.key
                )));
            };
            if field
                .validation
                .min_integer
                .is_some_and(|min| integer < min)
                || field
                    .validation
                    .max_integer
                    .is_some_and(|max| integer > max)
            {
                return Err(ClarificationValidationError::field(format!(
                    "answers.{}",
                    field.key
                )));
            }
            if field.key == "limit" {
                patch.insert(
                    ConstraintField::LimitMode,
                    TypedFactValue::LimitMode(LimitMode::TopN),
                );
                patch.insert(
                    ConstraintField::LimitValue,
                    TypedFactValue::Integer(integer),
                );
            }
        }
        ClarificationFieldType::Text => {
            let Some(text) = value
                .as_str()
                .map(str::trim)
                .filter(|text| !text.is_empty())
            else {
                return Err(ClarificationValidationError::field(format!(
                    "answers.{}",
                    field.key
                )));
            };
            if field
                .validation
                .max_length
                .is_some_and(|max| text.chars().count() > max as usize)
            {
                return Err(ClarificationValidationError::field(format!(
                    "answers.{}",
                    field.key
                )));
            }
            // The answer has to become a fact, or the loop cannot terminate: the
            // next turn re-runs `input_satisfied`, finds nothing, and asks the
            // same question again. `field.key` is the parameter input id, and
            // the catalog says which constraint field that input's parameter
            // binds from — the first one, since a text answer is a single
            // value the user typed for this specific field.
            if let Some(field_path) = target {
                patch.insert(field_path.clone(), text_fact(field_path, text));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assistant::{ClarificationFieldType, ClarificationOption, ClarificationValidation};
    fn catalog() -> KnowledgeCatalog {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        crate::knowledge::catalog::loader::KnowledgeLoader::new(
            root.join("knowledge"),
            root.join("queries"),
        )
        .load()
        .unwrap()
    }
    fn principal() -> PrincipalContext {
        PrincipalContext {
            user_id: Uuid::new_v4(),
            role: "admin".into(),
            capability_ids: vec!["top_deposits".into()],
            office_ids: vec![],
            can_view_pii: true,
            legacy_api_key_id: None,
        }
    }
    fn payload() -> ClarificationPayload {
        ClarificationPayload {
            version: 1,
            id: Uuid::new_v4(),
            revision: 2,
            kind: ClarificationKind::SelectOption,
            question: "?".into(),
            options: vec![ClarificationOption {
                id: "top_deposits".into(),
                label: "Top deposits".into(),
                description: None,
                fields: vec![field("limit", ClarificationFieldType::Integer, true)],
            }],
            fields: vec![field("date_range", ClarificationFieldType::DateRange, true)],
            attempt: 0,
            source_intent: None,
            allow_free_text: false,
            is_missing_execution_parameters: true,
            workflow_id: None,
            node_id: None,
            resume_node_id: None,
            entity_kind: None,
        }
    }
    fn field(key: &str, field_type: ClarificationFieldType, required: bool) -> ClarificationField {
        ClarificationField {
            key: key.into(),
            label: key.into(),
            field_type,
            required,
            value: None,
            default_value: None,
            help_text: None,
            validation: ClarificationValidation {
                min_integer: Some(1),
                max_integer: Some(10),
                max_length: Some(5),
                max_range_days: Some(31),
            },
            errors: vec![],
        }
    }
    fn valid(
        p: &ClarificationPayload,
    ) -> Result<ValidatedClarificationSubmission, ClarificationValidationError> {
        validate_submission(
            &catalog(),
            p,
            &principal(),
            Some(p.id),
            Some(p.revision),
            Some("top_deposits".into()),
            None,
            serde_json::json!({"date_range":{"from":"2024-01-01","to":"2024-01-02"},"limit":5})
                .as_object()
                .unwrap()
                .clone()
                .into_iter()
                .collect(),
        )
    }
    #[test]
    fn maps_date_and_limit() {
        let out = valid(&payload()).unwrap();
        assert!(matches!(
            out.constraint_patch.get(&ConstraintField::LimitValue),
            Some(TypedFactValue::Integer(5))
        ));
        assert!(matches!(
            out.constraint_patch.get(&ConstraintField::FromDate),
            Some(TypedFactValue::Date(_))
        ));
    }
    #[test]
    fn rejects_partial_unknown_and_invalid_values() {
        let p = payload();
        assert!(
            validate_submission(
                &catalog(),
                &p,
                &principal(),
                Some(p.id),
                None,
                None,
                None,
                BTreeMap::new()
            )
            .is_err()
        );
        let mut bad = BTreeMap::new();
        bad.insert("unknown".into(), Value::Null);
        assert!(
            validate_submission(
                &catalog(),
                &p,
                &principal(),
                Some(p.id),
                Some(2),
                Some("top_deposits".into()),
                None,
                bad
            )
            .is_err()
        );
        for dates in [
            serde_json::json!({"from":"2024-02-02","to":"2024-01-01"}),
            serde_json::json!({"from":"2024-01-01","to":"2024-03-15"}),
        ] {
            let mut answers = BTreeMap::new();
            answers.insert("date_range".into(), dates);
            answers.insert("limit".into(), serde_json::json!(0));
            assert!(
                validate_submission(
                    &catalog(),
                    &p,
                    &principal(),
                    Some(p.id),
                    Some(2),
                    Some("top_deposits".into()),
                    None,
                    answers
                )
                .is_err()
            );
        }
    }
    #[test]
    fn rejects_missing_required_and_empty_other() {
        let p = payload();
        assert!(
            validate_submission(
                &catalog(),
                &p,
                &principal(),
                Some(p.id),
                Some(2),
                Some("top_deposits".into()),
                None,
                BTreeMap::new()
            )
            .is_err()
        );
    }

    /// The loop this closes: the assistant asks for a charge type, the user
    /// types one, and before this the answer was validated and thrown away —
    /// no ConstraintPatch, so the next turn asked the same question again.
    #[test]
    fn text_answer_becomes_a_constraint_so_the_loop_terminates() {
        let mut p = payload();
        p.kind = ClarificationKind::CollectFields;
        p.options = vec![];
        p.fields = vec![
            field("charge_name", ClarificationFieldType::Text, true),
            field("search", ClarificationFieldType::Text, true),
        ];
        let mut answers = BTreeMap::new();
        answers.insert("charge_name".into(), serde_json::json!("Fee"));
        answers.insert("search".into(), serde_json::json!("Tony"));

        let out = validate_submission(
            &catalog(),
            &p,
            &principal(),
            Some(p.id),
            Some(p.revision),
            None,
            None,
            answers,
        )
        .unwrap();

        assert_eq!(
            out.constraint_patch.get(&ConstraintField::ChargeType),
            Some(&TypedFactValue::ChargeType("Fee".into()))
        );
        assert_eq!(
            out.constraint_patch.get(&ConstraintField::PersonName),
            Some(&TypedFactValue::PersonName("Tony".into()))
        );
    }
}
