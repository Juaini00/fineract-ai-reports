//! HTTP coverage for the durable, structured clarification contract.

mod common;

use common::{TestApp, spawn_app};
use serde_json::{Value, json};
use std::time::{Duration, Instant};
use uuid::Uuid;

const POLL_TIMEOUT: Duration = Duration::from_secs(15);
const POLL_INTERVAL: Duration = Duration::from_millis(200);
const OPTION_ID: &str = "savings_deposit_top_n";

#[tokio::test(flavor = "multi_thread")]
async fn structured_submission_is_durable_and_consumes_date_and_limit_once() {
    let app = spawn_app().await;
    let token = app.login_admin().await;
    let seeded = seed_waiting_clarification(&app, &token, "one submit").await;

    let recovered = get_job(&app, &token, &seeded.job_id).await;
    assert_public_clarification(&recovered, &seeded.payload);
    assert_typed_fields(&recovered);

    let response = post_structured(
        &app,
        &token,
        &seeded,
        json!({
            "option_id": OPTION_ID,
            "answers": {
                "date_range": { "from": "2024-01-01", "to": "2024-01-31" },
                "limit": 5
            }
        }),
    )
    .await;
    assert_eq!(response.status(), 201, "{}", response.text().await.unwrap());

    let after = wait_until_not_running(&app, &token, &seeded.job_id.to_string()).await;
    if after["status"] == "waiting_for_user_input" {
        let clarification = &after["result_json"]["structured_response"]["clarification"];
        let keys: Vec<_> = clarification["fields"]
            .as_array()
            .into_iter()
            .flatten()
            .chain(
                clarification["options"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .flat_map(|option| option["fields"].as_array().into_iter().flatten()),
            )
            .filter_map(|field| field["key"].as_str())
            .collect();
        assert!(
            !(keys.contains(&"date_range") || keys.contains(&"limit")),
            "the submitted parameters must not be requested again: {after}"
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn clarification_recovery_and_historical_metadata_do_not_override_active_revision() {
    let app = spawn_app().await;
    let token = app.login_admin().await;
    let seeded = seed_waiting_clarification(&app, &token, "recovery").await;
    let historical = clarification_payload(Uuid::new_v4(), 0);

    sqlx::query(
        "UPDATE chat_messages SET metadata_json = jsonb_build_object('type', 'assistant_response', 'assistant_response', jsonb_build_object('clarification', $1::jsonb)) WHERE job_id = $2 AND role = 'assistant'",
    )
    .bind(clarification_view(&historical))
    .bind(seeded.job_id)
    .execute(&app.app_pool)
    .await
    .unwrap();

    // This is a fresh HTTP read with Redis disabled by the harness; it must be
    // reconstructed from Postgres, not from an SSE replay.
    let recovered = get_job(&app, &token, &seeded.job_id).await;
    assert_public_clarification(&recovered, &seeded.payload);
    let messages = app
        .get_bearer(
            &format!("/chat/sessions/{}/messages", seeded.session_id),
            &token,
        )
        .await;
    assert_eq!(messages.status(), 200);
    let messages: Value = messages.json().await.unwrap();
    assert!(messages["data"].as_array().unwrap().iter().any(|message| {
        message["metadata_json"]["assistant_response"]["clarification"]["id"]
            == historical["id"].as_str().unwrap()
    }));

    let stale = app
        .post_json_bearer(
            &format!("/chat/jobs/{}/responses", seeded.job_id),
            &token,
            &json!({
                "clarification_id": historical["id"],
                "clarification_revision": historical["revision"],
                "option_id": OPTION_ID,
                "answers": valid_answers(),
            }),
        )
        .await;
    assert_eq!(stale.status(), 409);
    assert_eq!(
        stale.json::<Value>().await.unwrap()["error"]["code"],
        "clarification_stale"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn invalid_stale_and_duplicate_submissions_do_not_create_extra_messages() {
    let app = spawn_app().await;
    let token = app.login_admin().await;
    let seeded = seed_waiting_clarification(&app, &token, "validation").await;
    let before = clarification_message_count(&app, seeded.job_id).await;

    let invalid = post_structured(
        &app,
        &token,
        &seeded,
        json!({
            "option_id": "not-offered",
            "answers": valid_answers(),
        }),
    )
    .await;
    assert_eq!(invalid.status(), 400);
    let stale = post_structured(
        &app,
        &token,
        &seeded,
        json!({
            "clarification_revision": seeded.payload["revision"].as_u64().unwrap() + 1,
            "option_id": OPTION_ID,
            "answers": valid_answers(),
        }),
    )
    .await;
    assert_eq!(stale.status(), 409);
    assert_eq!(
        clarification_message_count(&app, seeded.job_id).await,
        before
    );
    assert_eq!(
        get_job(&app, &token, &seeded.job_id).await["status"],
        "waiting_for_user_input"
    );

    let path = format!("{}/chat/jobs/{}/responses", app.base_url, seeded.job_id);
    let body = json!({
        "clarification_id": seeded.payload["id"],
        "clarification_revision": seeded.payload["revision"],
        "option_id": OPTION_ID,
        "answers": valid_answers(),
    });
    let (first, second) = tokio::join!(
        app.http.post(&path).bearer_auth(&token).json(&body).send(),
        app.http.post(&path).bearer_auth(&token).json(&body).send()
    );
    let statuses = [
        first.unwrap().status().as_u16(),
        second.unwrap().status().as_u16(),
    ];
    assert!(
        statuses.contains(&201),
        "expected one accepted response: {statuses:?}"
    );
    assert!(
        statuses.contains(&409),
        "expected stale/not-active duplicate: {statuses:?}"
    );
    assert_eq!(
        clarification_message_count(&app, seeded.job_id).await,
        before + 1
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn archived_and_foreign_clarification_resources_are_hidden() {
    let app = spawn_app().await;
    let owner = app.login_admin().await;
    let seeded = seed_waiting_clarification(&app, &owner, "archive me").await;
    sqlx::query("UPDATE chat_sessions SET status = 'archived', archived_at = now() WHERE id = $1")
        .bind(seeded.session_id)
        .execute(&app.app_pool)
        .await
        .unwrap();

    for path in [
        format!("/chat/sessions/{}", seeded.session_id),
        format!("/chat/sessions/{}/messages", seeded.session_id),
    ] {
        let response = app.get_bearer(&path, &owner).await;
        assert_eq!(response.status(), 404);
        assert!(!response.text().await.unwrap().contains("archive me"));
    }
    let create = app
        .post_json_bearer(
            "/chat/jobs",
            &owner,
            &json!({ "session_id": seeded.session_id, "message": "again" }),
        )
        .await;
    assert_eq!(create.status(), 404);

    let other = app.create_test_user_bearer().await;
    let foreign = app
        .get_bearer(&format!("/chat/jobs/{}", seeded.job_id), &other)
        .await;
    assert_eq!(foreign.status(), 404);
    assert!(
        !foreign
            .text()
            .await
            .unwrap()
            .contains(&seeded.job_id.to_string())
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn active_clarifications_are_job_scoped_and_legacy_responses_remain_compatible() {
    let app = spawn_app().await;
    let token = app.login_admin().await;
    let first = seed_waiting_clarification(&app, &token, "isolated first").await;
    let second = seed_waiting_clarification_in_session(&app, &token, first.session_id).await;
    assert_ne!(first.payload["id"], second.payload["id"]);
    assert_public_clarification(&get_job(&app, &token, &first.job_id).await, &first.payload);
    assert_public_clarification(
        &get_job(&app, &token, &second.job_id).await,
        &second.payload,
    );

    // Legacy message-only, option-plus-authoritative-message, meaningful and
    // boilerplate Others, and a paraphrase all remain accepted continuations.
    for (seeded, body) in [
        (
            seed_waiting_clarification(&app, &token, "legacy date").await,
            json!({ "message": "from 2024-01-01 to 2024-01-31" }),
        ),
        (
            seed_waiting_clarification(&app, &token, "legacy option").await,
            json!({ "option_id": OPTION_ID, "message": "Please rank deposit transactions" }),
        ),
        (
            seed_waiting_clarification(&app, &token, "meaningful others").await,
            json!({ "option_id": "others", "message": "show withdrawals instead" }),
        ),
        (
            seed_waiting_clarification(&app, &token, "boilerplate others").await,
            json!({ "option_id": "others", "message": "others" }),
        ),
        (
            seed_waiting_clarification(&app, &token, "paraphrase").await,
            json!({ "message": "largest savings deposits, please" }),
        ),
    ] {
        let response = app
            .post_json_bearer(
                &format!("/chat/jobs/{}/responses", seeded.job_id),
                &token,
                &body,
            )
            .await;
        assert_eq!(response.status(), 201, "legacy response rejected: {body}");
        let metadata: Value = sqlx::query_scalar(
            "SELECT metadata_json FROM chat_messages WHERE job_id = $1 AND role = 'clarification' ORDER BY created_at DESC LIMIT 1",
        )
        .bind(seeded.job_id)
        .fetch_one(&app.app_pool)
        .await
        .unwrap();
        assert_eq!(metadata["source_message"], body["message"]);
        if let Some(option) = body["option_id"].as_str() {
            assert_eq!(metadata["selected_option_id"], option);
        }
    }
}

struct SeededClarification {
    session_id: Uuid,
    job_id: Uuid,
    payload: Value,
}

async fn seed_waiting_clarification(
    app: &TestApp,
    token: &str,
    title: &str,
) -> SeededClarification {
    let session = app
        .post_json_bearer("/chat/sessions", token, &json!({ "title": title }))
        .await;
    assert_eq!(session.status(), 201);
    let session: Value = session.json().await.unwrap();
    seed_waiting_clarification_in_session(
        app,
        token,
        Uuid::parse_str(session["data"]["id"].as_str().unwrap()).unwrap(),
    )
    .await
}

async fn seed_waiting_clarification_in_session(
    app: &TestApp,
    token: &str,
    session_id: Uuid,
) -> SeededClarification {
    let create = app
        .post_json_bearer(
            "/chat/jobs",
            token,
            &json!({ "session_id": session_id, "message": "hello" }),
        )
        .await;
    assert_eq!(create.status(), 201);
    let create: Value = create.json().await.unwrap();
    let job_id = Uuid::parse_str(create["data"]["job_id"].as_str().unwrap()).unwrap();
    let _ = wait_until_not_running(app, token, &job_id.to_string()).await;
    let payload = clarification_payload(Uuid::new_v4(), 7);
    let view = clarification_view(&payload);
    sqlx::query(
        "UPDATE chat_jobs SET status = 'waiting_for_user_input', current_step = 'complete_or_wait', result_json = jsonb_build_object('structured_response', jsonb_build_object('response_type', 'clarification', 'clarification', $1::jsonb)) WHERE id = $2",
    )
    .bind(&view)
    .bind(job_id)
    .execute(&app.app_pool)
    .await
    .unwrap();
    sqlx::query(
        "UPDATE assistant_job_memory SET pending_clarification_json = $1::jsonb WHERE job_id = $2",
    )
    .bind(&payload)
    .bind(job_id)
    .execute(&app.app_pool)
    .await
    .unwrap();
    SeededClarification {
        session_id,
        job_id,
        payload,
    }
}

fn clarification_payload(id: Uuid, revision: u32) -> Value {
    json!({
        "version": 1, "id": id, "revision": revision, "kind": "select_option",
        "question": "Which deposit report?", "attempt": 0, "allow_free_text": true,
        "is_missing_execution_parameters": true,
        "fields": [{ "key": "date_range", "label": "Date range", "field_type": "date_range", "required": true, "validation": { "max_range_days": 31 } }],
        "options": [
            { "id": OPTION_ID, "label": "Top deposits", "description": null,
              "fields": [{ "key": "limit", "label": "Limit", "field_type": "integer", "required": true, "validation": { "min_integer": 1, "max_integer": 10 } }] },
            { "id": "others", "label": "Other", "description": null, "fields": [] }
        ]
    })
}

fn clarification_view(payload: &Value) -> Value {
    let mut view = payload.clone();
    view.as_object_mut().unwrap().remove("attempt");
    view.as_object_mut()
        .unwrap()
        .remove("is_missing_execution_parameters");
    view
}

fn valid_answers() -> Value {
    json!({ "date_range": { "from": "2024-01-01", "to": "2024-01-31" }, "limit": 5 })
}

async fn post_structured(
    app: &TestApp,
    token: &str,
    seeded: &SeededClarification,
    body: Value,
) -> reqwest::Response {
    let mut body = body;
    if body.get("clarification_id").is_none() {
        body["clarification_id"] = seeded.payload["id"].clone();
    }
    if body.get("clarification_revision").is_none() {
        body["clarification_revision"] = seeded.payload["revision"].clone();
    }
    app.post_json_bearer(
        &format!("/chat/jobs/{}/responses", seeded.job_id),
        token,
        &body,
    )
    .await
}

async fn get_job(app: &TestApp, token: &str, job_id: &Uuid) -> Value {
    let response = app.get_bearer(&format!("/chat/jobs/{job_id}"), token).await;
    assert_eq!(response.status(), 200);
    response.json::<Value>().await.unwrap()["data"].clone()
}

async fn wait_until_not_running(app: &TestApp, token: &str, job_id: &str) -> Value {
    let deadline = Instant::now() + POLL_TIMEOUT;
    loop {
        let job = get_job(app, token, &Uuid::parse_str(job_id).unwrap()).await;
        if !matches!(job["status"].as_str(), Some("queued" | "running")) {
            return job;
        }
        assert!(Instant::now() < deadline, "job did not settle: {job}");
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

async fn clarification_message_count(app: &TestApp, job_id: Uuid) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM chat_messages WHERE job_id = $1 AND role = 'clarification'",
    )
    .bind(job_id)
    .fetch_one(&app.app_pool)
    .await
    .unwrap()
}

fn assert_public_clarification(job: &Value, payload: &Value) {
    let clarification = &job["result_json"]["structured_response"]["clarification"];
    assert_eq!(clarification, &clarification_view(payload), "{job}");
}

fn assert_typed_fields(job: &Value) {
    let clarification = &job["result_json"]["structured_response"]["clarification"];
    assert_eq!(clarification["kind"], "select_option");
    assert_eq!(clarification["fields"][0]["field_type"], "date_range");
    assert_eq!(
        clarification["options"][0]["fields"][0]["field_type"],
        "integer"
    );
}
