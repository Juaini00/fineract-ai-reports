#[test]
fn legacy_public_paths_resolve_to_canonical_types() {
    let _: Option<chat::chat::model::message::ChatMessage> =
        None::<chat::conversation::model::message::ChatMessage>;
    let _: Option<chat::chat::model::session::ChatSession> =
        None::<chat::conversation::model::session::ChatSession>;
    let _: Option<chat::chat::model::job::ChatJob> = None::<chat::job::model::ChatJob>;
    let _: Option<chat::chat::repository::message::MessageRepository> =
        None::<chat::conversation::repository::message::MessageRepository>;
    let _: Option<chat::chat::repository::session::SessionRepository> =
        None::<chat::conversation::repository::session::SessionRepository>;
    let _: Option<chat::chat::repository::job::JobRepository> =
        None::<chat::job::repository::JobRepository>;
    let _: Option<chat::chat::service::message::MessageService> =
        None::<chat::conversation::service::message::MessageService>;
    let _: Option<chat::chat::service::session::SessionService> =
        None::<chat::conversation::service::session::SessionService>;
    let _: Option<chat::chat::service::job::JobService> = None::<chat::job::service::JobService>;

    #[allow(unused_imports)]
    use chat::assistant::{
        canonical_state as _, canonical_state_repo as _, clarification as _,
        clarification_resolver as _, context_builder as _, contracts as _, evidence as _,
        extraction as _, graph as _, intent as _, job_memory_repo as _, llm_trace_repo as _,
        memory as _, renderer as _, reranker as _, response as _, response_builder as _,
        router as _, runtime as _, session_memory_repo as _, swiftide_index as _, tool as _,
    };
    #[allow(unused_imports)]
    use chat::chat::{classifier as _, llm as _};

    let _: Option<chat::assistant::context_builder::ContextBuilder> =
        None::<chat::assistant::context::builder::ContextBuilder>;
    let _: Option<chat::assistant::runtime::AssistantGraphRuntime> =
        None::<chat::assistant::execution::runtime::AssistantGraphRuntime>;
}
