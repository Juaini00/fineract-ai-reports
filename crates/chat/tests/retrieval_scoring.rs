use chat::assistant::evidence::RetrievalPlan;
use chat::assistant::retrieval::compatible_ids;
use chat::assistant::{
    AssistantConstraints, AssistantDomain, AssistantIntent, AssistantIntentKind,
    AssistantLanguage, ContextReference, RequestGrouping, RequestOperation, RequestOutput,
    RequestPii, RequestShape, RequestSubject,
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
        classification: Default::default(),
    }
}

#[test]
fn domain_mismatch_does_not_exclude_capability_when_subject_matches() {
    // Regression for issue 04: router misclassifies domain as Savings for
    // "top clients by savings account" queries while subject is correctly Client.
    // Previously this filtered out client_top_n_by_savings_account_count.
    let intent = make_intent(AssistantDomain::Savings, RequestSubject::Client);
    let plan = RetrievalPlan::new("top 3 clients by savings account", &intent, false, vec![
        "client_top_n_by_savings_account_count".to_string(),
    ]);
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
