use chrono::NaiveDate;

use super::*;

#[test]
fn extraction_quantity_currency_date_metric() {
    let extraction = extract_message_facts(
        "show top 10 clients with the most savings accounts in USD from 2026-01-01 to 2026-01-31",
    );

    assert_eq!(
        extraction.constraints.quantity,
        Some(Quantity::TopN { value: 10 })
    );
    assert_eq!(extraction.constraints.currency_code.as_deref(), Some("USD"));
    assert_eq!(
        extraction.constraints.from_date.as_deref(),
        Some("2026-01-01")
    );
    assert_eq!(
        extraction.constraints.to_date.as_deref(),
        Some("2026-01-31")
    );
    assert_eq!(
        extraction.constraints.metric.as_deref(),
        Some("savings_account_count")
    );
    assert!(extraction.candidates.iter().any(|candidate| {
        candidate.field == PayloadField::Limit && candidate.trust == PayloadTrust::Trusted
    }));
}

#[test]
fn extraction_merges_metric_when_absent() {
    let extraction = extract_message_facts("show 10 clients with the most savings accounts");
    let mut intent = AssistantIntent {
        intent: Default::default(),
        domain: AssistantDomain::Unknown,
        request_shape: Default::default(),
        language: crate::assistant::AssistantLanguage::En,
        entities: Vec::new(),
        constraints: Default::default(),
        context_reference: Default::default(),
        source: None,
        confidence: 0.0,
        reason: String::new(),
    };

    extraction.merge_into(&mut intent);

    assert_eq!(
        intent.constraints.quantity,
        Some(Quantity::TopN { value: 10 })
    );
    assert_eq!(
        intent.constraints.metric.as_deref(),
        Some("savings_account_count")
    );
    assert_eq!(intent.domain, AssistantDomain::Client);
    assert!(
        intent
            .entities
            .iter()
            .any(|entity| entity.entity_type == AssistantEntityType::Metric)
    );
}

#[test]
fn extracts_trusted_person_name() {
    let extraction = extract_message_facts("find client named Tony");

    assert!(extraction.entities.iter().any(|entity| {
        entity.entity_type == AssistantEntityType::PersonName && entity.value == "Tony"
    }));
    assert!(extraction.candidates.iter().any(|candidate| {
        candidate.field == PayloadField::PersonName && candidate.trust == PayloadTrust::Trusted
    }));
}

fn reference(value: &str) -> DateTime<Utc> {
    value.parse().unwrap()
}

#[test]
fn temporal_uses_jakarta_date_and_exact_period_boundaries() {
    let instant = reference("2026-01-01T17:30:00Z");
    let business_today = NaiveDate::from_ymd_opt(2026, 1, 2).unwrap();
    let today = extract_message_facts_at("show deposits today", instant, business_today, 366);
    assert_eq!(today.constraints.from_date.as_deref(), Some("2026-01-02"));
    assert_eq!(today.constraints.to_date.as_deref(), Some("2026-01-02"));
    assert_eq!(today.temporal_provenance.unwrap().timezone, "Asia/Jakarta");

    let year = extract_message_facts_at("laporan tahun ini", instant, business_today, 366);
    assert_eq!(year.constraints.from_date.as_deref(), Some("2026-01-01"));
    assert_eq!(year.constraints.to_date.as_deref(), Some("2026-12-31"));

    let week = extract_message_facts_at(
        "last week",
        reference("2026-03-11T12:00:00Z"),
        NaiveDate::from_ymd_opt(2026, 3, 11).unwrap(),
        366,
    );
    assert_eq!(week.constraints.from_date.as_deref(), Some("2026-03-02"));
    assert_eq!(week.constraints.to_date.as_deref(), Some("2026-03-08"));
}

#[test]
fn temporal_validates_dates_ranges_and_counts() {
    let instant = reference("2026-03-11T12:00:00Z");
    let business_today = NaiveDate::from_ymd_opt(2026, 3, 11).unwrap();
    let leap = extract_message_facts_at("2024-02-29", instant, business_today, 366);
    assert_eq!(leap.constraints.from_date, leap.constraints.to_date);
    assert!(
        extract_message_facts_at("2026-02-29", instant, business_today, 366)
            .temporal_error
            .is_some()
    );
    assert!(
        extract_message_facts_at(
            "from 2026-03-02 to 2026-03-01",
            instant,
            business_today,
            366
        )
        .temporal_error
        .is_some()
    );
    assert!(
        extract_message_facts_at("last 0 days", instant, business_today, 366)
            .temporal_error
            .is_some()
    );

    let range = extract_message_facts_at(
        "dari 2026-03-01 sampai 2026-03-03",
        instant,
        business_today,
        366,
    );
    assert_eq!(range.constraints.from_date.as_deref(), Some("2026-03-01"));
    assert_eq!(range.constraints.to_date.as_deref(), Some("2026-03-03"));
    let days = extract_message_facts_at("last 3 days", instant, business_today, 366);
    assert_eq!(days.constraints.from_date.as_deref(), Some("2026-03-09"));
    assert_eq!(days.constraints.to_date.as_deref(), Some("2026-03-11"));
    assert!(days.constraints.quantity.is_none());
}

#[test]
fn temporal_reuses_the_same_job_reference_after_clarification() {
    let job_reference = reference("2026-12-31T18:00:00Z");
    let business_today = NaiveDate::from_ymd_opt(2027, 1, 1).unwrap();
    let initial = extract_message_facts_at("today", job_reference, business_today, 366);
    let clarification = extract_message_facts_at("hari ini", job_reference, business_today, 366);

    assert_eq!(
        initial.constraints.from_date,
        clarification.constraints.from_date
    );
    assert_eq!(
        initial.constraints.to_date,
        clarification.constraints.to_date
    );
    assert_eq!(
        clarification.temporal_provenance.unwrap().reference_instant,
        job_reference
    );
}
