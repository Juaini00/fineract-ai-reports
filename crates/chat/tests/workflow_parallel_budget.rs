//! Coverage for Phase 5 of the workflow runtime (docs/issues/active/012):
//! bounded parallel fan-out, FailFast cancellation of in-flight siblings, and
//! the live budget ledger. Uses the same lighter direct-migration DB harness
//! as `workflow_resume.rs` — no HTTP server, no `ChatAppState`.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use app_core::auth::model::PrincipalContext;
use async_trait::async_trait;
use chat::assistant::workflow::{
    ExecuteQueryNode, ExecutionWorkflow, FailPolicy, Idempotency, NodeBudget, NodeExecution,
    NodeId, NodeKind, NodePolicy, NodeRunStatus, OfficeScope, OutputContract, OutputMode,
    RetryPolicy, WORKFLOW_CONTRACT_VERSION, WorkflowBudgets, WorkflowEdge, WorkflowNode,
    WorkflowNodeExecutor, WorkflowRunOutcome, WorkflowRunner, WorkflowStateRepository,
};
use chat::knowledge::catalog::loader::KnowledgeLoader;
use chat::knowledge::model::{KnowledgeCatalog, Sensitivity};
use serde_json::{Value, json};
use sqlx::{AssertSqlSafe, PgPool, postgres::PgPoolOptions};
use uuid::Uuid;

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

fn policy() -> NodePolicy {
    NodePolicy {
        required_capability: None,
        office_scope: OfficeScope::AuthorizedIntersection,
        max_sensitivity: Sensitivity::Pii,
        pii_required: false,
    }
}
fn budget(query_cost: u8) -> NodeBudget {
    NodeBudget {
        timeout_ms: 10,
        row_cap: 10,
        query_cost,
    }
}
/// A bare `ExecuteQuery` node with no catalog/dataset reference — the runner
/// hands it straight to the test's `WorkflowNodeExecutor`, never touching the
/// catalog, so it needs no approved capability to exercise the runner alone.
fn query_node(id: &str, query_cost: u8) -> WorkflowNode {
    WorkflowNode {
        id: NodeId::new(id).unwrap(),
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
        budget: budget(query_cost),
        idempotency: Idempotency::Replayable,
        retry: RetryPolicy { max_attempts: 0 },
    }
}
fn complete_node(id: &str) -> WorkflowNode {
    WorkflowNode {
        id: NodeId::new(id).unwrap(),
        kind: NodeKind::Complete(chat::assistant::workflow::CompleteNode {
            terminal: chat::assistant::workflow::TerminalState::Success,
        }),
        inputs: vec![],
        outputs: vec![],
        policy: policy(),
        budget: budget(0),
        idempotency: Idempotency::Pure,
        retry: RetryPolicy { max_attempts: 0 },
    }
}
fn edge(from: &str, to: &str) -> WorkflowEdge {
    WorkflowEdge {
        from: NodeId::new(from).unwrap(),
        to: NodeId::new(to).unwrap(),
        condition: chat::assistant::workflow::EdgeCondition::Always,
    }
}
fn workflow_budgets(max_parallel_queries: u8, max_query_count: u8) -> WorkflowBudgets {
    WorkflowBudgets {
        shared_timeout_ms: 30_000,
        shared_row_cap: 1_000,
        max_query_count,
        max_parallel_queries,
        max_model_turns: 2,
        max_node_retries: 0,
    }
}
fn build_workflow(
    nodes: Vec<WorkflowNode>,
    edges: Vec<WorkflowEdge>,
    budgets: WorkflowBudgets,
) -> ExecutionWorkflow {
    ExecutionWorkflow {
        id: Uuid::new_v4(),
        contract_version: WORKFLOW_CONTRACT_VERSION,
        catalog_version: Uuid::nil(),
        nodes,
        edges,
        budgets,
        fail_policy: FailPolicy::FailFast,
        output_contract: OutputContract {
            mode: OutputMode::Table,
            allows_partial: false,
            max_sensitivity: Sensitivity::Pii,
        },
    }
}
fn admin_principal(user_id: Uuid) -> PrincipalContext {
    PrincipalContext {
        user_id,
        role: "admin".into(),
        capability_ids: vec![],
        office_ids: vec![1],
        can_view_pii: true,
        legacy_api_key_id: None,
    }
}
fn catalog() -> KnowledgeCatalog {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    KnowledgeLoader::new(root.join("knowledge"), root.join("queries"))
        .load()
        .expect("load catalog")
}

/// Records the `Instant` each node's `execute` call started, then sleeps a
/// fixed delay before completing — used to prove concurrent nodes overlap.
#[derive(Clone)]
struct DelayedExecutor {
    delay: Duration,
    starts: Arc<Mutex<Vec<(String, Instant)>>>,
}
#[async_trait]
impl WorkflowNodeExecutor for DelayedExecutor {
    async fn execute(
        &self,
        node: &WorkflowNode,
        _bindings: &BTreeMap<String, Value>,
    ) -> anyhow::Result<NodeExecution> {
        self.starts
            .lock()
            .unwrap()
            .push((node.id.as_str().to_owned(), Instant::now()));
        tokio::time::sleep(self.delay).await;
        Ok(NodeExecution::Completed {
            output: json!({}),
            rows_returned: 0,
        })
    }
}

#[tokio::test]
async fn independent_runnable_nodes_execute_concurrently() {
    let db = spawn_db().await;
    let user_id = db.insert_user().await;
    let job_id = insert_job(&db.pool, user_id).await;
    let state = WorkflowStateRepository::new(db.pool.clone());

    let workflow = build_workflow(
        vec![
            query_node("probe_a", 0),
            query_node("probe_b", 0),
            complete_node("finish"),
        ],
        vec![edge("probe_a", "finish"), edge("probe_b", "finish")],
        workflow_budgets(2, 10),
    );
    state
        .install_workflow(job_id, user_id, &workflow)
        .await
        .expect("install workflow");

    let delay = Duration::from_millis(200);
    let executor = DelayedExecutor {
        delay,
        starts: Arc::new(Mutex::new(Vec::new())),
    };
    let starts = executor.starts.clone();
    let principal = admin_principal(user_id);
    let runner = WorkflowRunner::new(state, executor, Arc::new(catalog()));

    let started = Instant::now();
    let outcome = runner
        .run(job_id, user_id, &principal, &workflow)
        .await
        .expect("workflow runs to completion");
    let elapsed = started.elapsed();

    assert_eq!(outcome, WorkflowRunOutcome::Completed);
    // Sequential execution of two 200ms nodes would take >= 400ms; concurrent
    // execution takes ~200ms plus overhead. 350ms leaves ample margin.
    assert!(
        elapsed < Duration::from_millis(350),
        "two independent nodes should run concurrently, took {elapsed:?}"
    );
    let gap = {
        let recorded = starts.lock().unwrap();
        assert_eq!(recorded.len(), 2, "both probes must have executed");
        recorded[0].1.max(recorded[1].1) - recorded[0].1.min(recorded[1].1)
    };
    assert!(
        gap < Duration::from_millis(100),
        "both probes should start within the same batch, gap was {gap:?}"
    );
    db.drop_database().await;
}

/// Fails one node immediately and would (if not cancelled) let its siblings
/// each sleep far longer than the whole test should take.
struct FailFastExecutor {
    fail_id: String,
    slow_delay: Duration,
}
#[async_trait]
impl WorkflowNodeExecutor for FailFastExecutor {
    async fn execute(
        &self,
        node: &WorkflowNode,
        _bindings: &BTreeMap<String, Value>,
    ) -> anyhow::Result<NodeExecution> {
        if node.id.as_str() == self.fail_id {
            return Ok(NodeExecution::Failed);
        }
        tokio::time::sleep(self.slow_delay).await;
        Ok(NodeExecution::Completed {
            output: json!({}),
            rows_returned: 0,
        })
    }
}

#[tokio::test]
async fn fail_fast_cancels_in_flight_siblings_and_skips_their_completion() {
    let db = spawn_db().await;
    let user_id = db.insert_user().await;
    let job_id = insert_job(&db.pool, user_id).await;
    let state = WorkflowStateRepository::new(db.pool.clone());
    let inspect_state = WorkflowStateRepository::new(db.pool.clone());

    let workflow = build_workflow(
        vec![
            query_node("fail_fast", 0),
            query_node("slow_1", 0),
            query_node("slow_2", 0),
            complete_node("finish"),
        ],
        vec![
            edge("fail_fast", "finish"),
            edge("slow_1", "finish"),
            edge("slow_2", "finish"),
        ],
        workflow_budgets(3, 10),
    );
    state
        .install_workflow(job_id, user_id, &workflow)
        .await
        .expect("install workflow");

    let executor = FailFastExecutor {
        fail_id: "fail_fast".into(),
        slow_delay: Duration::from_secs(5),
    };
    let principal = admin_principal(user_id);
    let runner = WorkflowRunner::new(state, executor, Arc::new(catalog()));

    let started = Instant::now();
    let outcome = runner
        .run(job_id, user_id, &principal, &workflow)
        .await
        .expect("run executes");
    let elapsed = started.elapsed();

    assert_eq!(outcome, WorkflowRunOutcome::Failed);
    assert!(
        elapsed < Duration::from_secs(1),
        "FailFast must not wait out the 5s slow siblings, took {elapsed:?}"
    );

    let runs = inspect_state
        .node_runs(job_id, workflow.id)
        .await
        .expect("load node runs");
    for slow_id in ["slow_1", "slow_2"] {
        let run = runs
            .iter()
            .find(|run| run.node_id.as_str() == slow_id)
            .unwrap_or_else(|| panic!("{slow_id} has a run row"));
        assert_ne!(
            run.status,
            NodeRunStatus::Completed,
            "{slow_id} must not have been completed after cancellation"
        );
    }
    db.drop_database().await;
}

/// Executor that records which node ids it was actually invoked for — proves
/// a budget-exceeding node is never handed to the executor at all.
struct LoggingExecutor {
    invoked: Arc<Mutex<Vec<String>>>,
}
#[async_trait]
impl WorkflowNodeExecutor for LoggingExecutor {
    async fn execute(
        &self,
        node: &WorkflowNode,
        _bindings: &BTreeMap<String, Value>,
    ) -> anyhow::Result<NodeExecution> {
        self.invoked
            .lock()
            .unwrap()
            .push(node.id.as_str().to_owned());
        Ok(NodeExecution::Completed {
            output: json!({}),
            rows_returned: 0,
        })
    }
}

#[tokio::test]
async fn budget_exhaustion_stops_the_run_before_the_over_budget_node_executes() {
    let db = spawn_db().await;
    let user_id = db.insert_user().await;
    let job_id = insert_job(&db.pool, user_id).await;
    let state = WorkflowStateRepository::new(db.pool.clone());

    // `first` and `second` each cost 1 query; the shared budget only allows 1
    // total, so `second` must never run once `first` has consumed it.
    let workflow = build_workflow(
        vec![
            query_node("first", 1),
            query_node("second", 1),
            complete_node("finish"),
        ],
        vec![edge("first", "second"), edge("second", "finish")],
        workflow_budgets(2, 1),
    );
    state
        .install_workflow(job_id, user_id, &workflow)
        .await
        .expect("install workflow");

    let invoked = Arc::new(Mutex::new(Vec::new()));
    let executor = LoggingExecutor {
        invoked: invoked.clone(),
    };
    let principal = admin_principal(user_id);
    let runner = WorkflowRunner::new(state, executor, Arc::new(catalog()));

    let outcome = runner
        .run(job_id, user_id, &principal, &workflow)
        .await
        .expect("run executes");

    assert_eq!(outcome, WorkflowRunOutcome::Failed);
    assert_eq!(
        *invoked.lock().unwrap(),
        vec!["first".to_string()],
        "the over-budget `second` node must never reach the executor"
    );
    db.drop_database().await;
}
