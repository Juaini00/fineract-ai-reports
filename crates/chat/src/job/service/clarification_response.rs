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

pub fn validate_submission(
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
        ClarificationKind::SelectOption | ClarificationKind::SelectEntity => {
            let Some(option_id) = selected_option_id.as_deref() else {
                return Err(ClarificationValidationError::field("option_id"));
            };
            let Some(option) = payload.options.iter().find(|option| option.id == option_id) else {
                return Err(ClarificationValidationError::field("option_id"));
            };
            if payload.kind == ClarificationKind::SelectOption
                && option.id != OTHER_CLARIFICATION_OPTION_ID
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
            Some(value) => validate_field(field, value, &mut patch)?,
            None if field.required && field.default_value.is_none() => {
                return Err(ClarificationValidationError::field(format!(
                    "answers.{}",
                    field.key
                )));
            }
            None => {}
        }
    }
    let display_message = if payload.kind == ClarificationKind::SelectEntity {
        "Client selected".to_owned()
    } else if !source_message.is_empty() {
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
            // TODO(issue-003): map text answers only after their canonical constraint semantics are defined.
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assistant::{ClarificationFieldType, ClarificationOption, ClarificationValidation};
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
}
