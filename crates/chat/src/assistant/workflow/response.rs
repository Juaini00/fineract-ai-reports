//! Maps a finished `WorkflowRunOutcome` (Task 5's caller, replacing the
//! legacy `execute_selected_capability` execution path) back into the
//! `AssistantResponse`/`ClarificationPayload` shapes chat callers already
//! expect. Kept as one cohesive responsibility: outcome -> response only, no
//! execution or persistence logic lives here.

use anyhow::{Result, anyhow};
use serde_json::Value;
use uuid::Uuid;

use crate::assistant::execution::plan::{ExecutionPlan, PolicyDecision};
use crate::assistant::{
    AssistantIntent, AssistantResponse, CLARIFICATION_VERSION_1, ClarificationKind,
    ClarificationOption, ClarificationPayload, ResponseBuilder, SourceIntentSnapshot,
    tool_request_from_plan, tool_result_from_execution,
};
use crate::knowledge::model::KnowledgeCatalog;

use super::run::WorkflowRunOutcome;
use super::state::{NodeRunStatus, WorkflowNodeRun, WorkflowStateRepository};

pub enum WorkflowResponseOutcome {
    Response(AssistantResponse),
    Clarification(ClarificationPayload),
    Failed,
}

/// `plan` and `policy` are the same atomic single-capability plan/decision
/// Task 5's caller already built to run the workflow — `ResponseBuilder::
/// from_tool_result` requires a `&ExecutionPlan` (for `dataset_selection`
/// column resolution) that has no `WorkflowRunOutcome`-derived equivalent.
pub async fn workflow_response(
    outcome: WorkflowRunOutcome,
    state: &WorkflowStateRepository,
    job_id: Uuid,
    workflow_id: Uuid,
    capability_id: &str,
    intent: &AssistantIntent,
    plan: &ExecutionPlan,
    policy: &PolicyDecision,
    catalog: &KnowledgeCatalog,
) -> Result<WorkflowResponseOutcome> {
    match outcome {
        WorkflowRunOutcome::Completed => {
            let runs = state.node_runs(job_id, workflow_id).await?;
            let output = terminal_output(&runs)
                .ok_or_else(|| anyhow!("workflow completed with no capability node output"))?;
            let execution_result = output
                .get("untrusted_tool_output")
                .cloned()
                .unwrap_or(Value::Null);
            let tool_request = tool_request_from_plan(plan, Vec::new());
            let tool_result = tool_result_from_execution(&tool_request, execution_result);

            let entity_options = matches!(
                capability_id,
                "client_name_lookup" | "client_relationship_lookup"
            )
            .then(|| client_entity_options(&tool_result.rows, policy.can_view_pii))
            .unwrap_or_default();
            if entity_options.len() > 1 {
                let payload = ClarificationPayload {
                    version: CLARIFICATION_VERSION_1,
                    id: Uuid::new_v4(),
                    revision: 1,
                    kind: ClarificationKind::SelectEntity,
                    question: "Which client did you mean?".into(),
                    options: entity_options,
                    fields: Vec::new(),
                    attempt: 1,
                    source_intent: Some(source_intent_snapshot(intent)),
                    allow_free_text: false,
                    is_missing_execution_parameters: false,
                    workflow_id: None,
                    node_id: None,
                    resume_node_id: None,
                    entity_kind: None,
                };
                return Ok(WorkflowResponseOutcome::Clarification(payload));
            }

            let response =
                ResponseBuilder::from_tool_result(intent, plan, policy, &tool_result, catalog);
            Ok(WorkflowResponseOutcome::Response(response))
        }
        WorkflowRunOutcome::WaitingForUserInput { .. } => {
            let payload = state
                .load_pending_clarification(job_id)
                .await?
                .ok_or_else(|| anyhow!("workflow is waiting but no clarification is pending"))?;
            Ok(WorkflowResponseOutcome::Clarification(payload))
        }
        WorkflowRunOutcome::Failed => Ok(WorkflowResponseOutcome::Failed),
    }
}

/// The `Complete` node's own output is always `{}` (see `run.rs`'s
/// `resolve_execution`), so the capability node's output is identified by the
/// `untrusted_tool_output` wrapper `GuardedDataTools::execute` always adds to
/// a real `ExecuteQuery`/`ResolveEntity` result — not by node id, since this
/// function only has `job_id`/`workflow_id`, not the full `ExecutionWorkflow`
/// graph to walk backward from the `Complete` node.
fn terminal_output(runs: &[WorkflowNodeRun]) -> Option<&Value> {
    runs.iter()
        .rev()
        .filter(|run| run.status == NodeRunStatus::Completed)
        .find_map(|run| {
            let output = run.output_json.as_ref()?;
            output
                .get("untrusted_tool_output")
                .is_some()
                .then_some(output)
        })
}

fn source_intent_snapshot(intent: &AssistantIntent) -> SourceIntentSnapshot {
    SourceIntentSnapshot {
        prompt: intent.reason.clone(),
        normalized_prompt: Some(intent.reason.trim().to_lowercase()),
        intent: intent.intent.clone(),
        domain: intent.domain.clone(),
        request_shape: intent.request_shape.clone(),
        entities: intent.entities.clone(),
        constraints: intent.constraints.clone(),
        context_reference: intent.context_reference.clone(),
        confidence: intent.confidence,
        reason: intent.reason.clone(),
    }
}

/// Moved from `execution/runtime/execution.rs` (Task 4): duplicate-client
/// disambiguation is pure/stateless and belongs at the response-mapping layer
/// for both the legacy and workflow execution paths, not duplicated between
/// them. `execution.rs` re-imports this until Task 6 deletes its call site.
pub(crate) fn client_entity_options(
    rows: &[Value],
    can_view_pii: bool,
) -> Vec<ClarificationOption> {
    let mut seen = std::collections::HashSet::new();
    rows.iter()
        .filter_map(|row| {
            let client_id = row.get("client_id")?.as_i64()?;
            if !seen.insert(client_id) {
                return None;
            }
            let label = if can_view_pii {
                row.get("display_name")?.as_str()?.to_owned()
            } else {
                format!("Client {client_id}")
            };
            let office = row.get("office_name").and_then(Value::as_str);
            let status = row.get("status_label").and_then(Value::as_str);
            Some(ClarificationOption {
                id: format!("client:{client_id}"),
                label,
                description: Some(
                    [office, status]
                        .into_iter()
                        .flatten()
                        .collect::<Vec<_>>()
                        .join(" · "),
                ),
                fields: Vec::new(),
            })
        })
        .collect()
}

#[cfg(test)]
mod entity_clarification_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn duplicate_client_rows_build_safe_entity_choices() {
        let rows = vec![
            json!({"client_id": 7, "display_name": "Alex Doe", "office_name": "North", "status_label": "active", "external_id": "SECRET"}),
            json!({"client_id": 8, "display_name": "Alex Doe", "office_name": "South", "status_label": "pending", "mobile_no": "SECRET"}),
        ];

        let options = client_entity_options(&rows, true);

        assert_eq!(options.len(), 2);
        assert_eq!(options[0].id, "client:7");
        assert_eq!(options[0].label, "Alex Doe");
        assert_eq!(options[0].description.as_deref(), Some("North · active"));
        assert!(!serde_json::to_string(&options).unwrap().contains("SECRET"));
    }

    #[test]
    fn entity_choices_hide_names_when_pii_is_disallowed() {
        let rows = vec![json!({
            "client_id": 7,
            "display_name": "Alex Doe",
            "office_name": "North",
            "status_label": "active"
        })];

        let options = client_entity_options(&rows, false);

        assert_eq!(options[0].label, "Client 7");
        assert!(
            !serde_json::to_string(&options)
                .unwrap()
                .contains("Alex Doe")
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assistant::execution::plan::{ExecutionPlanType, PolicyDecisionStatus};
    use crate::assistant::llm::tool::data_executor::FineractDataExecutor;
    use crate::assistant::workflow::contract::{
        CompleteNode, EdgeCondition, ExecuteQueryNode, ExecutionWorkflow, FailPolicy, Idempotency,
        NodeBudget, NodeId, NodeKind, NodePolicy, OfficeScope, OutputContract, OutputMode,
        RetryPolicy, TerminalState, WORKFLOW_CONTRACT_VERSION, WorkflowBudgets, WorkflowEdge,
        WorkflowNode,
    };
    use crate::assistant::workflow::node_executor::CapabilityNodeExecutor;
    use crate::assistant::workflow::run::WorkflowRunner;
    use crate::assistant::{
        AssistantConstraints, AssistantDomain, AssistantIntentKind, AssistantLanguage,
        ContextReference,
    };
    use crate::execution::repository::ExecutionLimits;
    use crate::knowledge::catalog::loader::KnowledgeLoader;
    use crate::knowledge::model::{KnowledgeCatalog, Sensitivity};
    use app_core::auth::model::PrincipalContext;
    use sqlx::{AssertSqlSafe, PgPool, postgres::PgPoolOptions};
    use std::sync::Arc;

    const CAPABILITY_ID: &str = "organization_hierarchy_summary";
    const QUERY_ID: &str = "organization.hierarchy_summary";

    struct DbFixture {
        pool: PgPool,
        admin_pool: PgPool,
        db_name: String,
    }

    async fn spawn_db() -> DbFixture {
        let admin_db_url = std::env::var("TEST_ADMIN_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://root:password@127.0.0.1:5432/postgres".into());
        let db_name = format!("ai_report_test_{}", Uuid::new_v4().simple());
        let admin_pool = PgPoolOptions::new()
            .max_connections(1)
            .connect(&admin_db_url)
            .await
            .unwrap_or_else(|error| {
                panic!("cannot reach Postgres at {admin_db_url}; is it running? {error}")
            });
        sqlx::query(AssertSqlSafe(format!("CREATE DATABASE \"{db_name}\"")))
            .execute(&admin_pool)
            .await
            .expect("create test database");
        let pool = PgPool::connect(&admin_db_url.replace("/postgres", &format!("/{db_name}")))
            .await
            .expect("connect test database");
        sqlx::migrate!("../../migrations")
            .run(&pool)
            .await
            .expect("run migrations");
        DbFixture {
            pool,
            admin_pool,
            db_name,
        }
    }

    impl DbFixture {
        async fn insert_user(&self) -> Uuid {
            let user_id = Uuid::new_v4();
            sqlx::query(
                "INSERT INTO users (id, username, password_hash, role) VALUES ($1, $2, 'unused', 'admin')",
            )
            .bind(user_id)
            .bind(format!("test-{user_id}"))
            .execute(&self.pool)
            .await
            .expect("insert test user");
            user_id
        }

        async fn drop_database(self) {
            drop(self.pool);
            let _ = sqlx::query(AssertSqlSafe(format!(
                "DROP DATABASE IF EXISTS \"{}\" WITH (FORCE)",
                self.db_name
            )))
            .execute(&self.admin_pool)
            .await;
        }
    }

    async fn insert_job(pool: &PgPool, user_id: Uuid) -> Uuid {
        let session_id = Uuid::new_v4();
        let job_id = Uuid::new_v4();
        sqlx::query("INSERT INTO chat_sessions (id, user_id, status) VALUES ($1,$2,'active')")
            .bind(session_id)
            .bind(user_id)
            .execute(pool)
            .await
            .expect("insert chat session");
        sqlx::query(
            "INSERT INTO chat_jobs (id, session_id, user_id, status, current_step, message, expires_at) \
             VALUES ($1,$2,$3,'queued','queued','test', now() + interval '1 hour')",
        )
        .bind(job_id)
        .bind(session_id)
        .bind(user_id)
        .execute(pool)
        .await
        .expect("insert chat job");
        sqlx::query(
            "INSERT INTO assistant_job_memory (job_id, graph_state) VALUES ($1, 'receive_message')",
        )
        .bind(job_id)
        .execute(pool)
        .await
        .expect("insert assistant job memory");
        job_id
    }

    async fn fineract_pool() -> Option<PgPool> {
        let Ok(url) = std::env::var("FINERACT_DATABASE_URL") else {
            eprintln!("skipping: FINERACT_DATABASE_URL unset");
            return None;
        };
        Some(PgPool::connect(&url).await.expect("connect fineract"))
    }

    fn catalog() -> KnowledgeCatalog {
        let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let workspace_root = manifest_dir
            .parent()
            .and_then(|path| path.parent())
            .unwrap();
        KnowledgeLoader::new(
            workspace_root.join("knowledge"),
            workspace_root.join("queries"),
        )
        .load()
        .expect("load catalog")
    }

    fn admin_principal(user_id: Uuid) -> PrincipalContext {
        PrincipalContext {
            user_id,
            role: "admin".into(),
            capability_ids: vec![CAPABILITY_ID.into()],
            office_ids: vec![1],
            can_view_pii: true,
            legacy_api_key_id: None,
        }
    }

    fn allowed_policy() -> PolicyDecision {
        PolicyDecision {
            status: PolicyDecisionStatus::Allowed,
            reason: None,
            office_ids: vec![1],
            can_view_pii: true,
        }
    }

    fn plan() -> ExecutionPlan {
        ExecutionPlan {
            plan_type: ExecutionPlanType::Atomic,
            domain: "organization".into(),
            capability: CAPABILITY_ID.into(),
            query_id: QUERY_ID.into(),
            dataset_selection: None,
            output_mode: "table".into(),
            params: serde_json::json!({}),
            retrieval_plan: Default::default(),
            evidence_evaluation: Default::default(),
            requires_policy_check: true,
        }
    }

    fn intent() -> AssistantIntent {
        AssistantIntent {
            intent: AssistantIntentKind::DataLookup,
            domain: AssistantDomain::Organization,
            request_shape: Default::default(),
            language: AssistantLanguage::En,
            canonical_query_en: String::new(),
            entities: Vec::new(),
            constraints: AssistantConstraints::default(),
            context_reference: ContextReference::None,
            source: None,
            confidence: 1.0,
            reason: "test".into(),
        }
    }

    fn query_workflow() -> ExecutionWorkflow {
        let query_node = WorkflowNode {
            id: NodeId::new("query").unwrap(),
            kind: NodeKind::ExecuteQuery(ExecuteQueryNode {
                capability_id: Some(CAPABILITY_ID.into()),
                dataset_id: None,
                shape_id: None,
                query_id: Some(QUERY_ID.into()),
                iterate_over: None,
            }),
            inputs: vec![],
            outputs: vec![],
            policy: NodePolicy {
                required_capability: Some(CAPABILITY_ID.into()),
                office_scope: OfficeScope::AuthorizedIntersection,
                max_sensitivity: Sensitivity::FilterOnly,
                pii_required: false,
            },
            budget: NodeBudget {
                timeout_ms: 3_000,
                row_cap: 50,
                query_cost: 1,
            },
            idempotency: Idempotency::Replayable,
            retry: RetryPolicy { max_attempts: 0 },
        };
        let finish_id = NodeId::new("finish").unwrap();
        let finish_node = WorkflowNode {
            id: finish_id.clone(),
            kind: NodeKind::Complete(CompleteNode {
                terminal: TerminalState::Success,
            }),
            inputs: vec![],
            outputs: vec![],
            policy: NodePolicy {
                required_capability: None,
                office_scope: OfficeScope::AuthorizedIntersection,
                max_sensitivity: Sensitivity::FilterOnly,
                pii_required: false,
            },
            budget: NodeBudget {
                timeout_ms: 0,
                row_cap: 0,
                query_cost: 0,
            },
            idempotency: Idempotency::Pure,
            retry: RetryPolicy { max_attempts: 0 },
        };
        ExecutionWorkflow {
            id: Uuid::new_v4(),
            contract_version: WORKFLOW_CONTRACT_VERSION,
            catalog_version: Uuid::nil(),
            nodes: vec![query_node, finish_node],
            edges: vec![WorkflowEdge {
                from: NodeId::new("query").unwrap(),
                to: finish_id,
                condition: EdgeCondition::Always,
            }],
            budgets: WorkflowBudgets {
                shared_timeout_ms: 30_000,
                shared_row_cap: 1_000,
                max_query_count: 5,
                max_parallel_queries: 1,
                max_model_turns: 2,
                max_node_retries: 0,
            },
            fail_policy: FailPolicy::FailFast,
            output_contract: OutputContract {
                mode: OutputMode::Table,
                allows_partial: false,
                max_sensitivity: Sensitivity::FilterOnly,
            },
        }
    }

    #[tokio::test]
    async fn workflow_response_matches_from_tool_result_for_a_completed_run() {
        let Some(fineract) = fineract_pool().await else {
            return;
        };
        let db = spawn_db().await;
        let user_id = db.insert_user().await;
        let job_id = insert_job(&db.pool, user_id).await;
        let state = WorkflowStateRepository::new(db.pool.clone());
        let response_state = WorkflowStateRepository::new(db.pool.clone());

        let workflow = query_workflow();
        state
            .install_workflow(job_id, user_id, &workflow)
            .await
            .expect("install workflow");

        let catalog = Arc::new(catalog());
        let principal = admin_principal(user_id);
        let executor = FineractDataExecutor::new(
            fineract,
            catalog.clone(),
            allowed_policy(),
            ExecutionLimits::default(),
            None,
        );
        let node_executor = CapabilityNodeExecutor::new(
            executor,
            principal.clone(),
            catalog.clone(),
            state.clone(),
            job_id,
            workflow.clone(),
        );
        let runner = WorkflowRunner::new(state, node_executor, catalog.clone());

        let outcome = runner
            .run(job_id, user_id, &principal, &workflow)
            .await
            .expect("workflow runs to completion");
        assert_eq!(outcome, WorkflowRunOutcome::Completed);

        let plan = plan();
        let intent = intent();
        let policy = allowed_policy();

        let mapped = workflow_response(
            outcome,
            &response_state,
            job_id,
            workflow.id,
            CAPABILITY_ID,
            &intent,
            &plan,
            &policy,
            &catalog,
        )
        .await
        .expect("workflow_response maps a completed run");

        let WorkflowResponseOutcome::Response(response) = mapped else {
            panic!("expected a Response outcome for a completed non-ambiguous run");
        };

        // Reconstruct what `ResponseBuilder::from_tool_result` produces
        // directly from the same node output, and assert they match.
        let runs = response_state
            .node_runs(job_id, workflow.id)
            .await
            .expect("load node runs");
        let query_run = runs
            .iter()
            .find(|run| run.node_id.as_str() == "query")
            .expect("query node has a run row");
        let execution_result = query_run
            .output_json
            .as_ref()
            .and_then(|value| value.get("untrusted_tool_output"))
            .cloned()
            .expect("query node output carries untrusted_tool_output");
        let tool_request = crate::assistant::tool_request_from_plan(&plan, Vec::new());
        let tool_result =
            crate::assistant::tool_result_from_execution(&tool_request, execution_result);
        let expected =
            ResponseBuilder::from_tool_result(&intent, &plan, &policy, &tool_result, &catalog);

        assert_eq!(response.table, expected.table);
        assert_eq!(response.message, expected.message);
        assert_eq!(response.rendered_markdown, expected.rendered_markdown);

        db.drop_database().await;
    }
}
