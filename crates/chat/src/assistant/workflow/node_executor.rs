//! `WorkflowNodeExecutor` adapter that routes `ExecuteQuery`/`ResolveEntity`
//! nodes through the same server-side guard (`GuardedDataTools`) the LLM tool
//! layer uses, backed by `FineractDataExecutor` (Task 1).
//!
//! Plumbing note: `WorkflowNodeExecutor::execute` only receives `node` and
//! `bindings` (fixed trait signature — other callers depend on it as-is), but
//! `GuardedDataTools::execute_approved_capability` needs the full
//! `&ExecutionWorkflow` and current `&[WorkflowNodeRun]` to do its
//! membership/budget checks. `WorkflowNode` carries no `job_id`/`workflow_id`
//! field, so there is no way to derive either from the trait call's
//! arguments alone. Resolution: `WorkflowRunner` is constructed fresh per job
//! (see `WorkflowRunner::new` call sites), so `CapabilityNodeExecutor` is
//! constructed the same way — `job_id` and `workflow` are captured once at
//! construction (the workflow's node/edge/budget shape does not change
//! mid-run), and `runs` is re-fetched from `WorkflowStateRepository` on every
//! `execute()` call so budget/membership checks always see the latest
//! completed set, including runs completed earlier in the same `run()` loop.

use std::collections::BTreeMap;
use std::sync::Arc;

use anyhow::{Result, anyhow};
use app_core::auth::model::PrincipalContext;
use async_trait::async_trait;
use serde_json::Value;
use uuid::Uuid;

use crate::assistant::llm::tool::data::{DataToolRequest, GuardedDataTools};
use crate::assistant::llm::tool::data_executor::FineractDataExecutor;
use crate::assistant::{CLARIFICATION_VERSION_1, ClarificationKind, ClarificationPayload};
use crate::knowledge::model::KnowledgeCatalog;

use super::contract::{ExecutionWorkflow, NodeKind, WorkflowNode};
use super::run::{NodeExecution, WorkflowNodeExecutor};
use super::state::WorkflowStateRepository;

pub struct CapabilityNodeExecutor {
    guard: GuardedDataTools<FineractDataExecutor>,
    principal: PrincipalContext,
    catalog: Arc<KnowledgeCatalog>,
    state: WorkflowStateRepository,
    job_id: Uuid,
    workflow: ExecutionWorkflow,
}

impl CapabilityNodeExecutor {
    pub fn new(
        executor: FineractDataExecutor,
        principal: PrincipalContext,
        catalog: Arc<KnowledgeCatalog>,
        state: WorkflowStateRepository,
        job_id: Uuid,
        workflow: ExecutionWorkflow,
    ) -> Self {
        Self {
            guard: GuardedDataTools::new(executor),
            principal,
            catalog,
            state,
            job_id,
            workflow,
        }
    }
}

#[async_trait]
impl WorkflowNodeExecutor for CapabilityNodeExecutor {
    async fn execute(
        &self,
        node: &WorkflowNode,
        bindings: &BTreeMap<String, Value>,
    ) -> Result<NodeExecution> {
        // `WorkflowRunner::resolve_execution` handles `CardinalityBranch`,
        // `Complete`, and `ComposeResult` inline, and routes everything else
        // (`ExecuteQuery`, `ResolveEntity`, `ClarificationInterrupt`) here.
        if matches!(node.kind, NodeKind::ClarificationInterrupt(_)) {
            // The caller (`WorkflowRunner::execute_node`) overwrites
            // `workflow_id`/`node_id`/`entity_kind`/`resume_node_id` on the
            // returned payload from the node/graph itself, so only the
            // user-facing question content needs to be filled in here.
            let kind = match &node.kind {
                NodeKind::ClarificationInterrupt(interrupt) => {
                    match interrupt.clarification_kind.as_str() {
                        "select_option" => ClarificationKind::SelectOption,
                        "select_entity" => ClarificationKind::SelectEntity,
                        "free_text" => ClarificationKind::FreeText,
                        _ => ClarificationKind::CollectFields,
                    }
                }
                _ => unreachable!(),
            };
            return Ok(NodeExecution::Waiting {
                clarification: Box::new(ClarificationPayload {
                    version: CLARIFICATION_VERSION_1,
                    id: Uuid::new_v4(),
                    revision: 0,
                    kind,
                    question: "Additional information is needed to continue.".into(),
                    options: vec![],
                    fields: vec![],
                    attempt: 0,
                    source_intent: None,
                    allow_free_text: false,
                    is_missing_execution_parameters: true,
                    workflow_id: None,
                    node_id: None,
                    resume_node_id: None,
                    entity_kind: None,
                }),
            });
        }

        let capability_id = match &node.kind {
            NodeKind::ExecuteQuery(query) => query.capability_id.clone(),
            NodeKind::ResolveEntity(_) => node.policy.required_capability.clone(),
            other => unreachable!("WorkflowRunner never routes {other:?} to WorkflowNodeExecutor"),
        }
        .ok_or_else(|| anyhow!("workflow node has no declared capability id"))?;

        let request = DataToolRequest {
            node_id: node.id.clone(),
            capability_id,
            parameters: bindings.clone(),
            timeout_ms: node.budget.timeout_ms,
            row_cap: node.budget.row_cap,
        };

        let runs = self.state.node_runs(self.job_id, self.workflow.id).await?;

        match self
            .guard
            .execute_approved_capability(
                &self.workflow,
                &runs,
                &self.principal,
                &self.catalog,
                request,
            )
            .await
        {
            Ok(output) => {
                let rows_returned = output
                    .get("untrusted_tool_output")
                    .and_then(|value| value.get("rows"))
                    .and_then(Value::as_array)
                    .map_or(0, |rows| rows.len() as i32);
                Ok(NodeExecution::Completed {
                    output,
                    rows_returned,
                })
            }
            Err(rejection) => {
                tracing::warn!(
                    node_id = %node.id,
                    ?rejection,
                    "capability node execution rejected by GuardedDataTools"
                );
                Ok(NodeExecution::Failed)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    //! Integration-style coverage: a real `WorkflowRunner<CapabilityNodeExecutor>`
    //! against a fresh app-schema Postgres database (job/workflow state) and a
    //! live Fineract-shaped pool (data plane), mirroring the harness in
    //! `crates/chat/tests/workflow_parallel_budget.rs`. Lives here rather than
    //! in `crates/chat/tests/` because `FineractDataExecutor::new` is
    //! `pub(crate)` (see Task 1) and is therefore invisible to the external
    //! integration-test crate. `FINERACT_DATABASE_URL` gates the test the same
    //! way `crates/chat/tests/query_budget.rs` does: skip (pass trivially)
    //! when it isn't set, rather than fail the whole suite in environments
    //! without a Fineract-shaped database.
    use super::*;
    use crate::assistant::execution::plan::{PolicyDecision, PolicyDecisionStatus};
    use crate::assistant::workflow::{
        ClarificationInterruptNode, CompleteNode, EdgeCondition, ExecuteQueryNode, FailPolicy,
        Idempotency, NodeBudget, NodeId, NodePolicy, NodeRunStatus, OfficeScope, OutputContract,
        OutputMode, RetryPolicy, TerminalState, WORKFLOW_CONTRACT_VERSION, WorkflowBudgets,
        WorkflowEdge, WorkflowRunOutcome, WorkflowRunner,
    };
    use crate::execution::repository::ExecutionLimits;
    use crate::knowledge::catalog::loader::KnowledgeLoader;
    use crate::knowledge::model::Sensitivity;
    use sqlx::{AssertSqlSafe, PgPool, postgres::PgPoolOptions};

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

    /// Single-node workflow whose entry node is a `ClarificationInterrupt` —
    /// exercises `resolve_execution`'s `_` arm, which routes this kind to
    /// `WorkflowNodeExecutor::execute` same as `ExecuteQuery`/`ResolveEntity`.
    fn clarification_workflow() -> ExecutionWorkflow {
        let clarify_id = NodeId::new("clarify_amount").unwrap();
        let clarify_node = WorkflowNode {
            id: clarify_id.clone(),
            kind: NodeKind::ClarificationInterrupt(ClarificationInterruptNode {
                clarification_kind: "collect_fields".into(),
                option_source: NodeId::new("query").unwrap(),
                resume: NodeId::new("query").unwrap(),
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
            idempotency: Idempotency::Replayable,
            retry: RetryPolicy { max_attempts: 0 },
        };
        ExecutionWorkflow {
            id: Uuid::new_v4(),
            contract_version: WORKFLOW_CONTRACT_VERSION,
            catalog_version: Uuid::nil(),
            nodes: vec![clarify_node],
            edges: vec![],
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
    async fn capability_node_executor_waits_on_clarification_interrupt_node() {
        // No `FINERACT_DATABASE_URL` gate: a `ClarificationInterrupt` node
        // never reaches `GuardedDataTools`, so the Fineract pool is unused —
        // `db.pool` is passed only to satisfy `FineractDataExecutor::new`.
        let db = spawn_db().await;
        let user_id = db.insert_user().await;
        let job_id = insert_job(&db.pool, user_id).await;
        let state = WorkflowStateRepository::new(db.pool.clone());

        let workflow = clarification_workflow();
        state
            .install_workflow(job_id, user_id, &workflow)
            .await
            .expect("install workflow");

        let catalog = Arc::new(catalog());
        let principal = admin_principal(user_id);
        let executor = FineractDataExecutor::new(
            db.pool.clone(),
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
        let runner = WorkflowRunner::new(state, node_executor, catalog);

        let outcome = runner
            .run(job_id, user_id, &principal, &workflow)
            .await
            .expect("workflow pauses for clarification, not panics");
        assert_eq!(
            outcome,
            WorkflowRunOutcome::WaitingForUserInput {
                node_id: NodeId::new("clarify_amount").unwrap()
            }
        );

        db.drop_database().await;
    }

    const SENSITIVE_CAPABILITY_ID: &str = "savings_account_identity_lookup";
    const SENSITIVE_QUERY_ID: &str = "savings.account_identity_lookup";

    fn sensitive_lookup_principal(user_id: Uuid) -> PrincipalContext {
        PrincipalContext {
            user_id,
            role: "admin".into(),
            capability_ids: vec![SENSITIVE_CAPABILITY_ID.into()],
            office_ids: vec![1],
            can_view_pii: true,
            legacy_api_key_id: None,
        }
    }

    /// Single-node `ExecuteQuery` workflow whose `account_number` input is
    /// declared `BindingSource::ExactSensitiveInput` — proves the value
    /// reaches the real SQL bind (out-of-band, via `FineractDataExecutor`'s
    /// constructor field) without ever passing through `bindings`/
    /// `DataToolRequest.parameters`.
    fn sensitive_lookup_workflow() -> ExecutionWorkflow {
        use crate::assistant::workflow::contract::{BindingSource, NodeInput};
        use crate::knowledge::catalog::parameter_policy::ParameterType;

        let query_node = WorkflowNode {
            id: NodeId::new("query").unwrap(),
            kind: NodeKind::ExecuteQuery(ExecuteQueryNode {
                capability_id: Some(SENSITIVE_CAPABILITY_ID.into()),
                dataset_id: None,
                shape_id: None,
                query_id: Some(SENSITIVE_QUERY_ID.into()),
                iterate_over: None,
            }),
            inputs: vec![
                NodeInput {
                    parameter: "office_ids".into(),
                    kind: ParameterType::IntegerArray,
                    source: BindingSource::AuthorizedScope,
                },
                NodeInput {
                    parameter: "account_number".into(),
                    kind: ParameterType::String,
                    source: BindingSource::ExactSensitiveInput,
                },
            ],
            outputs: vec![],
            policy: NodePolicy {
                required_capability: Some(SENSITIVE_CAPABILITY_ID.into()),
                office_scope: OfficeScope::AuthorizedIntersection,
                max_sensitivity: Sensitivity::Pii,
                pii_required: true,
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
                max_sensitivity: Sensitivity::Pii,
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
                max_sensitivity: Sensitivity::Pii,
            },
        }
    }

    /// Proves the sensitive-identifier threading end to end: the real value
    /// never travels through `bindings_for`/`DataToolRequest.parameters`
    /// (`run.rs`'s `ExactSensitiveInput` arm is skipped, not `Null`-padded),
    /// yet the query still resolves the exact row because
    /// `FineractDataExecutor` carries the identifier out-of-band via its own
    /// constructor field straight into `execute_plan_with_sensitive`.
    #[tokio::test]
    async fn capability_node_executor_resolves_exact_sensitive_identifier() {
        let Some(fineract) = fineract_pool().await else {
            return;
        };
        let Some(real_account_no) =
            sqlx::query_scalar::<_, String>("SELECT account_no FROM m_savings_account LIMIT 1")
                .fetch_optional(&fineract)
                .await
                .expect("query fineract for a savings account fixture")
        else {
            eprintln!("skipping: no savings accounts in Fineract fixture data");
            return;
        };

        let db = spawn_db().await;
        let user_id = db.insert_user().await;
        let job_id = insert_job(&db.pool, user_id).await;
        let state = WorkflowStateRepository::new(db.pool.clone());
        let inspect_state = WorkflowStateRepository::new(db.pool.clone());

        let workflow = sensitive_lookup_workflow();
        state
            .install_workflow(job_id, user_id, &workflow)
            .await
            .expect("install workflow");

        let catalog = Arc::new(catalog());
        let principal = sensitive_lookup_principal(user_id);

        let (_, sensitive_identifier) =
            crate::assistant::understanding::extraction::identifier_intake(&format!(
                "savings account number {real_account_no}"
            ))
            .into_parts();
        let sensitive_identifier =
            sensitive_identifier.expect("marker parses the fixture account number back out");

        let executor = FineractDataExecutor::new(
            fineract,
            catalog.clone(),
            allowed_policy(),
            ExecutionLimits::default(),
            Some(sensitive_identifier),
        );
        let node_executor = CapabilityNodeExecutor::new(
            executor,
            principal.clone(),
            catalog.clone(),
            state.clone(),
            job_id,
            workflow.clone(),
        );
        let runner = WorkflowRunner::new(state, node_executor, catalog);

        let outcome = runner
            .run(job_id, user_id, &principal, &workflow)
            .await
            .expect("workflow runs to completion");
        assert_eq!(outcome, WorkflowRunOutcome::Completed);

        let runs = inspect_state
            .node_runs(job_id, workflow.id)
            .await
            .expect("load node runs");
        let query_run = runs
            .iter()
            .find(|run| run.node_id.as_str() == "query")
            .expect("query node has a run row");
        assert_eq!(query_run.status, NodeRunStatus::Completed);
        assert_eq!(
            query_run.rows_returned, 1,
            "exact account-number lookup must return exactly the matching row \
             (proves the real value, not a Null placeholder, reached the SQL bind)"
        );

        // (b) `node.inputs` declares `account_number` as `ExactSensitiveInput`,
        // so `persisted_output` (state.rs) redacts the *entire* executor
        // output down to `{"typed_output": null}` before it is written to
        // `chat_workflow_node_runs.output_json` — the untrusted rows (and
        // therefore the raw account number) never reach durable storage at
        // all, not merely "the raw string happens to be absent".
        let output = query_run
            .output_json
            .as_ref()
            .expect("completed node persists output");
        assert_eq!(
            output.get("untrusted_tool_output"),
            None,
            "ExactSensitiveInput node output must be redacted before persistence"
        );
        let persisted = serde_json::to_string(&query_run).expect("serialize persisted run");
        assert!(
            !persisted.contains(&real_account_no),
            "raw sensitive value must never be persisted in chat_workflow_node_runs"
        );

        db.drop_database().await;
    }

    #[tokio::test]
    async fn capability_node_executor_completes_a_real_approved_capability() {
        let Some(fineract) = fineract_pool().await else {
            return;
        };
        let db = spawn_db().await;
        let user_id = db.insert_user().await;
        let job_id = insert_job(&db.pool, user_id).await;
        let state = WorkflowStateRepository::new(db.pool.clone());
        let inspect_state = WorkflowStateRepository::new(db.pool.clone());

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
        let runner = WorkflowRunner::new(state, node_executor, catalog);

        let outcome = runner
            .run(job_id, user_id, &principal, &workflow)
            .await
            .expect("workflow runs to completion");
        assert_eq!(outcome, WorkflowRunOutcome::Completed);

        let runs = inspect_state
            .node_runs(job_id, workflow.id)
            .await
            .expect("load node runs");
        let query_run = runs
            .iter()
            .find(|run| run.node_id.as_str() == "query")
            .expect("query node has a run row");
        assert_eq!(query_run.status, NodeRunStatus::Completed);
        assert!(query_run.rows_returned >= 0);

        db.drop_database().await;
    }
}
