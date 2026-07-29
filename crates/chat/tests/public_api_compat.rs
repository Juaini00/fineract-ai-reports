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
        canonical_state as _, canonical_state_repo as _, clarification as _,
        clarification_resolver as _, context_builder as _, contracts as _, evidence as _,
        extraction as _, graph as _, intent as _, job_memory_repo as _, llm_trace_repo as _,
        memory as _, renderer as _, reranker as _, response as _, response_builder as _,
        router as _, runtime as _, session_memory_repo as _, swiftide_index as _, tool as _,
    };

    let _: Option<chat::assistant::context_builder::ContextBuilder> =
        None::<chat::assistant::context::builder::ContextBuilder>;
    let _: Option<chat::assistant::runtime::AssistantGraphRuntime> =
        None::<chat::assistant::execution::runtime::AssistantGraphRuntime>;
}
