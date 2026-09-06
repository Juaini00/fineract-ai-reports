//! Coverage for Phase 4 of the workflow runtime (docs/issues/active/012):
//! `WorkflowStateRepository::resume` stale/mismatch rejection across all five
//! durable identity dimensions, a simulated process-restart between a probe
//! node completing and the resume happening, and lineage reconstruction of a
//! `CardinalityBranch` decision from the persisted checkpoint/event trail.

// Deliberately does NOT use `mod common` / `spawn_app`: that harness boots the
// full `ChatAppState`, which unconditionally attempts a catalog embedding
// sync against the brand-new (always-stale) throwaway DB and requires a real
// Voyage AI key even with `catalog.sync_on_startup = false` — unrelated to
// anything under test here. These tests only need a migrated Postgres and the
// `WorkflowStateRepository`/`WorkflowRunner` surface, so they follow the
// lighter direct-migration pattern already used by `ownership_migration.rs`
// in this suite instead.

use std::collections::BTreeMap;

use app_core::auth::model::PrincipalContext;
use async_trait::async_trait;
use chat::assistant::ClarificationKind;
use chat::assistant::ClarificationPayload;
use chat::assistant::workflow::{
    Cardinality, CardinalityBranchNode, CompleteNode, EdgeCondition, ExecuteQueryNode,
    ExecutionWorkflow, FailPolicy, Idempotency, NodeBudget, NodeExecution, NodeId, NodeKind,
    NodePolicy, NodeRunStatus, OfficeScope, OutputContract, OutputMode, ResumeOutcome, RetryPolicy,
    WORKFLOW_CONTRACT_VERSION, WorkflowBudgets, WorkflowEdge, WorkflowNode, WorkflowNodeExecutor,
    WorkflowResumeRequest, WorkflowRunOutcome, WorkflowRunner, WorkflowStateRepository,
};
use chat::knowledge::catalog::loader::KnowledgeLoader;
use chat::knowledge::model::{KnowledgeCatalog, Sensitivity};
use serde_json::{Value, json};
use sqlx::{AssertSqlSafe, PgPool, postgres::PgPoolOptions};
use uuid::Uuid;

/// A throwaway migrated Postgres database, dropped at the end of the test.
/// Lighter than the full `spawn_app` harness — no HTTP server, no
/// `ChatAppState`, no embedding sync.
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

fn budget() -> NodeBudget {
    NodeBudget {
        timeout_ms: 10,
        row_cap: 10,
        query_cost: 0,
    }
}
fn policy() -> NodePolicy {
    NodePolicy {
        required_capability: None,
        office_scope: OfficeScope::AuthorizedIntersection,
        max_sensitivity: Sensitivity::Pii,
        pii_required: false,
    }
}
fn complete_node(id: &str) -> WorkflowNode {
    WorkflowNode {
        id: NodeId::new(id).unwrap(),
        kind: NodeKind::Complete(CompleteNode {
            terminal: chat::assistant::workflow::TerminalState::Success,
        }),
        inputs: vec![],
        outputs: vec![],
        policy: policy(),
        budget: budget(),
        idempotency: Idempotency::Pure,
        retry: RetryPolicy { max_attempts: 0 },
    }
}
fn workflow_budgets() -> WorkflowBudgets {
    WorkflowBudgets {
        shared_timeout_ms: 30_000,
        shared_row_cap: 1_000,
        max_query_count: 10,
        max_parallel_queries: 2,
        max_model_turns: 2,
        max_node_retries: 0,
    }
}
fn build_workflow(nodes: Vec<WorkflowNode>, edges: Vec<WorkflowEdge>) -> ExecutionWorkflow {
    ExecutionWorkflow {
        id: Uuid::new_v4(),
        contract_version: WORKFLOW_CONTRACT_VERSION,
        catalog_version: Uuid::nil(),
        nodes,
        edges,
        budgets: workflow_budgets(),
        fail_policy: FailPolicy::FailFast,
        output_contract: OutputContract {
            mode: OutputMode::Table,
            allows_partial: false,
            max_sensitivity: Sensitivity::Pii,
        },
    }
}
fn single_node_workflow() -> ExecutionWorkflow {
    build_workflow(vec![complete_node("wait")], vec![])
}
fn two_node_workflow() -> ExecutionWorkflow {
    build_workflow(
        vec![complete_node("probe"), complete_node("wait")],
        vec![WorkflowEdge {
            from: NodeId::new("probe").unwrap(),
            to: NodeId::new("wait").unwrap(),
            condition: EdgeCondition::Always,
        }],
    )
}
fn clarification_payload() -> ClarificationPayload {
    ClarificationPayload {
        version: 1,
        id: Uuid::new_v4(),
        revision: 0,
        kind: ClarificationKind::SelectOption,
        question: "Which office?".into(),
        options: vec![],
        fields: vec![],
        attempt: 1,
        source_intent: None,
        allow_free_text: false,
        is_missing_execution_parameters: false,
        workflow_id: None,
        node_id: None,
        resume_node_id: None,
        entity_kind: None,
    }
}

/// Installs a single-node workflow, begins its only node, and pauses it —
/// producing a real pending clarification with a real id/revision the way
/// `WorkflowRunner` would on a `ClarificationInterrupt` node.
async fn paused_fixture(
    db: &DbFixture,
) -> (
    WorkflowStateRepository,
    Uuid,
    Uuid,
    ExecutionWorkflow,
    NodeId,
    ClarificationPayload,
    i64,
) {
    let user_id = db.insert_user().await;
    let job_id = insert_job(&db.pool, user_id).await;
    let state = WorkflowStateRepository::new(db.pool.clone());
    let workflow = single_node_workflow();
    state
        .install_workflow(job_id, user_id, &workflow)
        .await
        .expect("install workflow");
    let node_id = NodeId::new("wait").unwrap();
    state
        .begin_node(job_id, workflow.id, &node_id, 0, json!({}))
        .await
        .expect("begin node");
    let clarification = clarification_payload();
    state
        .mark_workflow_paused(job_id, user_id, workflow.id, &node_id, &clarification)
        .await
        .expect("pause workflow");
    let revision: i64 = sqlx::query_scalar("SELECT workflow_revision FROM chat_jobs WHERE id = $1")
        .bind(job_id)
        .fetch_one(&db.pool)
        .await
        .expect("read workflow revision");
    (
        state,
        job_id,
        user_id,
        workflow,
        node_id,
        clarification,
        revision,
    )
}

fn valid_request(
    job_id: Uuid,
    user_id: Uuid,
    workflow: &ExecutionWorkflow,
    node_id: &NodeId,
    clarification: &ClarificationPayload,
    revision: i64,
) -> WorkflowResumeRequest {
    WorkflowResumeRequest {
        job_id,
        user_id,
        workflow_id: workflow.id,
        node_id: node_id.clone(),
        clarification_id: clarification.id,
        workflow_revision: revision,
        selected_value: json!({"choice": "a"}),
    }
}

#[tokio::test]
async fn resume_rejects_stale_job_id() {
    let db = spawn_db().await;
    let (state, job_id, user_id, workflow, node_id, clarification, revision) =
        paused_fixture(&db).await;
    let mut request = valid_request(
        job_id,
        user_id,
        &workflow,
        &node_id,
        &clarification,
        revision,
    );
    request.job_id = Uuid::new_v4();
    let outcome = state.resume(request).await.expect("resume executes");
    assert_eq!(outcome, ResumeOutcome::NotFound);
    db.drop_database().await;
}

#[tokio::test]
async fn resume_rejects_stale_workflow_id() {
    let db = spawn_db().await;
    let (state, job_id, user_id, workflow, node_id, clarification, revision) =
        paused_fixture(&db).await;
    let mut request = valid_request(
        job_id,
        user_id,
        &workflow,
        &node_id,
        &clarification,
        revision,
    );
    request.workflow_id = Uuid::new_v4();
    let outcome = state.resume(request).await.expect("resume executes");
    assert_eq!(outcome, ResumeOutcome::Stale);
    db.drop_database().await;
}

#[tokio::test]
async fn resume_rejects_stale_node_id() {
    let db = spawn_db().await;
    let (state, job_id, user_id, workflow, node_id, clarification, revision) =
        paused_fixture(&db).await;
    let mut request = valid_request(
        job_id,
        user_id,
        &workflow,
        &node_id,
        &clarification,
        revision,
    );
    request.node_id = NodeId::new("not_the_paused_node").unwrap();
    let outcome = state.resume(request).await.expect("resume executes");
    assert_eq!(outcome, ResumeOutcome::Stale);
    db.drop_database().await;
}

#[tokio::test]
async fn resume_rejects_stale_clarification_id() {
    let db = spawn_db().await;
    let (state, job_id, user_id, workflow, node_id, clarification, revision) =
        paused_fixture(&db).await;
    let mut request = valid_request(
        job_id,
        user_id,
        &workflow,
        &node_id,
        &clarification,
        revision,
    );
    request.clarification_id = Uuid::new_v4();
    let outcome = state.resume(request).await.expect("resume executes");
    assert_eq!(outcome, ResumeOutcome::Stale);
    db.drop_database().await;
}

#[tokio::test]
async fn resume_rejects_stale_workflow_revision() {
    let db = spawn_db().await;
    let (state, job_id, user_id, workflow, node_id, clarification, revision) =
        paused_fixture(&db).await;
    let mut request = valid_request(
        job_id,
        user_id,
        &workflow,
        &node_id,
        &clarification,
        revision,
    );
    request.workflow_revision = revision + 1;
    let outcome = state.resume(request).await.expect("resume executes");
    assert_eq!(outcome, ResumeOutcome::Stale);
    db.drop_database().await;
}

#[tokio::test]
async fn resume_with_all_five_identities_correct_succeeds() {
    // Sanity check for the five rejection tests above: the untouched request
    // (every identity dimension correct) actually resumes, so a Stale/NotFound
    // result in the mutated tests is proof of the guard, not a broken fixture.
    let db = spawn_db().await;
    let (state, job_id, user_id, workflow, node_id, clarification, revision) =
        paused_fixture(&db).await;
    let request = valid_request(
        job_id,
        user_id,
        &workflow,
        &node_id,
        &clarification,
        revision,
    );
    let outcome = state.resume(request).await.expect("resume executes");
    assert_eq!(outcome, ResumeOutcome::Resumed);
    db.drop_database().await;
}

#[tokio::test]
async fn restart_between_probe_completion_and_resume_does_not_duplicate_probe_row() {
    let db = spawn_db().await;
    let user_id = db.insert_user().await;
    let job_id = insert_job(&db.pool, user_id).await;
    let state = WorkflowStateRepository::new(db.pool.clone());
    let workflow = two_node_workflow();
    state
        .install_workflow(job_id, user_id, &workflow)
        .await
        .expect("install workflow");

    let probe_id = NodeId::new("probe").unwrap();
    let probe_run = state
        .begin_node(job_id, workflow.id, &probe_id, 0, json!({}))
        .await
        .expect("begin probe node");
    state
        .complete_node(&probe_run, json!({"row_count": 1}), 1, 5)
        .await
        .expect("complete probe node");

    let wait_id = NodeId::new("wait").unwrap();
    state
        .begin_node(job_id, workflow.id, &wait_id, 0, json!({}))
        .await
        .expect("begin wait node");
    let clarification = clarification_payload();
    state
        .mark_workflow_paused(job_id, user_id, workflow.id, &wait_id, &clarification)
        .await
        .expect("pause workflow");

    let revision: i64 = sqlx::query_scalar("SELECT workflow_revision FROM chat_jobs WHERE id = $1")
        .bind(job_id)
        .fetch_one(&db.pool)
        .await
        .expect("read workflow revision");

    // Simulate a process restart: a brand-new repository instance is built
    // from scratch rather than reusing `state`, proving resume's identity
    // check is driven entirely by durable row state, not in-memory state.
    let fresh_state = WorkflowStateRepository::new(db.pool.clone());
    let request = valid_request(
        job_id,
        user_id,
        &workflow,
        &wait_id,
        &clarification,
        revision,
    );
    let outcome = fresh_state.resume(request).await.expect("resume executes");
    assert_eq!(outcome, ResumeOutcome::Resumed);

    let runs = fresh_state
        .node_runs(job_id, workflow.id)
        .await
        .expect("load node runs");
    let probe_runs: Vec<_> = runs.iter().filter(|run| run.node_id == probe_id).collect();
    assert_eq!(
        probe_runs.len(),
        1,
        "restart + resume must not create a second probe row"
    );
    assert_eq!(probe_runs[0].status, NodeRunStatus::Completed);
    db.drop_database().await;
}

/// A `WorkflowNodeExecutor` that only ever handles the `start` node — the
/// runner resolves `CardinalityBranch` and `Complete` nodes internally, so a
/// call for either would mean the runner regressed to invoking the executor
/// for a node kind it must handle itself.
struct StartOnlyExecutor;
#[async_trait]
impl WorkflowNodeExecutor for StartOnlyExecutor {
    async fn execute(
        &self,
        node: &WorkflowNode,
        _bindings: &BTreeMap<String, Value>,
    ) -> anyhow::Result<NodeExecution> {
        assert_eq!(
            node.id.as_str(),
            "start",
            "only `start` reaches the executor"
        );
        Ok(NodeExecution::Completed {
            output: json!({}),
            rows_returned: 2,
        })
    }
}

fn catalog() -> KnowledgeCatalog {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    KnowledgeLoader::new(root.join("knowledge"), root.join("queries"))
        .load()
        .expect("load catalog")
}

#[tokio::test]
async fn cardinality_branch_lineage_is_reconstructable_from_checkpoints_and_events() {
    let db = spawn_db().await;
    let user_id = db.insert_user().await;
    let job_id = insert_job(&db.pool, user_id).await;
    let state = WorkflowStateRepository::new(db.pool.clone());

    let start_id = NodeId::new("start").unwrap();
    let branch_id = NodeId::new("branch").unwrap();
    let finish_id = NodeId::new("finish").unwrap();
    let start = WorkflowNode {
        id: start_id.clone(),
        kind: NodeKind::ExecuteQuery(ExecuteQueryNode {
            capability_id: None,
            dataset_id: None,
            shape_id: None,
            query_id: None,
            iterate_over: None,
        }),
        inputs: vec![],
        outputs: vec![],
        policy: policy(),
        budget: budget(),
        idempotency: Idempotency::Replayable,
        retry: RetryPolicy { max_attempts: 0 },
    };
    let branch = WorkflowNode {
        id: branch_id.clone(),
        kind: NodeKind::CardinalityBranch(CardinalityBranchNode {
            source: start_id.clone(),
            zero: finish_id.clone(),
            one: finish_id.clone(),
            many: finish_id.clone(),
        }),
        inputs: vec![],
        outputs: vec![],
        policy: policy(),
        budget: budget(),
        idempotency: Idempotency::Pure,
        retry: RetryPolicy { max_attempts: 0 },
    };
    let finish = complete_node("finish");
    let workflow = build_workflow(
        vec![start, branch, finish],
        vec![
            WorkflowEdge {
                from: start_id.clone(),
                to: branch_id.clone(),
                condition: EdgeCondition::Always,
            },
            WorkflowEdge {
                from: branch_id.clone(),
                to: finish_id.clone(),
                condition: EdgeCondition::Cardinality(Cardinality::Many),
            },
        ],
    );
    state
        .install_workflow(job_id, user_id, &workflow)
        .await
        .expect("install workflow");

    let principal = PrincipalContext {
        user_id,
        role: "admin".into(),
        capability_ids: vec![],
        office_ids: vec![1],
        can_view_pii: true,
        legacy_api_key_id: None,
    };
    let runner = WorkflowRunner::new(state, StartOnlyExecutor, std::sync::Arc::new(catalog()));
    let outcome = runner
        .run(job_id, user_id, &principal, &workflow)
        .await
        .expect("workflow runs to completion");
    assert_eq!(outcome, WorkflowRunOutcome::Completed);

    let events: Vec<(String, Option<String>, Value)> = sqlx::query_as(
        "SELECT event_type, step, payload_json FROM chat_job_events WHERE job_id = $1 ORDER BY created_at",
    )
    .bind(job_id)
    .fetch_all(&db.pool)
    .await
    .expect("load events");
    let event_types: Vec<&str> = events.iter().map(|(kind, _, _)| kind.as_str()).collect();
    assert_eq!(
        event_types,
        vec![
            "workflow_node_started",
            "workflow_node_completed",
            "workflow_node_started",
            "workflow_node_completed",
            "workflow_branch_decided",
            "workflow_node_started",
            "workflow_node_completed",
        ],
        "the event trail must reconstruct start -> branch -> finish in order"
    );
    let branch_decision = events
        .iter()
        .find(|(kind, _, _)| kind == "workflow_branch_decided")
        .expect("branch decision event recorded");
    assert_eq!(branch_decision.1.as_deref(), Some("branch"));
    assert_eq!(branch_decision.2["cardinality"], json!("many"));
    assert_eq!(branch_decision.2["node_id"], json!("branch"));

    let checkpoints: Vec<(String, String)> = sqlx::query_as(
        "SELECT step, checkpoint_type FROM chat_job_checkpoints WHERE job_id = $1 ORDER BY created_at",
    )
    .bind(job_id)
    .fetch_all(&db.pool)
    .await
    .expect("load checkpoints");
    assert_eq!(
        checkpoints,
        vec![
            ("start".to_string(), "node_started".to_string()),
            ("start".to_string(), "node_completed".to_string()),
            ("branch".to_string(), "node_started".to_string()),
            ("branch".to_string(), "node_completed".to_string()),
            ("finish".to_string(), "node_started".to_string()),
            ("finish".to_string(), "node_completed".to_string()),
        ],
        "checkpoints alone must reconstruct which nodes actually ran, in order"
    );
    db.drop_database().await;
}
