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
        canonical_query_en: String::new(),
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

#[test]
fn adjacency_to_client_does_not_scavenge_a_filler_word() {
    let extraction = extract_message_facts("Search client by display name.");
    assert!(
        !extraction
            .entities
            .iter()
            .any(|e| e.entity_type == AssistantEntityType::PersonName),
        "no person name should be invented from adjacency: {:?}",
        extraction.entities
    );
}

#[test]
fn identifier_intake_redacts_savings_account_and_preserves_leading_zeroes() {
    let intake = identifier_intake("Who owns savings account number 0012-3456?");

    assert_eq!(
        intake.semantic_message(),
        "Who owns savings account number [SAVINGS_ACCOUNT_NUMBER]?"
    );
    assert_eq!(
        intake.sensitive_identifier().map(|value| value.expose()),
        Some("00123456")
    );
    assert_eq!(
        intake.sensitive_identifier().map(|value| value.kind()),
        Some(identifier::SensitiveIdentifierKind::SavingsAccountNumber)
    );
}

#[test]
fn identifier_intake_redacts_supported_indonesian_identifier_phrases() {
    for (message, placeholder, value) in [
        (
            "Siapa pemilik nomor rekening tabungan 0012-3456?",
            "[SAVINGS_ACCOUNT_NUMBER]",
            "00123456",
        ),
        (
            "Siapa pemilik nomor pinjaman 0000 7788?",
            "[LOAN_NUMBER]",
            "00007788",
        ),
    ] {
        let intake = identifier_intake(message);
        assert!(intake.semantic_message().contains(placeholder));
        assert_eq!(
            intake.sensitive_identifier().map(|item| item.expose()),
            Some(value)
        );
        assert!(!intake.semantic_message().contains(value));
    }
}

#[test]
fn identifier_intake_redacts_loan_number_without_activating_loan_domain() {
    let intake = identifier_intake("What are the terms for loan number 0000 7788?");

    assert_eq!(
        intake.semantic_message(),
        "What are the terms for loan number [LOAN_NUMBER]?"
    );
    assert_eq!(
        intake.sensitive_identifier().map(|value| value.expose()),
        Some("00007788")
    );
    assert_eq!(
        intake.sensitive_identifier().map(|value| value.kind()),
        Some(identifier::SensitiveIdentifierKind::LoanNumber)
    );
}

#[test]
fn identifier_intake_ignores_unlabelled_numbers_dates_and_amounts() {
    for message in [
        "Show the latest 10 transactions",
        "Show activity from 2026-01-01",
        "Find transactions for amount 00123456",
        "Find client id 00123456",
        "The savings account numbering policy changed to 00123456",
    ] {
        let intake = identifier_intake(message);
        assert_eq!(intake.semantic_message(), message);
        assert!(intake.sensitive_identifier().is_none());
    }
}

#[test]
fn deferred_loan_identifier_is_redacted_without_exposing_the_value() {
    let raw = "00007788";
    let intake = identifier_intake(&format!("Who owns loan number {raw}?"));
    let serialized_public_input = serde_json::json!({
        "message": intake.semantic_message(),
        "state": { "input": { "message": intake.semantic_message() } }
    })
    .to_string();

    assert_eq!(
        intake.sensitive_identifier().map(|value| value.kind()),
        Some(identifier::SensitiveIdentifierKind::LoanNumber)
    );
    assert!(!serialized_public_input.contains(raw));
    assert!(serialized_public_input.contains("[LOAN_NUMBER]"));
}

#[test]
fn identifier_intake_does_not_expose_secret_through_debug() {
    let intake = identifier_intake("Who owns account number 00123456?");

    assert!(!format!("{intake:?}").contains("00123456"));
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
fn month_name_ranges_resolve_inclusively_in_both_languages() {
    let instant = reference("2026-07-24T12:00:00Z");
    let business_today = NaiveDate::from_ymd_opt(2026, 7, 24).unwrap();
    let english = extract_message_facts_at(
        "Monthly deposit totals from January to September 2026.",
        instant,
        business_today,
        366,
    );
    assert_eq!(english.constraints.from_date.as_deref(), Some("2026-01-01"));
    assert_eq!(english.constraints.to_date.as_deref(), Some("2026-09-30"));

    // No year stated: falls back to the business year, not to a default range.
    let indonesian = extract_message_facts_at(
        "Berapa setoran tabungan per bulan dari Januari sampai September.",
        instant,
        business_today,
        366,
    );
    assert_eq!(
        indonesian.constraints.from_date.as_deref(),
        Some("2026-01-01")
    );
    assert_eq!(
        indonesian.constraints.to_date.as_deref(),
        Some("2026-09-30")
    );

    // Leap February ends on the 29th.
    let leap = extract_message_facts_at(
        "from February 2024 to February 2024",
        instant,
        business_today,
        366,
    );
    assert_eq!(leap.constraints.to_date.as_deref(), Some("2024-02-29"));

    // Beyond the capability guard: refused, never truncated or defaulted.
    let too_long = extract_message_facts_at(
        "from January 2025 to December 2026",
        instant,
        business_today,
        366,
    );
    assert_eq!(
        too_long.temporal_error.map(|error| error.code).as_deref(),
        Some("temporal_range_too_large")
    );
    assert!(
        extract_message_facts_at("from November to February", instant, business_today, 366)
            .temporal_error
            .is_some()
    );
}

#[test]
fn relative_expressions_derive_from_business_date_both_languages() {
    let wall = reference("2026-07-25T02:00:00Z");
    let business_today = NaiveDate::from_ymd_opt(2026, 7, 23).unwrap();
    let cases = [
        ("today", "2026-07-23", "2026-07-23"),
        ("hari ini", "2026-07-23", "2026-07-23"),
        ("yesterday", "2026-07-22", "2026-07-22"),
        ("kemarin", "2026-07-22", "2026-07-22"),
        ("this week", "2026-07-20", "2026-07-26"),
        ("minggu ini", "2026-07-20", "2026-07-26"),
        ("last week", "2026-07-13", "2026-07-19"),
        ("minggu lalu", "2026-07-13", "2026-07-19"),
        ("this month", "2026-07-01", "2026-07-31"),
        ("bulan ini", "2026-07-01", "2026-07-31"),
        ("last month", "2026-06-01", "2026-06-30"),
        ("bulan lalu", "2026-06-01", "2026-06-30"),
        ("this quarter", "2026-07-01", "2026-09-30"),
        ("kuartal ini", "2026-07-01", "2026-09-30"),
        ("last quarter", "2026-04-01", "2026-06-30"),
        ("kuartal lalu", "2026-04-01", "2026-06-30"),
        ("this year", "2026-01-01", "2026-12-31"),
        ("tahun ini", "2026-01-01", "2026-12-31"),
        ("last year", "2025-01-01", "2025-12-31"),
        ("tahun lalu", "2025-01-01", "2025-12-31"),
        ("last 3 days", "2026-07-21", "2026-07-23"),
        ("3 hari terakhir", "2026-07-21", "2026-07-23"),
    ];
    for (phrase, from, to) in cases {
        let extraction = extract_message_facts_at(phrase, wall, business_today, 366);
        assert_eq!(
            extraction.constraints.from_date.as_deref(),
            Some(from),
            "{phrase}"
        );
        assert_eq!(
            extraction.constraints.to_date.as_deref(),
            Some(to),
            "{phrase}"
        );
    }
}

#[test]
fn provenance_reference_instant_stays_wall_clock() {
    let wall = reference("2026-07-25T02:00:00Z");
    let business_today = NaiveDate::from_ymd_opt(2026, 7, 23).unwrap();
    let extraction = extract_message_facts_at("kemarin", wall, business_today, 366);

    assert_eq!(
        extraction.constraints.from_date.as_deref(),
        Some("2026-07-22")
    );
    let provenance = extraction.temporal_provenance.expect("temporal provenance");
    assert_eq!(provenance.reference_instant, wall);
    assert_eq!(provenance.timezone, "Asia/Jakarta");
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

#[test]
fn payload_source_unknown_variant_deserialises_safely() {
    let parsed: PayloadSource =
        serde_json::from_str("\"prior_job\"").expect("unknown source must deserialise");
    assert_eq!(parsed, PayloadSource::Unknown);

    let candidate: PayloadCandidate = serde_json::from_value(serde_json::json!({
        "field": "limit",
        "value": 10,
        "source": "some_future_source",
        "trust": "trusted"
    }))
    .expect("candidate with unknown source must deserialise");
    assert_eq!(candidate.source, PayloadSource::Unknown);

    assert_eq!(
        serde_json::from_str::<PayloadSource>("\"user_text\"").unwrap(),
        PayloadSource::UserText
    );
    assert_eq!(
        serde_json::from_str::<PayloadSource>("\"llm_claim\"").unwrap(),
        PayloadSource::LlmClaim
    );
    assert_eq!(
        serde_json::from_str::<PayloadSource>("\"catalog_default\"").unwrap(),
        PayloadSource::CatalogDefault
    );
}

/// The named-entity recogniser is a heuristic feeding a trusted slot, so this
/// pins both halves: what it must catch, and what it must refuse to call a
/// person. A wrong `search` value silently returns another customer's data.
#[test]
fn named_entities_are_classified_by_what_they_name() {
    let entity = |message: &str, kind: AssistantEntityType| {
        extract_message_facts(message)
            .entities
            .into_iter()
            .find(|e| e.entity_type == kind)
            .map(|e| e.value)
    };

    // Multi-token names, and the Indonesian anchor the catalog's own example uses.
    assert_eq!(
        entity(
            "ada gak nama Tony di client kita?",
            AssistantEntityType::PersonName
        ),
        Some("Tony".into())
    );
    assert_eq!(
        entity(
            "How many savings accounts does John Doe have?",
            AssistantEntityType::PersonName
        ),
        Some("John Doe".into())
    );
    assert_eq!(
        entity(
            "find client named Jonathan Doe",
            AssistantEntityType::PersonName
        ),
        Some("Jonathan Doe".into())
    );

    // A capitalised phrase carrying a domain noun is that thing, never a person.
    assert_eq!(
        entity(
            "Is there an office named Head Office?",
            AssistantEntityType::Office
        ),
        Some("Head Office".into())
    );
    assert_eq!(
        entity(
            "Is there an office named Head Office?",
            AssistantEntityType::PersonName
        ),
        None
    );
    assert_eq!(
        entity(
            "Show all charges of type Weekly Charge on savings accounts.",
            AssistantEntityType::ChargeType
        ),
        Some("Weekly Charge".into())
    );
    assert_eq!(
        entity(
            "Show all charges of type Weekly Charge on savings accounts.",
            AssistantEntityType::PersonName
        ),
        None
    );

    // A lone stray capital is not evidence of a name.
    assert_eq!(
        entity(
            "return the savings account ID and transaction count",
            AssistantEntityType::PersonName
        ),
        None
    );
    // Neither is the first word of the sentence.
    assert_eq!(
        entity("Show the top 10 clients", AssistantEntityType::PersonName),
        None
    );
}

/// A name typed in lower case is still a name. `search` reaches SQL as
/// `c.display_name ILIKE '%' || $2 || '%'`, so "john doe" would have matched
/// "John Doe" — it just never got bound, and the user saw
/// `missing parameter search`.
///
/// The other half is what the anchor is *not* allowed to swallow. Without a
/// capital there is no self-terminating edge, so the run stops at a stop word or
/// at two tokens, and a run carrying a banking noun is dropped rather than
/// promoted to an office or a product.
#[test]
fn a_lowercase_name_after_an_anchor_is_read_without_swallowing_the_sentence() {
    let person = |message: &str| {
        extract_message_facts(message)
            .entities
            .into_iter()
            .find(|e| e.entity_type == AssistantEntityType::PersonName)
            .map(|e| e.value)
    };

    for (message, expected) in [
        ("nama john doe", Some("john doe")),
        ("nama JOHN DOE", Some("JOHN DOE")),
        ("nama john Doe", Some("john Doe")),
        ("nama john", Some("john")),
        // Only the anchors that announce a name. "client" is followed by an
        // ordinary noun far more often, and a wrong `search` is a wrong answer.
        ("look up a client please", None),
        ("find client tony", None),
        // The tail must not come along: "di" ends the run well before the cap.
        ("nama john doe di office foo", Some("john doe")),
        // Two tokens is the ceiling even with nothing to stop on.
        ("nama john doe smith", Some("john doe")),
        // The user talking about names, not typing one.
        ("what is the client name?", None),
        // Lower case never invents a place or a product.
        ("client head office", None),
        ("show me client accounts", None),
        // `for` is not a lowercase anchor: too many non-names follow it.
        ("savings report for january", None),
        // A sentence boundary breaks the anchor's reach.
        ("give me the client name. john said so", None),
    ] {
        assert_eq!(person(message).as_deref(), expected, "message: {message:?}");
    }
}

#[test]
fn transaction_amount_is_read_whole_and_only_when_anchored() {
    assert_eq!(
        extract_message_facts("with latest transaction amount 0.130000 on Current Account USD")
            .constraints
            .transaction_amount,
        Some("0.130000".into())
    );
    // "top 10" is a limit, not an amount.
    assert_eq!(
        extract_message_facts("show top 10 clients")
            .constraints
            .transaction_amount,
        None
    );
}
