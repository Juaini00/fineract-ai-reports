#[test]
fn canonical_public_paths_resolve() {
    let _: Option<chat::conversation::model::message::ChatMessage> = None;
    let _: Option<chat::conversation::model::session::ChatSession> = None;
    let _: Option<chat::job::model::ChatJob> = None;
    let _: Option<chat::conversation::repository::message::MessageRepository> = None;
    let _: Option<chat::conversation::repository::session::SessionRepository> = None;
    let _: Option<chat::job::repository::JobRepository> = None;
    let _: Option<chat::conversation::service::message::MessageService> = None;
    let _: Option<chat::conversation::service::session::SessionService> = None;
    let _: Option<chat::job::service::JobService> = None;

    #[allow(unused_imports)]
    use chat::assistant::{
        canonical_state as _, canonical_state_repo as _, catalog_index as _, clarification as _,
        clarification_resolver as _, context_builder as _, contracts as _, evidence as _,
        extraction as _, graph as _, intent as _, job_memory_repo as _, llm_trace_repo as _,
        memory as _, renderer as _, reranker as _, response as _, response_builder as _,
        router as _, runtime as _, session_memory_repo as _, tool as _,
    };

    let _: Option<chat::assistant::context_builder::ContextBuilder> =
        None::<chat::assistant::context::builder::ContextBuilder>;
}

#[test]
fn clarification_workflow_fields_are_additive_when_absent() {
    use chat::assistant::{ClarificationKind, ClarificationView};
    use uuid::Uuid;

    let view = ClarificationView {
        version: 1,
        id: Uuid::nil(),
        revision: 0,
        kind: ClarificationKind::FreeText,
        question: "Please clarify.".into(),
        options: vec![],
        fields: vec![],
        allow_free_text: true,
        workflow_id: None,
        node_id: None,
        resume_node_id: None,
        entity_kind: None,
    };
    assert_eq!(
        serde_json::to_string(&view).unwrap(),
        r#"{"version":1,"id":"00000000-0000-0000-0000-000000000000","revision":0,"kind":"free_text","question":"Please clarify.","options":[],"fields":[],"allow_free_text":true}"#,
    );
}
