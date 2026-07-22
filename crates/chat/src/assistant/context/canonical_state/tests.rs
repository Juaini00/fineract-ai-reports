use super::*;

use chrono::{DateTime, Utc};
use std::collections::BTreeMap;
use uuid::Uuid;

fn fact(
    sequence: i64,
    source: FactSourceKind,
    source_id: &str,
    field: ConstraintField,
    value: TypedFactValue,
) -> FactObservation {
    FactObservation {
        id: Uuid::from_u128(sequence as u128 + 1),
        job_id: Uuid::from_u128(1),
        sequence,
        source_kind: source,
        source_id: source_id.into(),
        field_path: field,
        typed_value: value,
        confidence: Some(1.0),
        extractor_version: "test".into(),
        observed_at: DateTime::<Utc>::UNIX_EPOCH,
    }
}

#[test]
fn precedence_clear_and_list_algebra() {
    let observations = vec![
        fact(
            1,
            FactSourceKind::ApprovedDefault,
            "d",
            ConstraintField::Metric,
            TypedFactValue::Metric("balance".into()),
        ),
        fact(
            2,
            FactSourceKind::OriginalRequest,
            "r",
            ConstraintField::Metric,
            TypedFactValue::Metric("sum".into()),
        ),
        fact(
            3,
            FactSourceKind::Clarification,
            "c1",
            ConstraintField::Metric,
            TypedFactValue::Metric("count".into()),
        ),
        fact(
            4,
            FactSourceKind::Clarification,
            "c1",
            ConstraintField::ProductIds,
            TypedFactValue::IdList(ListPatch::Replace(vec![1, 2])),
        ),
        fact(
            5,
            FactSourceKind::Clarification,
            "c2",
            ConstraintField::ProductIds,
            TypedFactValue::IdList(ListPatch::Add(vec![2, 3])),
        ),
        fact(
            6,
            FactSourceKind::Clarification,
            "c3",
            ConstraintField::ProductIds,
            TypedFactValue::IdList(ListPatch::Remove(vec![1])),
        ),
    ];
    for input in [
        observations.clone(),
        observations.iter().rev().cloned().collect(),
    ] {
        let result = merge_observations(
            Uuid::from_u128(1),
            1,
            &input,
            &executable_constraint_contracts(),
        )
        .unwrap();
        assert_eq!(
            result.values[&ConstraintField::Metric],
            TypedFactValue::Metric("count".into())
        );
        assert_eq!(
            result.values[&ConstraintField::ProductIds],
            TypedFactValue::IdList(ListPatch::Replace(vec![2, 3]))
        );
        assert_eq!(
            result.winning_observation_ids[&ConstraintField::Metric],
            observations[2].id
        );
    }
}

#[test]
fn replay_clear_and_patch_contracts() {
    let contracts = executable_constraint_contracts();
    let a = fact(
        1,
        FactSourceKind::Clarification,
        "same",
        ConstraintField::Metric,
        TypedFactValue::Metric("a".into()),
    );
    let duplicate = FactObservation {
        id: Uuid::from_u128(9),
        sequence: 2,
        ..a.clone()
    };
    assert!(merge_observations(a.job_id, 1, &[a.clone(), duplicate], &contracts).is_ok());
    let conflict = fact(
        2,
        FactSourceKind::Clarification,
        "same",
        ConstraintField::Metric,
        TypedFactValue::Metric("b".into()),
    );
    assert!(matches!(
        merge_observations(a.job_id, 1, &[a, conflict], &contracts),
        Err(MergeError::ConflictingReplay(_))
    ));
    let clear = fact(
        1,
        FactSourceKind::Clarification,
        "c",
        ConstraintField::Domain,
        TypedFactValue::Clear,
    );
    assert_eq!(
        merge_observations(clear.job_id, 1, &[clear], &contracts),
        Err(MergeError::ClearRejected(ConstraintField::Domain))
    );
    let patch = BTreeMap::from([(
        ConstraintField::Metric,
        TypedFactValue::Metric("count".into()),
    )]);
    assert_eq!(
        observations_from_patch(Uuid::nil(), "c", 1, &patch, Utc::now(), &contracts)
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn approved_defaults_are_explicit_and_clarifications_override_them() {
    let contracts = executable_constraint_contracts();
    let defaults = BTreeMap::from([(ConstraintField::LimitValue, TypedFactValue::Integer(10))]);
    let clarification = BTreeMap::from([
        (
            ConstraintField::FromDate,
            TypedFactValue::Date("2026-01-01".into()),
        ),
        (ConstraintField::LimitValue, TypedFactValue::Integer(25)),
    ]);
    let defaults = approved_default_observations(
        Uuid::nil(),
        "approved_default:client_top_n",
        1,
        &defaults,
        Utc::now(),
        &contracts,
    )
    .unwrap();
    assert_eq!(defaults[0].source_kind, FactSourceKind::ApprovedDefault);
    let clarification = observations_from_patch(
        Uuid::nil(),
        "clarification:answer",
        2,
        &clarification,
        Utc::now(),
        &contracts,
    )
    .unwrap();
    let effective = merge_observations(
        Uuid::nil(),
        1,
        &[
            defaults[0].clone(),
            clarification[0].clone(),
            clarification[1].clone(),
        ],
        &contracts,
    )
    .unwrap();
    assert!(
        clarification
            .iter()
            .all(|observation| observation.source_kind == FactSourceKind::Clarification)
    );
    assert_eq!(
        effective.values[&ConstraintField::LimitValue],
        TypedFactValue::Integer(25)
    );
    assert_eq!(
        effective.values[&ConstraintField::FromDate],
        TypedFactValue::Date("2026-01-01".into())
    );
}
