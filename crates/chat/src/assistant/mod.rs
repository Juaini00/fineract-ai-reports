pub mod context;
pub mod execution;
pub mod legacy_pipeline;
pub mod llm;
pub mod presentation;
pub mod retrieval;
pub mod state;
pub mod understanding;

pub use context::{
    builder as context_builder, canonical_state,
    canonical_state_repository as canonical_state_repo, clarification, clarification_planner,
};
pub use execution::{runtime, tool};
pub use llm::router;
pub use presentation::{builder as response_builder, contracts, renderer, response};
pub use retrieval::{evidence, reranker};
pub use state::{graph, memory};
pub use understanding::{clarification_resolver, extraction, intent};

pub use crate::audit::llm_trace_repository as llm_trace_repo;
pub use crate::audit::llm_trace_repository::{LlmTrace, LlmTraceRecord, LlmTraceRepository};
pub use crate::conversation::repository::assistant_memory as session_memory_repo;
pub use crate::conversation::repository::assistant_memory::SessionMemoryRepository;
pub use crate::job::repository::assistant_memory as job_memory_repo;
pub use crate::job::repository::assistant_memory::{GraphCheckpoint, JobMemoryRepository};
pub use crate::knowledge::index::swiftide as swiftide_index;
pub use crate::knowledge::index::swiftide::{SwiftideIndexPipeline, SwiftideKnowledgeDocument};
pub use clarification::{
    CLARIFICATION_VERSION_1, ClarificationChoice, ClarificationField, ClarificationFieldType,
    ClarificationKind, ClarificationOption, ClarificationOutcome, ClarificationPayload,
    ClarificationValidation, ClarificationView, OTHER_CLARIFICATION_OPTION_ID,
    PendingClarification,
};
pub use context::builder::ContextBuilder;
pub use context::canonical_state::*;
pub use context::canonical_state_repository::*;
pub use context::{
    ClarificationFacts, ClarificationPlanResult, ClarificationPlanner, ContextMessage,
    ContextSourceSnippet, ContextWarning, ContextWarningCode, ContextWindow, ContextWindowPolicy,
    RelevantJobSummary,
};
pub use evidence::{Evidence, RetrievalPlan};
pub use execution::tool::{
    ToolRequest, ToolResult, ToolValidationError, guard_selected_capability,
    plan_selected_capability, plan_selected_capability_verified, tool_request_from_plan,
    tool_result_from_execution,
};
pub use intent::{
    AssistantConstraints, AssistantDomain, AssistantEntity, AssistantEntityType, AssistantIntent,
    AssistantIntentKind, AssistantLanguage, ContextReference, Quantity, RequestGrouping,
    RequestOperation, RequestOutput, RequestPii, RequestShape, RequestSubject,
    SourceIntentSnapshot,
};
pub use llm::router::SemanticRouter;
pub use llm::{FakeLlmClient, LlmClient, LlmPurpose, LlmResponse, SharedLlmClient, TokenUsage};
pub use memory::{JobMemory, MemoryDelta, SessionMemory};
pub use presentation::builder::ResponseBuilder;
pub use presentation::contracts::{assistant_contract_names, assistant_contract_schemas};
pub use presentation::renderer::{MarkdownRenderer, ResponseRenderer};
pub use reranker::{LlmReranker, RerankerDecision, RerankerVerdict};
pub use response::{AssistantResponse, AssistantResponseType, EvidenceReference};
pub use retrieval::RetrievalEngine;
pub use runtime::{AssistantGraphRuntime, GraphRuntimeResult, RuntimeUserInput};
pub use state::graph::{
    AssistantGraphTopology, GraphState, GraphTransition, TerminalState, TransitionRule,
};
pub use understanding::clarification_resolver::ClarificationResolver;
pub use understanding::extraction::{
    DeterministicExtraction, PayloadCandidate, PayloadField, PayloadSource, PayloadTrust,
    TemporalProvenance, TemporalValidationError, extract_message_facts, extract_message_facts_at,
};
