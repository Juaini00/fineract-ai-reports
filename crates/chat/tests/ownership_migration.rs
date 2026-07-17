use sqlx::{AssertSqlSafe, PgPool, postgres::PgPoolOptions};
use uuid::Uuid;
#[tokio::test]
async fn migration_backfills_only_provable_owners_and_accepts_bearer_rows() {
    let admin_url = std::env::var("TEST_ADMIN_DATABASE_URL")
        .unwrap_or_else(|_| "postgres://root:password@127.0.0.1:5432/postgres".into());
    let db_name = format!("ai_report_test_{}", Uuid::new_v4().simple());
    let admin = PgPoolOptions::new()
        .max_connections(1)
        .connect(&admin_url)
        .await
        .expect("connect test Postgres");
    sqlx::query(AssertSqlSafe(format!("CREATE DATABASE \"{db_name}\"")))
        .execute(&admin)
        .await
        .expect("create test database");
    let pool = PgPool::connect(&admin_url.replace("/postgres", &format!("/{db_name}")))
        .await
        .expect("connect test database");

    sqlx::raw_sql(r#"
        CREATE TABLE users (id UUID PRIMARY KEY);
        CREATE TABLE api_keys (id UUID PRIMARY KEY, user_id UUID REFERENCES users(id));
        CREATE TABLE chat_sessions (id UUID PRIMARY KEY, api_key_id UUID NOT NULL REFERENCES api_keys(id));
        CREATE TABLE chat_jobs (id UUID PRIMARY KEY, session_id UUID NOT NULL REFERENCES chat_sessions(id), api_key_id UUID NOT NULL REFERENCES api_keys(id));
        CREATE TABLE assistant_llm_traces (id UUID PRIMARY KEY, job_id UUID REFERENCES chat_jobs(id), session_id UUID REFERENCES chat_sessions(id), api_key_id UUID NOT NULL REFERENCES api_keys(id));
        CREATE TABLE chat_job_audit_events (id UUID PRIMARY KEY, job_id UUID NOT NULL REFERENCES chat_jobs(id), session_id UUID REFERENCES chat_sessions(id), api_key_id UUID REFERENCES api_keys(id));
        INSERT INTO users VALUES ('10000000-0000-0000-0000-000000000001'), ('10000000-0000-0000-0000-000000000002');
        INSERT INTO api_keys VALUES
          ('20000000-0000-0000-0000-000000000001', '10000000-0000-0000-0000-000000000001'),
          ('20000000-0000-0000-0000-000000000002', '10000000-0000-0000-0000-000000000002'),
          ('20000000-0000-0000-0000-000000000003', NULL);
        INSERT INTO chat_sessions VALUES
          ('30000000-0000-0000-0000-000000000001', '20000000-0000-0000-0000-000000000001'),
          ('30000000-0000-0000-0000-000000000003', '20000000-0000-0000-0000-000000000003');
        INSERT INTO chat_jobs VALUES
          ('40000000-0000-0000-0000-000000000001', '30000000-0000-0000-0000-000000000001', '20000000-0000-0000-0000-000000000002'),
          ('40000000-0000-0000-0000-000000000002', '30000000-0000-0000-0000-000000000001', '20000000-0000-0000-0000-000000000003');
        INSERT INTO assistant_llm_traces VALUES
          ('50000000-0000-0000-0000-000000000001', '40000000-0000-0000-0000-000000000001', NULL, '20000000-0000-0000-0000-000000000002'),
          ('50000000-0000-0000-0000-000000000002', '40000000-0000-0000-0000-000000000002', '30000000-0000-0000-0000-000000000001', '20000000-0000-0000-0000-000000000003');
        INSERT INTO chat_job_audit_events VALUES
          ('60000000-0000-0000-0000-000000000001', '40000000-0000-0000-0000-000000000001', NULL, '20000000-0000-0000-0000-000000000002'),
          ('60000000-0000-0000-0000-000000000002', '40000000-0000-0000-0000-000000000002', '30000000-0000-0000-0000-000000000001', '20000000-0000-0000-0000-000000000003');
    "#).execute(&pool).await.expect("install pre-migration schema");

    sqlx::raw_sql(include_str!(
        "../../../migrations/20260715120000_add_user_chat_ownership.sql"
    ))
    .execute(&pool)
    .await
    .expect("apply ownership migration");
    let session_owner: Option<Uuid> = sqlx::query_scalar(
        "SELECT user_id FROM chat_sessions WHERE id = '30000000-0000-0000-0000-000000000001'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let job_owner: Option<Uuid> = sqlx::query_scalar(
        "SELECT user_id FROM chat_jobs WHERE id = '40000000-0000-0000-0000-000000000001'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let trace_owner: Option<Uuid> = sqlx::query_scalar("SELECT user_id FROM assistant_llm_traces WHERE id = '50000000-0000-0000-0000-000000000001'")
        .fetch_one(&pool).await.unwrap();
    let audit_owner: Option<Uuid> = sqlx::query_scalar("SELECT user_id FROM chat_job_audit_events WHERE id = '60000000-0000-0000-0000-000000000001'")
        .fetch_one(&pool).await.unwrap();
    let legacy_owner: Option<Uuid> = sqlx::query_scalar(
        "SELECT user_id FROM chat_sessions WHERE id = '30000000-0000-0000-0000-000000000003'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let relationship_owners: (Option<Uuid>, Option<Uuid>, Option<Uuid>) = sqlx::query_as(
        "SELECT j.user_id, t.user_id, a.user_id FROM chat_jobs j JOIN assistant_llm_traces t ON t.job_id = j.id JOIN chat_job_audit_events a ON a.job_id = j.id WHERE j.id = '40000000-0000-0000-0000-000000000002'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let expected = Some(Uuid::parse_str("10000000-0000-0000-0000-000000000001").unwrap());
    assert_eq!((session_owner, job_owner), (expected, None));
    assert_eq!((trace_owner, audit_owner), (None, None));
    assert_eq!(relationship_owners, (expected, expected, expected));
    assert_eq!(legacy_owner, None, "must not substitute an API-key UUID");

    let users = [
        Uuid::new_v4(),
        Uuid::new_v4(),
        Uuid::new_v4(),
        Uuid::new_v4(),
    ];
    for user in users {
        sqlx::query("INSERT INTO users (id) VALUES ($1)")
            .bind(user)
            .execute(&pool)
            .await
            .unwrap();
    }
    let bearer_session = Uuid::new_v4();
    let bearer_job = Uuid::new_v4();
    sqlx::query("INSERT INTO chat_sessions (id, user_id, api_key_id) VALUES ($1, $2, NULL)")
        .bind(bearer_session)
        .bind(users[0])
        .execute(&pool)
        .await
        .expect("bearer session");
    sqlx::query(
        "INSERT INTO chat_jobs (id, session_id, user_id, api_key_id) VALUES ($1, $2, $3, NULL)",
    )
    .bind(bearer_job)
    .bind(bearer_session)
    .bind(users[1])
    .execute(&pool)
    .await
    .expect("bearer job");
    sqlx::query("INSERT INTO assistant_llm_traces (id, job_id, user_id, api_key_id) VALUES ($1, $2, $3, NULL)")
        .bind(Uuid::new_v4()).bind(bearer_job).bind(users[2]).execute(&pool).await.expect("bearer trace");
    sqlx::query("INSERT INTO chat_job_audit_events (id, job_id) VALUES ($1, $2)")
        .bind(Uuid::new_v4())
        .bind(bearer_job)
        .execute(&pool)
        .await
        .expect("unattributed audit event");
    sqlx::query("INSERT INTO chat_job_audit_events (id, job_id, user_id) VALUES ($1, $2, $3)")
        .bind(Uuid::new_v4())
        .bind(bearer_job)
        .bind(users[3])
        .execute(&pool)
        .await
        .expect("bearer-attributed audit event");
    for table in ["chat_sessions", "chat_jobs", "assistant_llm_traces"] {
        let sql = match table {
            "chat_sessions" => "INSERT INTO chat_sessions (id) VALUES ($1)",
            "chat_jobs" => {
                "INSERT INTO chat_jobs (id, session_id) VALUES ($1, '30000000-0000-0000-0000-000000000001')"
            }
            _ => "INSERT INTO assistant_llm_traces (id) VALUES ($1)",
        };
        assert!(
            sqlx::query(sql)
                .bind(Uuid::new_v4())
                .execute(&pool)
                .await
                .is_err(),
            "{table} owner check"
        );
    }
    for sql in [
        "INSERT INTO chat_sessions (id, user_id) VALUES ($1, $2)",
        "INSERT INTO chat_jobs (id, session_id, user_id) VALUES ($1, '30000000-0000-0000-0000-000000000001', $2)",
        "INSERT INTO assistant_llm_traces (id, user_id) VALUES ($1, $2)",
        "INSERT INTO chat_job_audit_events (id, job_id, user_id) VALUES ($1, '40000000-0000-0000-0000-000000000001', $2)",
    ] {
        assert!(
            sqlx::query(sql)
                .bind(Uuid::new_v4())
                .bind(Uuid::new_v4())
                .execute(&pool)
                .await
                .is_err(),
            "user FK: {sql}"
        );
    }
    for user in users {
        assert!(
            sqlx::query("DELETE FROM users WHERE id = $1")
                .bind(user)
                .execute(&pool)
                .await
                .is_err(),
            "user ownership must restrict deletion"
        );
    }

    pool.close().await;
    sqlx::query(AssertSqlSafe(format!(
        "DROP DATABASE \"{db_name}\" WITH (FORCE)"
    )))
    .execute(&admin)
    .await
    .expect("drop test database");

    let migrator_db_name = format!("ai_report_test_{}", Uuid::new_v4().simple());
    sqlx::query(AssertSqlSafe(format!(
        "CREATE DATABASE \"{migrator_db_name}\""
    )))
    .execute(&admin)
    .await
    .expect("create migrator test database");
    let migrator_pool =
        PgPool::connect(&admin_url.replace("/postgres", &format!("/{migrator_db_name}")))
            .await
            .expect("connect migrator test database");
    sqlx::migrate!("../../migrations")
        .run(&migrator_pool)
        .await
        .expect("run real migrator");
    let indexes: Vec<(String, bool, bool, Vec<String>)> = sqlx::query_as(
        "SELECT c.relname, i.indisvalid, i.indisready,
                ARRAY(SELECT a.attname FROM unnest(i.indkey) WITH ORDINALITY AS k(attnum, ord)
                      JOIN pg_attribute a ON a.attrelid = i.indrelid AND a.attnum = k.attnum
                      ORDER BY k.ord)
         FROM pg_index i JOIN pg_class c ON c.oid = i.indexrelid
         WHERE c.relname = ANY($1)",
    )
    .bind(
        &[
            "idx_chat_sessions_user_id",
            "idx_chat_jobs_user_id",
            "idx_assistant_llm_traces_user_id",
            "idx_chat_job_audit_events_user_id",
        ][..],
    )
    .fetch_all(&migrator_pool)
    .await
    .expect("inspect ownership indexes");
    assert_eq!(indexes.len(), 4, "all ownership indexes must exist");
    assert!(indexes.iter().all(|(_, valid, ready, _)| *valid && *ready));
    let session_index = indexes
        .iter()
        .find(|(name, _, _, _)| name == "idx_chat_sessions_user_id")
        .unwrap();
    assert_eq!(session_index.3, ["user_id", "updated_at"]);
    migrator_pool.close().await;
    sqlx::query(AssertSqlSafe(format!(
        "DROP DATABASE \"{migrator_db_name}\" WITH (FORCE)"
    )))
    .execute(&admin)
    .await
    .expect("drop migrator test database");
}
