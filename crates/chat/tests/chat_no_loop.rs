//! Loop-detection integration tests.
//!
//! Reproduces the exact Postman flow that used to loop: ambiguous prompt →
//! clarification with N options → user replies with free text that does NOT
//! match any option → assert the assistant does NOT return the identical
//! clarification again (that's the loop).
//!
//! The core invariant is state-based, not text-based:
//!
//!   After every user reply, the next assistant message must either
//!   (a) resolve to a matched capability (the classifier understood), or
//!   (b) present a DIFFERENT clarification — different options list or
//!       different clarification text — showing the classifier re-evaluated
//!       the input rather than echoing the previous decision.
//!
//! Two consecutive identical clarifications = loop = test failure.

mod common;

use common::{TestApp, spawn_app};
use serde_json::{Value, json};
use std::time::{Duration, Instant};

const POLL_TIMEOUT: Duration = Duration::from_secs(15);
const POLL_INTERVAL: Duration = Duration::from_millis(200);
const SETTLE: Duration = Duration::from_millis(1500);

const CAPS: &[&str] = &[
    "savings_deposit_total",
    "savings_deposit_top_n",
    "savings_deposit_monthly_breakdown",
    "savings_deposit_monthly_top_n",
    "savings_withdrawal_total",
    "savings_withdrawal_top_n",
    "savings_balance_summary",
];

/// One realistic Postman-style scenario.
struct Scenario {
    label: &'static str,
    prompt: &'static str,
    /// The clarification reply the user would actually type — deliberately
    /// free-text that doesn't exactly match option labels or capability ids.
    free_text_reply: &'static str,
}

const SCENARIOS: &[Scenario] = &[
    Scenario {
        label: "activity_this_month_typo",
        prompt: "Show customer savings activity this month",
        free_text_reply: "all acticity for this month",
    },
    Scenario {
        label: "activity_this_week_free_text",
        prompt: "Show customer savings activity this week",
        free_text_reply: "give me the biggest deposit transactions",
    },
    Scenario {
        label: "activity_bilingual_id",
        prompt: "Lihat aktivitas tabungan bulan ini",
        free_text_reply: "total setoran bulan ini",
    },
];

#[tokio::test(flavor = "multi_thread")]
async fn ambiguous_prompt_never_loops_across_scenarios() {
    let app = spawn_app().await;
    let key = app.provision_api_key(CAPS, vec![1, 2, 3], true).await;

    for scenario in SCENARIOS {
        run_scenario(&app, &key.raw, scenario).await;
    }
}

async fn run_scenario(app: &TestApp, api_key: &str, sc: &Scenario) {
    // ---------- 1. Session + job ----------
    let sess = app
        .post_json(
            "/chat/sessions",
            Some(api_key),
            &json!({ "title": sc.label }),
        )
        .await;
    assert_eq!(sess.status(), 201, "[{}] session create", sc.label);
    let session_id = sess.json::<Value>().await.unwrap()["data"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let job = app
        .post_json(
            "/chat/jobs",
            Some(api_key),
            &json!({ "session_id": session_id, "message": sc.prompt }),
        )
        .await;
    assert_eq!(job.status(), 201, "[{}] job create", sc.label);
    let job_id = job.json::<Value>().await.unwrap()["data"]["job_id"]
        .as_str()
        .unwrap()
        .to_string();

    // ---------- 2. Turn-1 terminal ----------
    let after_turn1 = wait_until_not_running(app, api_key, &job_id).await;
    let status1 = after_turn1["status"].as_str().unwrap_or("");

    // Path A: classifier understood the prompt directly.
    if status1 == "completed" {
        let cap = after_turn1["state_json"]["classification"]["capability"]
            .as_str()
            .unwrap_or("");
        assert!(
            !cap.is_empty(),
            "[{}] completed but no capability: {after_turn1}",
            sc.label
        );
        return;
    }

    // Path B: clarification. Must be waiting_for_user_input with options.
    assert_eq!(
        status1, "waiting_for_user_input",
        "[{}] unexpected turn-1 status={status1}: {after_turn1}",
        sc.label
    );
    let options1 = extract_options(&after_turn1);
    let clar1 = after_turn1["state_json"]["classification"]["clarification"]
        .as_str()
        .unwrap_or("")
        .to_string();
    assert!(
        !options1.is_empty(),
        "[{}] turn-1 clarification MUST have options — empty options is the loop signature: {after_turn1}",
        sc.label
    );
    assert!(
        options1
            .iter()
            .any(|opt| opt["capability"] == "other_activity"),
        "[{}] Others option MUST be present. Options: {options1:?}",
        sc.label
    );

    // ---------- 3. User replies with free text that does NOT match any option ----------
    let resp = app
        .post_json(
            &format!("/chat/jobs/{job_id}/responses"),
            Some(api_key),
            &json!({ "message": sc.free_text_reply }),
        )
        .await;
    assert_eq!(
        resp.status(),
        201,
        "[{}] responses POST: {}",
        sc.label,
        resp.text().await.unwrap_or_default()
    );

    tokio::time::sleep(SETTLE).await;

    // ---------- 4. Assert NO LOOP: turn-2 must not be identical to turn-1 ----------
    let after_turn2 = fetch_job(app, api_key, &job_id).await;
    let status2 = after_turn2["status"].as_str().unwrap_or("");
    let outcome2 = after_turn2["state_json"]["classification"]["outcome"]
        .as_str()
        .unwrap_or("");

    // Terminal success (matched → completed, or gracefully failed after classify).
    if matches!(status2, "completed" | "failed") {
        // System understood or reached a definite decision. No loop possible.
        return;
    }

    // Still waiting? Then the clarification MUST be different from turn-1.
    assert_eq!(
        status2, "waiting_for_user_input",
        "[{}] unexpected turn-2 status={status2}: {after_turn2}",
        sc.label
    );

    let options2 = extract_options(&after_turn2);
    let clar2 = after_turn2["state_json"]["classification"]["clarification"]
        .as_str()
        .unwrap_or("")
        .to_string();

    let same_options = options1 == options2;
    let same_clarification = clar1 == clar2;
    assert!(
        !(same_options && same_clarification),
        "[{}] LOOP DETECTED — turn-2 clarification is identical to turn-1 (system \
         did not re-classify the free-text reply).\n\
         Prompt:            {}\n\
         Reply:             {}\n\
         Same clarification: {clar2:?}\n\
         Same options:      {options2:?}\n\
         Full turn-2 job:   {after_turn2}",
        sc.label,
        sc.prompt,
        sc.free_text_reply
    );

    // Even if options differ, outcome must not stay stuck saying "please choose"
    // with an empty options list — that combination is a broken UX state.
    assert!(
        !(outcome2 == "clarification_required" && options2.is_empty()),
        "[{}] BROKEN STATE — clarification_required with EMPTY options. User \
         cannot progress. Turn-2 job: {after_turn2}",
        sc.label
    );
}

fn extract_options(job: &Value) -> Vec<Value> {
    job["state_json"]["classification"]["options"]
        .as_array()
        .cloned()
        .unwrap_or_default()
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
        let status = job["status"].as_str().unwrap_or("").to_string();
        if !matches!(status.as_str(), "queued" | "running") {
            return job;
        }
        if Instant::now() >= deadline {
            panic!("job did not leave queued/running: {job}");
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}
