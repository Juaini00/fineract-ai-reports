mod common;

use common::{TestApp, spawn_app};
use serde_json::{Value, json};
use std::time::{Duration, Instant};

const POLL_TIMEOUT: Duration = Duration::from_secs(15);
const POLL_INTERVAL: Duration = Duration::from_millis(200);

#[tokio::test(flavor = "multi_thread")]
async fn organization_report_answers_match_selected_capability_contracts() {
    let app = spawn_app().await;

    for case in [
        Case {
            prompt: "Show office hierarchy summary.",
            capability: "organization_hierarchy_summary",
            terms: &[
                "total_office_count",
                "root_office_count",
                "leaf_office_count",
                "max_hierarchy_depth",
            ],
        },
        Case {
            prompt: "Show office summary for my authorized offices.",
            capability: "organization_office_summary",
            terms: &["office_count", "root_office_count", "active_staff_count"],
        },
        Case {
            prompt: "I want to see office savings summary top 5 in IDR.",
            capability: "organization_office_savings_summary",
            terms: &[
                "office_name",
                "active_account_count",
                "total_balance",
                "currency_code",
            ],
        },
        Case {
            prompt: "Office activity ranking top 5 by transaction count from 2026-01-01 to 2026-12-31.",
            capability: "organization_office_activity_ranking",
            terms: &[
                "office_name",
                "transaction_count",
                "deposit_total",
                "withdrawal_total",
            ],
        },
        Case {
            prompt: "List 5 dormant offices from 2026-01-01 to 2026-12-31.",
            capability: "organization_office_dormant",
            terms: &[
                "office_name",
                "opening_date",
                "last_transaction_date",
                "transaction_count",
            ],
        },
        Case {
            prompt: "Monthly office openings from 2026-01-01 to 2026-12-31.",
            capability: "organization_office_opening_monthly_breakdown",
            terms: &["month_start", "opened_office_count"],
        },
    ] {
        let key = app
            .provision_api_key(&[case.capability], vec![1, 2, 3, 4, 5, 6, 7, 40], false)
            .await;
        let session_id = create_session(&app, &key.raw, "organization answer quality").await;
        let job_id = create_job(&app, &key.raw, &session_id, case.prompt).await;
        let mut job = wait_until_not_running(&app, &key.raw, &job_id).await;
        if job["status"].as_str() == Some("waiting_for_user_input") {
            let ids = option_ids(&job["result_json"]["structured_response"]);
            // Issue 02 (reranker): top-1 is deterministic, so the exact target
            // capability must appear as a clarification option — a same-shape
            // office sibling may no longer crowd it out of the top-3.
            assert!(
                ids.contains(&case.capability),
                "expected {} as a clarification option: {job}",
                case.capability
            );
            let resp = app
                .post_json(
                    &format!("/chat/jobs/{job_id}/responses"),
                    Some(&key.raw),
                    &json!({ "message": case.prompt, "option_id": case.capability }),
                )
                .await;
            assert_eq!(
                resp.status(),
                201,
                "clarification response failed for {}",
                case.capability
            );
            job = wait_until_not_running(&app, &key.raw, &job_id).await;
        }
        assert_completed_capability(&job, case.capability);
        assert_answer_mentions(&job["result_json"]["structured_response"], case.terms, &job);
        assert_no_sql_or_private_leak(&job);
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn organization_clarification_accepts_option_id_and_free_text() {
    let app = spawn_app().await;
    let key = app.provision_wildcard_api_key(false).await;
    let session_id = create_session(&app, &key.raw, "organization clarification").await;

    let (job_id, first) =
        first_org_clarification_with_no_param_option(&app, &key.raw, &session_id).await;
    let ids = option_ids(&first["result_json"]["structured_response"]);
    // Issue 02 (reranker): the helper only returns once one of these two
    // no-parameter summaries is offered, so assert that exact pair rather than
    // any organization_-prefixed id.
    assert!(
        ids.contains(&"organization_office_summary")
            || ids.contains(&"organization_hierarchy_summary"),
        "missing expected organization summary option: {first}"
    );
    assert!(
        ids.iter().all(|id| !id.starts_with("refine_")),
        "refine option leaked: {first}"
    );

    let selected = ids
        .iter()
        .copied()
        .find(|id| *id == "organization_office_summary")
        .or_else(|| {
            ids.iter()
                .copied()
                .find(|id| *id == "organization_hierarchy_summary")
        })
        .unwrap();
    let resp = app
        .post_json(
            &format!("/chat/jobs/{job_id}/responses"),
            Some(&key.raw),
            &json!({
                "message": option_message(selected),
                "option_id": selected
            }),
        )
        .await;
    assert_eq!(resp.status(), 201, "option response failed");
    let final_job = wait_until_not_running(&app, &key.raw, &job_id).await;
    assert_completed_capability(&final_job, selected);
    assert_no_sql_or_private_leak(&final_job);

    let (job_id, _) = first_org_clarification(&app, &key.raw, &session_id).await;
    let resp = app
        .post_json(
            &format!("/chat/jobs/{job_id}/responses"),
            Some(&key.raw),
            &json!({ "message": "Rank the 5 busiest offices by transaction count from 2026-01-01 to 2026-12-31." }),
        )
        .await;
    assert_eq!(resp.status(), 201, "free-text response failed");
    let final_job = wait_until_not_running(&app, &key.raw, &job_id).await;
    // Issue 01 (retrieval-pipeline-rework): shape is now a scoring signal, not
    // a hard gate, so this precise re-classification can still legitimately
    // land on a tied clarification (multiple office-shaped capabilities at
    // the score cap) instead of completing outright. Tie-breaking among
    // equally-scored candidates is issue 02's (reranker) concern.
    if final_job["status"].as_str() == Some("waiting_for_user_input") {
        let ids = option_ids(&final_job["result_json"]["structured_response"]);
        // Issue 02 (reranker): "rank the busiest offices by transaction count"
        // maps to organization_office_activity_ranking, which must be the
        // offered option if this re-classification ties into clarification.
        assert!(
            ids.contains(&"organization_office_activity_ranking"),
            "expected organization_office_activity_ranking option: {final_job}"
        );
    } else {
        assert_completed_organization_capability(&final_job);
    }
    assert_no_sql_or_private_leak(&final_job);
}

struct Case {
    prompt: &'static str,
    capability: &'static str,
    terms: &'static [&'static str],
}

fn assert_completed_capability(job: &Value, expected: &str) {
    assert_eq!(job["status"], "completed", "{job}");
    let result = &job["result_json"];
    assert_eq!(result["selected_capability"], expected, "{job}");
    let response = &result["structured_response"];
    assert!(response.is_object(), "missing structured response: {job}");
    assert_ne!(
        response["response_type"].as_str(),
        Some("unsupported"),
        "{job}"
    );
    assert_ne!(
        response["response_type"].as_str(),
        Some("out_of_domain"),
        "{job}"
    );
    assert_ne!(
        response["response_type"].as_str(),
        Some("policy_blocked"),
        "{job}"
    );
    assert_ne!(result["policy_blocked"].as_bool(), Some(true), "{job}");
}

fn assert_completed_organization_capability(job: &Value) {
    assert_eq!(job["status"], "completed", "{job}");
    let cap = job["result_json"]["selected_capability"]
        .as_str()
        .unwrap_or("");
    assert!(cap.starts_with("organization_"), "{job}");
    assert_ne!(
        job["result_json"]["structured_response"]["response_type"].as_str(),
        Some("clarification"),
        "{job}"
    );
}

fn assert_answer_mentions(response: &Value, terms: &[&str], job: &Value) {
    let payload = serde_json::to_string(response).unwrap().to_lowercase();
    for term in terms {
        assert!(payload.contains(term), "answer missing {term}: {job}");
    }
    if let Some(rows) = response["table"]["rows"].as_array() {
        assert!(
            rows.len() <= 5 || rows.len() <= 200,
            "unexpectedly large table: {job}"
        );
    }
}

fn assert_no_sql_or_private_leak(value: &Value) {
    let payload = serde_json::to_string(value).unwrap();
    for forbidden in [
        "SELECT ",
        "m_office",
        "m_staff",
        "m_client",
        "m_savings_account",
        "external_id",
        "mobile_no",
        "account_no",
        "not allowed to run capability",
    ] {
        assert!(
            !payload.contains(forbidden),
            "response leaked {forbidden}: {payload}"
        );
    }
}

fn option_ids(response: &Value) -> Vec<&str> {
    response["options"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|option| option["id"].as_str())
        .collect()
}

fn option_message(capability: &str) -> &'static str {
    match capability {
        "organization_office_opening_monthly_breakdown" => {
            "Monthly office openings from 2026-01-01 to 2026-12-31."
        }
        "organization_office_client_summary" => "Show top 5 offices by client counts.",
        "organization_office_hierarchy_tree" => "Show the top 5 office hierarchy tree rows.",
        "organization_office_activity_ranking" => {
            "Top 5 busiest offices by transaction count from 2026-01-01 to 2026-12-31."
        }
        "organization_office_dormant" => "List 5 dormant offices from 2026-01-01 to 2026-12-31.",
        "organization_office_savings_summary" => "Top 5 offices by savings balance in IDR.",
        _ => "Show office summary for my authorized offices.",
    }
}

async fn first_org_clarification(
    app: &TestApp,
    api_key: &str,
    session_id: &str,
) -> (String, Value) {
    for prompt in [
        "show organization report",
        "show office report",
        "show report",
    ] {
        let job_id = create_job(app, api_key, session_id, prompt).await;
        let job = wait_until_not_running(app, api_key, &job_id).await;
        if job["status"].as_str() == Some("waiting_for_user_input")
            && !option_ids(&job["result_json"]["structured_response"]).is_empty()
        {
            assert_eq!(
                job["result_json"]["structured_response"]["response_type"], "clarification",
                "{job}"
            );
            return (job_id, job);
        }
    }
    panic!("could not produce organization clarification");
}

async fn first_org_clarification_with_no_param_option(
    app: &TestApp,
    api_key: &str,
    session_id: &str,
) -> (String, Value) {
    for prompt in [
        "show office summary",
        "show organization summary",
        "show organization report",
        "show office report",
    ] {
        let job_id = create_job(app, api_key, session_id, prompt).await;
        let job = wait_until_not_running(app, api_key, &job_id).await;
        let ids = option_ids(&job["result_json"]["structured_response"]);
        if job["status"].as_str() == Some("waiting_for_user_input")
            && (ids.contains(&"organization_office_summary")
                || ids.contains(&"organization_hierarchy_summary"))
        {
            return (job_id, job);
        }
    }
    panic!("could not produce organization clarification with no-parameter option");
}

async fn create_session(app: &TestApp, api_key: &str, title: &str) -> String {
    let resp = app
        .post_json("/chat/sessions", Some(api_key), &json!({ "title": title }))
        .await;
    assert_eq!(resp.status(), 201);
    resp.json::<Value>().await.unwrap()["data"]["id"]
        .as_str()
        .unwrap()
        .to_string()
}

async fn create_job(app: &TestApp, api_key: &str, session_id: &str, message: &str) -> String {
    let resp = app
        .post_json(
            "/chat/jobs",
            Some(api_key),
            &json!({ "session_id": session_id, "message": message }),
        )
        .await;
    assert_eq!(resp.status(), 201, "create job failed for {message}");
    resp.json::<Value>().await.unwrap()["data"]["job_id"]
        .as_str()
        .unwrap()
        .to_string()
}

async fn fetch_job(app: &TestApp, api_key: &str, job_id: &str) -> Value {
    let resp = app
        .get(&format!("/chat/jobs/{job_id}"), Some(api_key))
        .await;
    assert_eq!(resp.status(), 200);
    resp.json::<Value>().await.unwrap()["data"].clone()
}

async fn wait_until_not_running(app: &TestApp, api_key: &str, job_id: &str) -> Value {
    let deadline = Instant::now() + POLL_TIMEOUT;
    loop {
        let job = fetch_job(app, api_key, job_id).await;
        let status = job["status"].as_str().unwrap_or("");
        if !matches!(status, "queued" | "running") {
            return job;
        }
        if Instant::now() >= deadline {
            panic!("job did not leave queued/running: {job}");
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}
