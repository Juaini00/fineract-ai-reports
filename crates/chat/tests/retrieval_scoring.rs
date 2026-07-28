use chat::assistant::evidence::RetrievalPlan;
use chat::assistant::retrieval::compatible_ids;
use chat::assistant::{
    AssistantConstraints, AssistantDomain, AssistantIntent, AssistantIntentKind, AssistantLanguage,
    ContextReference, RequestGrouping, RequestOperation, RequestOutput, RequestPii, RequestShape,
    RequestSubject,
};
use chat::knowledge::model::{CapabilityKnowledge, KnowledgeCatalog};

fn make_intent(domain: AssistantDomain, subject: RequestSubject) -> AssistantIntent {
    AssistantIntent {
        intent: AssistantIntentKind::ReportRequest,
        domain,
        request_shape: RequestShape {
            operation: RequestOperation::Rank,
            subject,
            grouping: RequestGrouping::None,
            output: RequestOutput::Ranking,
            pii: RequestPii::ClientIdentity,
        },
        language: AssistantLanguage::En,
        entities: Vec::new(),
        constraints: AssistantConstraints::default(),
        context_reference: ContextReference::None,
        source: None,
        confidence: 0.9,
        reason: "test".into(),
    }
}

fn make_capability(id: &str, domain: &str, subject: RequestSubject) -> CapabilityKnowledge {
    CapabilityKnowledge {
        id: id.into(),
        status: "approved_mvp".into(),
        domain: domain.into(),
        display_name: Some(id.into()),
        description: Some(format!("test capability {id}")),
        data_areas: vec![],
        query_id: format!("{id}.query"),
        metrics: vec!["savings.account_count".into()],
        output_mode: "top_n".into(),
        request_shape: RequestShape {
            operation: RequestOperation::Rank,
            subject,
            grouping: RequestGrouping::None,
            output: RequestOutput::Ranking,
            pii: RequestPii::ClientIdentity,
        },
        examples: vec![],
        required_parameters: vec![],
        optional_parameters: vec![],
        defaults: Default::default(),
        guards: Default::default(),
        parameter_policies: vec![],
    }
}

fn catalog_with(capability: CapabilityKnowledge) -> KnowledgeCatalog {
    KnowledgeCatalog {
        root_path: Default::default(),
        query_path: Default::default(),
        data_areas: vec![],
        domains: vec![],
        schemas: vec![],
        metrics: vec![],
        capabilities: vec![capability],
        queries: vec![],
        policies: vec![],
        responses: vec![],
        parameter_inputs: Vec::new(),
        classification: Default::default(),
    }
}

#[test]
fn domain_mismatch_does_not_exclude_capability_when_subject_matches() {
    // Regression for issue 04: router misclassifies domain as Savings for
    // "top clients by savings account" queries while subject is correctly Client.
    // Previously this filtered out client_top_n_by_savings_account_count.
    let intent = make_intent(AssistantDomain::Savings, RequestSubject::Client);
    let plan = RetrievalPlan::new(
        "top 3 clients by savings account",
        &intent,
        false,
        vec!["client_top_n_by_savings_account_count".to_string()],
    );
    let catalog = catalog_with(make_capability(
        "client_top_n_by_savings_account_count",
        "client",
        RequestSubject::Client,
    ));

    let compat = compatible_ids(&plan, &catalog);
    assert_eq!(
        compat,
        vec!["client_top_n_by_savings_account_count".to_string()],
        "capability with domain=client must survive when plan.domain=Savings and subject matches"
    );
}

#[test]
fn shape_score_ranks_full_match_over_partial_match() {
    use chat::assistant::retrieval::shape_score;

    let intent = make_intent(AssistantDomain::Client, RequestSubject::Client);
    let plan = RetrievalPlan::new("top clients", &intent, false, vec![]);

    let full = make_capability("full", "client", RequestSubject::Client);
    let partial = make_capability("partial", "client", RequestSubject::Office);
    // partial mismatches subject only

    let full_score = shape_score(&plan, &full);
    let partial_score = shape_score(&plan, &partial);

    assert!(
        full_score > partial_score,
        "full={full_score} partial={partial_score}"
    );
    assert!((0.0..=1.0).contains(&full_score));
    assert!((0.0..=1.0).contains(&partial_score));
}

#[test]
fn retrieve_returns_candidates_when_no_shape_matches_but_catalog_non_empty() {
    // Regression for issue 01: previously an empty compatible_ids collapsed
    // the entire pipeline. Now retrieve must still surface catalog_fallback
    // candidates, letting downstream (reranker / evaluator) decide.
    use chat::assistant::retrieval::RetrievalEngine;

    let intent = make_intent(AssistantDomain::Organization, RequestSubject::Office);
    let mut shape = intent.request_shape.clone();
    shape.operation = RequestOperation::RandomSample;
    let mut intent = intent;
    intent.request_shape = shape;

    let plan = RetrievalPlan::new(
        "berikan 3 office",
        &intent,
        false,
        vec!["organization_office_summary".to_string()],
    );
    let mut cap = make_capability(
        "organization_office_summary",
        "organization",
        RequestSubject::Office,
    );
    cap.request_shape.operation = RequestOperation::Summary;
    let catalog = catalog_with(cap);
    let catalog = std::sync::Arc::new(catalog);

    let evidence = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(async { RetrievalEngine::retrieve(&plan, None, None, Some(&catalog)).await })
        .expect("retrieve should not error");

    assert!(
        !evidence.is_empty(),
        "shape mismatch alone must not collapse retrieval to empty"
    );
    assert_eq!(evidence[0].capability_id, "organization_office_summary");
}

#[test]
fn top_n_by_savings_account_count_selected_for_rank_query() {
    // Query from prod log 2026-07-17: "3 clients where have the most savings account for this year"
    let intent = make_intent(AssistantDomain::Savings, RequestSubject::Client); // domain misclassified — must not matter
    let plan = RetrievalPlan::new(
        "3 clients where have the most savings account for this year",
        &intent,
        false,
        vec![
            "client_top_n_by_savings_account_count".to_string(),
            "savings_deposit_total".to_string(),
        ],
    );
    let mut target = make_capability(
        "client_top_n_by_savings_account_count",
        "client",
        RequestSubject::Client,
    );
    target.description = Some("Top clients by number of active savings accounts".into());
    let mut distractor = make_capability(
        "savings_deposit_total",
        "savings",
        RequestSubject::SavingsTransaction,
    );
    distractor.request_shape.operation = RequestOperation::Total;
    distractor.request_shape.output = RequestOutput::Scalar;

    let catalog = std::sync::Arc::new(KnowledgeCatalog {
        capabilities: vec![target, distractor],
        ..catalog_with(make_capability("_", "_", RequestSubject::Client))
    });
    let evidence = tokio::runtime::Runtime::new().unwrap().block_on(async {
        chat::assistant::retrieval::RetrievalEngine::retrieve(&plan, None, None, Some(&catalog))
            .await
            .unwrap()
    });
    assert_eq!(
        evidence[0].capability_id,
        "client_top_n_by_savings_account_count"
    );
}

#[test]
fn normalized_catalog_fallback_preserves_specific_six_term_rank_gap() {
    use chat::assistant::retrieval::catalog_fallback;

    let intent = make_intent(AssistantDomain::Client, RequestSubject::Client);
    let plan = RetrievalPlan::new(
        "alpha bravo charlie delta echo foxtrot",
        &intent,
        true,
        vec![],
    );
    let mut specific = make_capability("specific", "client", RequestSubject::Client);
    specific.description = Some("alpha bravo charlie delta echo foxtrot".into());
    let mut broad = make_capability("broad", "client", RequestSubject::Client);
    broad.description = Some("alpha bravo charlie delta echo".into());
    let catalog = KnowledgeCatalog {
        capabilities: vec![specific, broad],
        ..catalog_with(make_capability("_", "_", RequestSubject::Client))
    };

    let evidence = catalog_fallback(&plan, &catalog);
    let specific_score = evidence
        .iter()
        .find(|candidate| candidate.capability_id == "specific")
        .expect("specific candidate")
        .score;
    let broad_score = evidence
        .iter()
        .find(|candidate| candidate.capability_id == "broad")
        .expect("broad candidate")
        .score;

    assert!(
        specific_score - broad_score >= 0.05,
        "specific={specific_score} broad={broad_score}"
    );
}

#[test]
fn build_retrieval_trace_emits_expected_top_level_keys() {
    use chat::assistant::evidence::Evidence;
    use chat::assistant::reranker::RerankerDecision;
    use chat::assistant::runtime::build_retrieval_trace;

    let intent = make_intent(AssistantDomain::Client, RequestSubject::Client);
    let plan = RetrievalPlan::new("top 3 clients", &intent, false, vec!["capability_a".into()]);
    let evidence = vec![Evidence {
        capability_id: "capability_a".into(),
        title: "Cap A".into(),
        score: 0.82,
        source_type: "capability".into(),
        metadata: serde_json::json!({}),
        conflicting: false,
    }];
    let decision = RerankerDecision::select("capability_a", 0.9);

    let trace = build_retrieval_trace(&intent, &plan, &evidence, &decision);

    let obj = trace.as_object().expect("trace must be a JSON object");
    for key in ["router_intent", "plan", "candidates", "decision"] {
        assert!(obj.contains_key(key), "missing key {key}");
    }
    let candidates = trace["candidates"].as_array().expect("candidates array");
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0]["capability_id"], "capability_a");
    assert_eq!(trace["decision"]["kind"], "select");
    assert_eq!(trace["decision"]["capability_id"], "capability_a");
}

#[test]
fn clarification_option_falls_back_to_humanized_id_when_display_name_missing() {
    // Regression for issue 08 Bug A: organization_office_summary rendered in
    // a clarification prompt with label = raw id ("organization_office_summary")
    // because the YAML lacked display_name and the option builder had no
    // humanization fallback. The fix is a humanize_id() helper used wherever
    // a ClarificationOption label is derived from a capability.
    use chat::assistant::clarification::{ClarificationOption, humanize_id};

    let mut cap = make_capability(
        "organization_office_summary",
        "organization",
        RequestSubject::Office,
    );
    cap.display_name = None;

    let option = ClarificationOption {
        id: cap.id.clone(),
        label: cap
            .display_name
            .clone()
            .unwrap_or_else(|| humanize_id(&cap.id)),
        description: cap.description.clone(),
        fields: Vec::new(),
    };

    assert_eq!(option.label, "Organization Office Summary");
    assert_ne!(
        option.label, cap.id,
        "label must not be the raw capability id"
    );
}

/// Loads the real `knowledge/` + `queries/` catalog from the workspace, the
/// same way `catalog_validation.rs` does. Using the real catalog (rather than
/// a synthetic one) is what makes these three tests genuinely RED before the
/// issue-03 capability YAMLs exist: the target id simply isn't in the catalog
/// yet, so `evidence[0].capability_id` cannot equal it.
#[derive(Clone, Copy)]
struct BilingualInventoryCase {
    id: &'static str,
    indonesian: &'static str,
    english: &'static str,
    capability_id: &'static str,
}

struct ScoringGap {
    inventory_id: &'static str,
    language: &'static str,
    observed_top_id: &'static str,
}

// Direct transcription of the 31 covered savings, client, and organization
// rows in docs/product/analyst-question-inventory.md. This is deliberately test
// data, not a second knowledge source: the loaded catalog remains authoritative.
const COVERED_INVENTORY: &[BilingualInventoryCase] = &[
    BilingualInventoryCase {
        id: "1",
        indonesian: "Berapa saldo total tabungan aktif saat ini?",
        english: "Total active savings balance now?",
        capability_id: "savings_balance_summary",
    },
    BilingualInventoryCase {
        id: "2",
        indonesian: "Nasabah mana masih punya charge belum dibayar, beserta due date, hari terlambat, dibayar, dan sisa?",
        english: "Which clients have unpaid savings charges, due date, overdue days, paid amount, and balance?",
        capability_id: "savings_pending_charges_clients",
    },
    BilingualInventoryCase {
        id: "3",
        indonesian: "Berapa total penarikan bulan ini?",
        english: "Total withdrawals this month?",
        capability_id: "savings_withdrawal_total",
    },
    BilingualInventoryCase {
        id: "4",
        indonesian: "Berapa total setoran bulan ini?",
        english: "Total deposits this month?",
        capability_id: "savings_deposit_total",
    },
    BilingualInventoryCase {
        id: "5",
        indonesian: "Siapa penarik terbesar bulan ini?",
        english: "Who made the largest withdrawals this month?",
        capability_id: "savings_withdrawal_top_n",
    },
    BilingualInventoryCase {
        id: "6",
        indonesian: "Siapa penyetor terbesar hari ini?",
        english: "Who made the largest deposits today?",
        capability_id: "savings_deposit_top_n",
    },
    BilingualInventoryCase {
        id: "7",
        indonesian: "Tunjukkan aktivitas tabungan minggu ini",
        english: "Show savings activity this week",
        capability_id: "savings_activity_list",
    },
    BilingualInventoryCase {
        id: "8",
        indonesian: "Setoran per bulan tahun ini",
        english: "Monthly deposits this year",
        capability_id: "savings_deposit_monthly_breakdown",
    },
    BilingualInventoryCase {
        id: "9",
        indonesian: "Penarikan per bulan tahun ini",
        english: "Monthly withdrawals this year",
        capability_id: "savings_withdrawal_monthly_breakdown",
    },
    BilingualInventoryCase {
        id: "10",
        indonesian: "Setoran terbesar setiap bulan",
        english: "Largest deposit each month",
        capability_id: "savings_deposit_monthly_top_n",
    },
    BilingualInventoryCase {
        id: "11",
        indonesian: "Tunjukkan nasabah baru diaktivasi",
        english: "Show recently activated clients",
        capability_id: "client_list_recent",
    },
    BilingualInventoryCase {
        id: "12",
        indonesian: "Ada nama Tony di client?",
        english: "Is there a client named Tony?",
        capability_id: "client_name_lookup",
    },
    BilingualInventoryCase {
        id: "13",
        indonesian: "Aktivasi nasabah tiap bulan tahun lalu",
        english: "Client activations each month last year",
        capability_id: "client_activation_monthly_breakdown",
    },
    BilingualInventoryCase {
        id: "14",
        indonesian: "Kantor teratas aktivasi nasabah bulan ini",
        english: "Top offices by new client activations",
        capability_id: "client_activation_top_n_offices",
    },
    BilingualInventoryCase {
        id: "15",
        indonesian: "Berikan 5 client sembarang",
        english: "Give a random sample of 5 clients",
        capability_id: "client_random_sample",
    },
    BilingualInventoryCase {
        id: "16",
        indonesian: "Ringkasan lifecycle nasabah",
        english: "Client lifecycle summary",
        capability_id: "client_lifecycle_summary",
    },
    BilingualInventoryCase {
        id: "17",
        indonesian: "Jumlah nasabah aktif per kantor",
        english: "Active clients per office",
        capability_id: "client_summary_by_office",
    },
    BilingualInventoryCase {
        id: "18",
        indonesian: "Nasabah dengan setoran terbesar",
        english: "Top clients by deposit volume",
        capability_id: "client_top_n_by_deposit_volume",
    },
    BilingualInventoryCase {
        id: "19",
        indonesian: "Nasabah dengan rekening tabungan terbanyak",
        english: "Clients with most savings accounts",
        capability_id: "client_top_n_by_savings_account_count",
    },
    BilingualInventoryCase {
        id: "20",
        indonesian: "Nasabah dengan saldo tertinggi",
        english: "Clients with highest savings balance",
        capability_id: "client_top_n_by_savings_balance",
    },
    BilingualInventoryCase {
        id: "21",
        indonesian: "Ringkasan hierarki kantor",
        english: "Office hierarchy summary",
        capability_id: "organization_hierarchy_summary",
    },
    BilingualInventoryCase {
        id: "22",
        indonesian: "Kantor paling aktif bulan ini",
        english: "Offices with most transactions",
        capability_id: "organization_office_activity_ranking",
    },
    BilingualInventoryCase {
        id: "23",
        indonesian: "Jumlah nasabah per kantor",
        english: "Client counts per office",
        capability_id: "organization_office_client_summary",
    },
    BilingualInventoryCase {
        id: "24",
        indonesian: "Kantor tanpa aktivitas kuartal ini",
        english: "Offices with no activity this quarter",
        capability_id: "organization_office_dormant",
    },
    BilingualInventoryCase {
        id: "25",
        indonesian: "Pohon hierarki kantor",
        english: "Office hierarchy tree",
        capability_id: "organization_office_hierarchy_tree",
    },
    BilingualInventoryCase {
        id: "26",
        indonesian: "Daftar kantor pada scope saya",
        english: "Offices in my authorized scope",
        capability_id: "office_list_basic",
    },
    BilingualInventoryCase {
        id: "27",
        indonesian: "Kantor dibuka tiap bulan",
        english: "Offices opened each month",
        capability_id: "organization_office_opening_monthly_breakdown",
    },
    BilingualInventoryCase {
        id: "28",
        indonesian: "Kantor dengan saldo terbesar",
        english: "Offices with greatest savings balance",
        capability_id: "organization_office_savings_summary",
    },
    BilingualInventoryCase {
        id: "29",
        indonesian: "Ringkasan kantor dan staf aktif",
        english: "Office summary with active staff",
        capability_id: "organization_office_summary",
    },
    BilingualInventoryCase {
        id: "G1",
        indonesian: "Total charge yang pernah dikenakan berapa?",
        english: "What is the total ever levied on this savings charge?",
        capability_id: "savings_pending_charges_clients",
    },
    BilingualInventoryCase {
        id: "G2",
        indonesian: "Charge mana yang benar-benar overdue saja?",
        english: "Which savings charges are strictly overdue only?",
        capability_id: "savings_strictly_overdue_charges_clients",
    },
];

const MISSING_INVENTORY: &[BilingualInventoryCase] = &[
    BilingualInventoryCase {
        id: "L1",
        indonesian: "Nasabah mana pinjamannya menunggak?",
        english: "Which clients have loans in arrears?",
        capability_id: "loans_in_arrears_clients",
    },
    BilingualInventoryCase {
        id: "L2",
        indonesian: "Angsuran mana lewat jatuh tempo?",
        english: "Which installments are overdue?",
        capability_id: "loan_overdue_installments",
    },
    BilingualInventoryCase {
        id: "L3",
        indonesian: "Sisa pokok pinjaman per nasabah",
        english: "Outstanding loan balance per client",
        capability_id: "loan_outstanding_balances_clients",
    },
    BilingualInventoryCase {
        id: "L4",
        indonesian: "Charge pinjaman belum dibayar",
        english: "Clients with unpaid loan charges",
        capability_id: "loan_unpaid_charges_clients",
    },
    BilingualInventoryCase {
        id: "L5",
        indonesian: "Ringkasan portofolio pinjaman per kantor",
        english: "Loan portfolio summary per office",
        capability_id: "loan_portfolio_summary_by_office",
    },
];

// Historical observations from Bundle 7's red audit. They no longer relax the
// covered-row acceptance test; the documented ledger preserves the evidence.
const BUNDLE_8_SCORING_GAPS: &[ScoringGap] = &[
    ScoringGap {
        inventory_id: "2",
        language: "en",
        observed_top_id: "client_top_n_by_deposit_volume",
    },
    ScoringGap {
        inventory_id: "3",
        language: "en",
        observed_top_id: "savings_deposit_total",
    },
    ScoringGap {
        inventory_id: "4",
        language: "id",
        observed_top_id: "savings_deposit_monthly_breakdown",
    },
    ScoringGap {
        inventory_id: "5",
        language: "en",
        observed_top_id: "savings_deposit_top_n",
    },
    ScoringGap {
        inventory_id: "6",
        language: "id",
        observed_top_id: "savings_withdrawal_top_n",
    },
    ScoringGap {
        inventory_id: "6",
        language: "en",
        observed_top_id: "savings_deposit_top_n",
    },
    ScoringGap {
        inventory_id: "10",
        language: "en",
        observed_top_id: "savings_deposit_monthly_top_n",
    },
    ScoringGap {
        inventory_id: "13",
        language: "id",
        observed_top_id: "savings_deposit_monthly_breakdown",
    },
    ScoringGap {
        inventory_id: "13",
        language: "en",
        observed_top_id: "client_activation_monthly_breakdown",
    },
    ScoringGap {
        inventory_id: "14",
        language: "id",
        observed_top_id: "client_activation_top_n_offices",
    },
    ScoringGap {
        inventory_id: "14",
        language: "en",
        observed_top_id: "client_activation_top_n_offices",
    },
    ScoringGap {
        inventory_id: "17",
        language: "id",
        observed_top_id: "client_summary_by_office",
    },
    ScoringGap {
        inventory_id: "17",
        language: "en",
        observed_top_id: "client_summary_by_office",
    },
    ScoringGap {
        inventory_id: "18",
        language: "id",
        observed_top_id: "savings_deposit_monthly_top_n",
    },
    ScoringGap {
        inventory_id: "19",
        language: "id",
        observed_top_id: "client_top_n_by_deposit_volume",
    },
    ScoringGap {
        inventory_id: "19",
        language: "en",
        observed_top_id: "client_top_n_by_savings_account_count",
    },
    ScoringGap {
        inventory_id: "20",
        language: "id",
        observed_top_id: "client_top_n_by_deposit_volume",
    },
    ScoringGap {
        inventory_id: "21",
        language: "id",
        observed_top_id: "savings_balance_summary",
    },
    ScoringGap {
        inventory_id: "22",
        language: "id",
        observed_top_id: "organization_office_activity_ranking",
    },
    ScoringGap {
        inventory_id: "22",
        language: "en",
        observed_top_id: "organization_office_activity_ranking",
    },
    ScoringGap {
        inventory_id: "23",
        language: "id",
        observed_top_id: "organization_office_client_summary",
    },
    ScoringGap {
        inventory_id: "23",
        language: "en",
        observed_top_id: "client_summary_by_office",
    },
    ScoringGap {
        inventory_id: "24",
        language: "id",
        observed_top_id: "organization_office_activity_ranking",
    },
    ScoringGap {
        inventory_id: "25",
        language: "id",
        observed_top_id: "organization_office_hierarchy_tree",
    },
    ScoringGap {
        inventory_id: "27",
        language: "id",
        observed_top_id: "savings_deposit_monthly_breakdown",
    },
    ScoringGap {
        inventory_id: "28",
        language: "id",
        observed_top_id: "organization_office_activity_ranking",
    },
    ScoringGap {
        inventory_id: "29",
        language: "id",
        observed_top_id: "savings_balance_summary",
    },
    ScoringGap {
        inventory_id: "29",
        language: "en",
        observed_top_id: "organization_office_summary",
    },
];

// Scores are historical audit evidence, not incidental test output.
const BUNDLE_8_GAP_SCORES: &[(&str, &str, f32, f32)] = &[
    ("2", "en", 0.99, 0.99),
    ("3", "en", 0.90, 0.90),
    ("4", "id", 0.99, 0.99),
    ("5", "en", 0.99, 0.99),
    ("6", "id", 0.75, 0.69),
    ("6", "en", 0.99, 0.99),
    ("10", "en", 0.99, 0.99),
    ("13", "id", 0.64, 0.64),
    ("13", "en", 0.99, 0.99),
    ("14", "id", 0.60, 0.60),
    ("14", "en", 0.99, 0.99),
    ("17", "id", 0.75, 0.70),
    ("17", "en", 0.99, 0.99),
    ("18", "id", 0.73, 0.64),
    ("19", "id", 0.60, 0.60),
    ("19", "en", 0.99, 0.99),
    ("20", "id", 0.60, 0.60),
    ("21", "id", 0.64, 0.60),
    ("22", "id", 0.60, 0.60),
    ("22", "en", 0.90, 0.90),
    ("23", "id", 0.75, 0.75),
    ("23", "en", 0.99, 0.99),
    ("24", "id", 0.60, 0.60),
    ("25", "id", 0.60, 0.60),
    ("27", "id", 0.64, 0.64),
    ("28", "id", 0.60, 0.60),
    ("29", "id", 0.79, 0.75),
    ("29", "en", 0.99, 0.94),
];

#[test]
fn bundle_7_scoring_ledger_retains_every_remediated_observation() {
    assert_eq!(BUNDLE_8_SCORING_GAPS.len(), 28);
    assert_eq!(BUNDLE_8_GAP_SCORES.len(), BUNDLE_8_SCORING_GAPS.len());
    assert!(BUNDLE_8_SCORING_GAPS.iter().any(|gap| {
        gap.inventory_id == "2"
            && gap.language == "en"
            && gap.observed_top_id == "client_top_n_by_deposit_volume"
    }));
}

fn inventory_intent(
    capability: &CapabilityKnowledge,
    language: AssistantLanguage,
) -> AssistantIntent {
    let domain = match capability.domain.as_str() {
        "savings" => AssistantDomain::Savings,
        "client" => AssistantDomain::Client,
        "organization" => AssistantDomain::Organization,
        other => panic!("unsupported inventory domain {other}"),
    };
    AssistantIntent {
        intent: AssistantIntentKind::ReportRequest,
        domain,
        request_shape: capability.request_shape.clone(),
        language,
        entities: Vec::new(),
        constraints: AssistantConstraints::default(),
        context_reference: ContextReference::None,
        source: None,
        confidence: 0.9,
        reason: "inventory retrieval regression".into(),
    }
}

async fn inventory_evidence(
    catalog: &std::sync::Arc<KnowledgeCatalog>,
    capability_id: &str,
    phrase: &str,
    language: AssistantLanguage,
) -> Vec<chat::assistant::Evidence> {
    use chat::assistant::retrieval::RetrievalEngine;

    let capability = catalog
        .capabilities
        .iter()
        .find(|capability| capability.id == capability_id)
        .unwrap_or_else(|| panic!("inventory capability {capability_id} must exist"));
    let intent = inventory_intent(capability, language);
    let plan = RetrievalPlan::new(phrase, &intent, true, vec![]);
    RetrievalEngine::retrieve(&plan, None, None, Some(catalog))
        .await
        .expect("catalog fallback retrieval")
}

fn load_real_catalog() -> std::sync::Arc<KnowledgeCatalog> {
    let workspace_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root");
    let catalog = chat::knowledge::catalog::loader::KnowledgeLoader::new(
        workspace_root.join("knowledge"),
        workspace_root.join("queries"),
    )
    .load()
    .expect("load knowledge catalog");
    std::sync::Arc::new(catalog)
}

#[test]
fn office_list_basic_selected_for_berikan_office_query() {
    // Issue 03: "berikan 3 office yg ada pada system saat ini" has no ranking
    // metric — must route to the plain browse/list capability, not a summary
    // or top-n-by-metric capability.
    use chat::assistant::retrieval::RetrievalEngine;

    let intent = make_intent(AssistantDomain::Organization, RequestSubject::Office);
    let mut shape = intent.request_shape.clone();
    shape.operation = RequestOperation::List;
    shape.output = RequestOutput::List;
    shape.pii = RequestPii::None;
    let mut intent = intent;
    intent.request_shape = shape;

    let plan = RetrievalPlan::new(
        "berikan 3 office yg ada pada system saat ini",
        &intent,
        false,
        vec![
            "office_list_basic".to_string(),
            "organization_office_summary".to_string(),
        ],
    );

    let catalog = load_real_catalog();

    let evidence = tokio::runtime::Runtime::new().unwrap().block_on(async {
        RetrievalEngine::retrieve(&plan, None, None, Some(&catalog))
            .await
            .unwrap()
    });

    assert_eq!(evidence[0].capability_id, "office_list_basic");
}

#[test]
fn client_list_recent_selected_for_new_clients_query() {
    // "recently activated clients" has no ranking metric — must route to the
    // plain browse/list capability, not a lifecycle summary or top-n capability.
    use chat::assistant::retrieval::RetrievalEngine;

    let intent = make_intent(AssistantDomain::Client, RequestSubject::Client);
    let mut shape = intent.request_shape.clone();
    shape.operation = RequestOperation::List;
    shape.output = RequestOutput::List;
    let mut intent = intent;
    intent.request_shape = shape;

    let plan = RetrievalPlan::new(
        "show me the most recently activated clients",
        &intent,
        false,
        vec![
            "client_list_recent".to_string(),
            "client_top_n_by_savings_account_count".to_string(),
        ],
    );

    let catalog = load_real_catalog();

    let evidence = tokio::runtime::Runtime::new().unwrap().block_on(async {
        RetrievalEngine::retrieve(&plan, None, None, Some(&catalog))
            .await
            .unwrap()
    });

    assert_eq!(evidence[0].capability_id, "client_list_recent");
}

#[test]
fn client_random_sample_selected_for_sembarang_query() {
    // "coba berikan saya 5 client sembarang pada tahun ini" — "sembarang"
    // (Indonesian for "random/arbitrary") must route to the random-sample
    // capability, not a ranked or plain-list capability.
    use chat::assistant::retrieval::RetrievalEngine;

    let intent = make_intent(AssistantDomain::Client, RequestSubject::Client);
    let mut shape = intent.request_shape.clone();
    shape.operation = RequestOperation::RandomSample;
    shape.output = RequestOutput::List;
    let mut intent = intent;
    intent.request_shape = shape;

    let plan = RetrievalPlan::new(
        "coba berikan saya 5 client sembarang pada tahun ini",
        &intent,
        false,
        vec![
            "client_random_sample".to_string(),
            "client_list_recent".to_string(),
        ],
    );

    let catalog = load_real_catalog();

    let evidence = tokio::runtime::Runtime::new().unwrap().block_on(async {
        RetrievalEngine::retrieve(&plan, None, None, Some(&catalog))
            .await
            .unwrap()
    });

    assert_eq!(evidence[0].capability_id, "client_random_sample");
}

#[test]
fn bilingual_covered_inventory_rows_rank_first_and_clear_policy_thresholds() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let catalog = load_real_catalog();
        let mut unexpected = Vec::new();

        for case in COVERED_INVENTORY {
            for (language, language_code, phrase) in [
                (AssistantLanguage::Id, "id", case.indonesian),
                (AssistantLanguage::En, "en", case.english),
            ] {
                let evidence = inventory_evidence(
                    &catalog,
                    case.capability_id,
                    phrase,
                    language,
                )
                .await;
                let top = evidence.first();
                let top_score = top.map_or(0.0, |candidate| candidate.score);
                let second = evidence.get(1);
                let second_score = second.map_or(0.0, |candidate| candidate.score);
                let top_id = top.map(|candidate| candidate.capability_id.as_str());
                let second_id = second.map(|candidate| candidate.capability_id.as_str());
                let passes = top_id == Some(case.capability_id)
                    && top_score >= catalog.classification.min_floor
                    && top_score - second_score >= catalog.classification.min_gap;
                if !passes {
                    unexpected.push(format!(
                        "{} {language_code}: expected={} top={top_id:?} score={top_score:.2} second={second_id:?} score={second_score:.2}",
                        case.id, case.capability_id,
                    ));
                }
            }
        }

        assert!(
            unexpected.is_empty(),
            "covered inventory regression drift:\n{}",
            unexpected.join("\n")
        );
    });
}

#[test]
fn bilingual_missing_inventory_rows_stay_explicit_for_issue_008() {
    use chat::assistant::retrieval::RetrievalEngine;
    use chat::assistant::{LlmReranker, RerankerVerdict};

    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let catalog = load_real_catalog();

        assert_eq!(COVERED_INVENTORY.len() * 2, 62);
        assert_eq!(MISSING_INVENTORY.len() * 2, 10);

        for case in MISSING_INVENTORY {
            assert!(
                !catalog
                    .capabilities
                    .iter()
                    .any(|capability| capability.id == case.capability_id),
                "missing row {} unexpectedly gained {}; promote it deliberately in Bundle 8/issue 008",
                case.id,
                case.capability_id,
            );
            for (language, phrase) in [
                (AssistantLanguage::Id, case.indonesian),
                (AssistantLanguage::En, case.english),
            ] {
                let intent = make_intent(AssistantDomain::Loan, RequestSubject::Unknown);
                let plan = RetrievalPlan::new(
                    phrase,
                    &intent,
                    false,
                    vec![case.capability_id.to_string()],
                );
                let evidence = RetrievalEngine::retrieve(&plan, None, None, Some(&catalog))
                    .await
                    .expect("restricted missing-capability retrieval remains valid");
                assert!(evidence.is_empty(), "missing row {} leaked an offered candidate", case.id);
                let decision = LlmReranker::new(None).rerank(phrase, &evidence).await;
                assert_eq!(decision.decision, RerankerVerdict::Unsupported, "missing row {} ({language:?})", case.id);
                assert!(decision.alternatives.is_empty(), "missing row {} ({language:?}) offered options", case.id);
            }
        }
    });
}

#[test]
fn deliberately_out_of_catalog_request_is_unsupported_with_no_options() {
    use chat::assistant::retrieval::RetrievalEngine;
    use chat::assistant::{LlmReranker, RerankerVerdict};

    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let catalog = load_real_catalog();
        let intent = make_intent(AssistantDomain::Loan, RequestSubject::Unknown);
        let plan = RetrievalPlan::new(
            "Prepare a loan amortization schedule for next quarter",
            &intent,
            false,
            vec!["loan_amortization_schedule".to_string()],
        );
        let evidence = RetrievalEngine::retrieve(&plan, None, None, Some(&catalog))
            .await
            .expect("out-of-catalog retrieval remains a valid request");
        assert!(
            evidence.is_empty(),
            "reserved capability must not leak catalog options"
        );

        let decision = LlmReranker::new(None)
            .rerank(&plan.query_text, &evidence)
            .await;
        assert_eq!(decision.decision, RerankerVerdict::Unsupported);
        assert!(decision.alternatives.is_empty());
    });
}

#[test]
fn retrieval_evidence_title_is_humanized_when_display_name_missing() {
    // Same regression, exercised through the actual production fallback
    // path (RetrievalEngine::retrieve -> catalog_fallback) that feeds
    // clarification_payload's ClarificationOption labels.
    use chat::assistant::retrieval::RetrievalEngine;

    let intent = make_intent(AssistantDomain::Organization, RequestSubject::Office);
    let plan = RetrievalPlan::new(
        "berikan office summary",
        &intent,
        false,
        vec!["organization_office_summary".to_string()],
    );
    let mut cap = make_capability(
        "organization_office_summary",
        "organization",
        RequestSubject::Office,
    );
    cap.display_name = None;
    let catalog = std::sync::Arc::new(catalog_with(cap));

    let evidence = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(async { RetrievalEngine::retrieve(&plan, None, None, Some(&catalog)).await })
        .expect("retrieve should not error");

    assert_eq!(evidence.len(), 1);
    assert_eq!(evidence[0].title, "Organization Office Summary");
    assert_ne!(evidence[0].title, "organization_office_summary");
}
