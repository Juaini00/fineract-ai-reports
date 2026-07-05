# Contract Test Fixtures — Design Spec

**Date:** 2026-07-05
**Status:** design, awaiting implementation plan
**Related:** `docs/capability-coverage-matrix.md` (contract test policy), `crates/chat/tests/common/mod.rs` (existing harness)

## Problem

Existing integration tests validate infrastructure guardrails (routes wired, no crash, no data leak) but **do not verify semantic correctness**. A user prompt that maps to the wrong capability, or extracts wrong params, passes the current test suite. Manual Postman testing repeatedly surfaces bugs the integration test suite cannot see.

The user's complaint is direct: "integration test lolos 100% tapi manual test sederhana gagal — ini tidak masuk akal."

`docs/capability-coverage-matrix.md` declares the contract test policy: every implemented cell must have at least one fixture entry that asserts the prompt maps to the right capability. That fixture file (`crates/chat/tests/fixtures/prompts.yaml`) does not yet exist. This spec designs it.

## Goal

Build a data-driven contract test that iterates a YAML fixture and, for each scenario, asserts:

1. The classifier's chosen capability is the expected capability.
2. Core parameters (from_date, to_date, limit) are extracted correctly.
3. When a prompt is deliberately ambiguous, the classifier requests clarification with the expected options (and the Others escape hatch).

The test framework itself must be:
- **Deterministic** — no flakiness between runs on the same code
- **Externalizable** — fixture data is YAML, not Rust code; edit without recompiling
- **Bilingual** — first-class support for English + Indonesian prompt pairs
- **Time-independent** — fixtures use symbolic date tokens resolved at run time; no need to update fixtures as the calendar moves

## Non-Goals for this iteration

- Planned / deferred outcomes → depend on the future `planned_unimplemented` outcome (separate slice).
- Response text assertions → formatter has its own test scope.
- Row-count / actual data assertions → require Fineract seed data; belongs to a separate integration layer.
- Voyage semantic quality validation → runs manually via a separate `#[ignore]` suite when the developer has a Voyage API key.

## Architecture

Iteration test lives in `crates/chat/tests/contract_prompts.rs`. It calls `spawn_app()` from `tests/common/mod.rs` (existing harness), which builds `AppConfig` in-process with `VOYAGEAI_API_KEY = ""`. That empty key forces `chat::chat::service::job::JobService::classify_with_retrieval` to fall through to `catalog_lexical_candidates` — the deterministic path.

For each scenario:

```
prompts.yaml
    ↓ serde_yaml deserialize
Vec<Scenario>
    ↓ for each scenario:
    ├─ resolve symbolic date tokens with today = chrono::Utc::now().date_naive()
    ├─ spawn_app().await (once per test binary; reused across scenarios in a batch)
    ├─ POST /chat/jobs { message: scenario.prompt }
    ├─ poll GET /chat/jobs/{id} until terminal
    └─ assert expected outcome + capability + params
```

Voyage-based semantic quality lives in `crates/chat/tests/voyage_semantic.rs`, gated with `#[ignore]`, run with `cargo test -- --ignored voyage_semantic` when `VOYAGEAI_API_KEY` is set locally.

## Components

### `crates/chat/tests/fixtures/prompts.yaml`

Data. Array of scenarios. Editing this file is the primary way to add contract coverage.

Schema:

```yaml
scenarios:
  - id: <string, unique>              # snake_case scenario id for grep-ability
    prompt: <string>                  # the user's natural-language input
    expected:
      outcome: matched                # or "clarification_required" or "unsupported"
      capability: <string>            # required when outcome=matched
      params:                         # optional; when present, keys must match
        from_date: <symbolic|literal>
        to_date: <symbolic|literal>
        limit: <integer|any>
      options_include:                # only for clarification_required
        - <capability_id>
      others_option: true             # only for clarification_required; default true
      reason_contains: <substring>    # only for unsupported
```

### `crates/chat/tests/fixtures/mod.rs`

Loader + resolver. Convention note: `mod fixtures;` inside `contract_prompts.rs` resolves to `tests/fixtures/mod.rs` per Rust 2018+ module rules — same pattern already used by `mod common;` in the existing test suite. The YAML data file `prompts.yaml` sits alongside the module code inside the `fixtures/` directory.

```rust
#[derive(Debug, Clone, Deserialize)]
pub struct FixtureFile {
    pub scenarios: Vec<Scenario>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Scenario {
    pub id: String,
    pub prompt: String,
    pub expected: Expected,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum Expected {
    Matched {
        capability: String,
        #[serde(default)]
        params: HashMap<String, ParamValue>,
    },
    ClarificationRequired {
        #[serde(default)]
        options_include: Vec<String>,
        #[serde(default = "default_true")]
        others_option: bool,
    },
    Unsupported {
        #[serde(default)]
        reason_contains: Option<String>,
    },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum ParamValue {
    Symbolic(SymbolicToken),     // parsed as enum
    Literal(serde_json::Value),  // integer, string, etc
    Any,                          // "any" sentinel means assertion skipped
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SymbolicToken {
    Today,
    Yesterday,
    StartOfCurrentMonth,
    EndOfCurrentMonth,
    StartOfLastMonth,
    EndOfLastMonth,
    StartOfCurrentYear,
    EndOfCurrentYear,
    StartOfLastYear,
    EndOfLastYear,
    TodayMinus7d,
    TodayMinus30d,
}

pub fn load_fixtures() -> FixtureFile { /* serde_yaml from workspace-relative path */ }

pub fn resolve_symbolic(token: SymbolicToken, today: NaiveDate) -> NaiveDate { /* match */ }
```

### `crates/chat/tests/contract_prompts.rs`

The iterator test. Single `#[tokio::test]` function that loads the fixture and asserts each scenario. Emits detailed failure messages including scenario id, prompt, expected, actual, and full job state.

Skeleton:

```rust
mod common;
mod fixtures;

use fixtures::{load_fixtures, resolve_symbolic, Expected};

#[tokio::test(flavor = "multi_thread")]
async fn every_fixture_scenario_matches_expected_outcome() {
    let today = chrono::Utc::now().date_naive();
    let file = load_fixtures();
    let app = common::spawn_app().await;
    let key = app
        .provision_api_key(ALL_APPROVED_CAPS, vec![1, 2, 3], true)
        .await;

    let mut failures = Vec::new();
    for scenario in &file.scenarios {
        if let Err(msg) = run_scenario(&app, &key.raw, scenario, today).await {
            failures.push(msg);
        }
    }
    assert!(
        failures.is_empty(),
        "{} fixture scenarios failed:\n{}",
        failures.len(),
        failures.join("\n---\n")
    );
}
```

`run_scenario` returns `Result<(), String>` so all scenarios run to completion and the developer sees every failure in a single run.

### `crates/chat/tests/voyage_semantic.rs` (opt-in)

Same shape but with a real Voyage API key. All tests marked `#[ignore]`. Documented in `docs/scenarios/README.md` how to run:

```bash
VOYAGEAI_API_KEY=your-key cargo test --test voyage_semantic -- --ignored
```

Optional in this iteration — can be a follow-up commit.

## Data Flow

### Symbolic date resolution

`start_of_current_month` with today=`2026-07-05` resolves to `2026-07-01`.
`start_of_last_year` resolves to `2025-01-01`.
`today_minus_7d` resolves to `2026-06-28`.

All computed in `resolve_symbolic` given the run-time `today`. Fixture stays stable across dates.

### Assertion

```rust
fn assert_matched(actual: &Value, expected: &Matched, today: NaiveDate) -> Result<(), String> {
    let actual_cap = actual["state_json"]["classification"]["capability"].as_str();
    if actual_cap != Some(&expected.capability) {
        return Err(format!("capability mismatch: expected {}, got {:?}", expected.capability, actual_cap));
    }
    for (key, expected_value) in &expected.params {
        let actual_value = &actual["state_json"]["classification"]["params"][key];
        match expected_value {
            ParamValue::Any => continue,
            ParamValue::Symbolic(tok) => {
                let expected_date = resolve_symbolic(*tok, today);
                if actual_value.as_str() != Some(&expected_date.to_string()) {
                    return Err(format!(
                        "param {} mismatch: expected {} (symbolic {}), got {:?}",
                        key, expected_date, format!("{tok:?}"), actual_value
                    ));
                }
            }
            ParamValue::Literal(v) => {
                if actual_value != v {
                    return Err(format!("param {} mismatch: expected {}, got {}", key, v, actual_value));
                }
            }
        }
    }
    Ok(())
}
```

Analogous helpers `assert_clarification` and `assert_unsupported`.

### Coverage day 1

22 scenarios, bilingual EN + ID per capability. Distribution:

| Capability | EN prompt | ID prompt |
|---|---|---|
| `savings_balance_summary` | "What is the total savings balance right now?" | "Berapa saldo total tabungan aktif saat ini?" |
| `savings_deposit_total` | "What is the total deposit this month?" | "Berapa total setoran bulan ini?" |
| `savings_deposit_top_n` | "Top 10 deposits this month" | "10 setoran terbesar bulan ini" |
| `savings_deposit_monthly_breakdown` | "Monthly deposits this year" | "Setoran per bulan tahun ini" |
| `savings_deposit_monthly_top_n` | "Top 3 deposits per month this year" | "3 setoran terbesar per bulan tahun ini" |
| `savings_withdrawal_total` | "What is the total withdrawal this month?" | "Berapa total penarikan bulan ini?" |
| `savings_withdrawal_top_n` | "Top 10 withdrawals this month" | "10 penarikan terbesar bulan ini" |
| `savings_withdrawal_monthly_breakdown` | "Monthly withdrawals this year" | "Penarikan per bulan tahun ini" |
| `savings_withdrawal_monthly_top_n` | "Top 3 withdrawals per month this year" | "3 penarikan terbesar per bulan tahun ini" |
| `organization_office_summary` | "Office summary" | "Ringkasan kantor" |
| `client_lifecycle_summary` | "Client lifecycle summary" | "Ringkasan siklus hidup klien" |

Balance summary and office summary have no date params (snapshot). Others have `from_date` + `to_date`; `top_n` variants also have `limit`.

## Error Handling

The test binary must:

1. **Fail loudly on YAML parse error** — the loader panics with the file path and line number.
2. **Fail loudly on unknown symbolic token** — deserializer error names the token.
3. **Continue after each scenario failure** — collect all failures, report at end. Developer sees every problem in one run, not the first.
4. **Emit precise mismatch messages** — scenario id, prompt, expected, actual, plus full job state as fallback for debugging.

## Testing the Framework

The test framework itself needs three self-tests inside `crates/chat/src/lib.rs` or a dedicated unit test file:

1. `resolve_symbolic` produces the expected date for every token given a fixed `today = 2026-07-05`.
2. `serde_yaml::from_str` parses a hand-written minimal fixture yaml without error.
3. `serde_yaml::from_str` fails with a message that names the offending scenario when an unknown token is used.

## Interfaces That Touch Existing Code

- `spawn_app()` — no change (already exists in `crates/chat/tests/common/mod.rs`).
- `POST /chat/jobs`, `GET /chat/jobs/{id}` — no change (already exists).
- Runtime YAML in `knowledge/capabilities/*.yaml` — read but not modified.
- No Rust production code changes.

## Success Criteria

1. `cargo test --test contract_prompts` runs all 22 scenarios and reports pass or fail per scenario.
2. Adding a new capability requires only: (a) YAML file in `knowledge/capabilities/`, (b) two prompt lines in `prompts.yaml`. No Rust edits.
3. A regression in classifier (wrong capability picked) surfaces as one or more scenario failures with clear "expected X got Y" messages.
4. Voyage embedding changes do not cause flakiness because the test forces the lexical path.

## Risks

- **Lexical fallback path is not the production path.** If the lexical classifier and Voyage classifier diverge in their decisions on the same prompt, the contract test passes while production fails. Mitigation: the opt-in `voyage_semantic.rs` suite is the second line of defence. Longer-term, we make the two paths converge or snapshot Voyage embeddings.
- **Fixture drift.** New capabilities added without adding fixture entries stay silently untested. Mitigation: a meta-assertion that `catalog.capabilities.len() * 2 <= scenarios.len()` fails when someone adds a capability without adding prompts. This is a compile-time guardrail that forces the discipline.
- **Ambiguous fixtures.** Two symbolic tokens may resolve to the same date at a specific run-time, hiding a bug. Low risk; symbolic vocabulary is small.

## Follow-Up Work

- Iteration 2: expand to 5+ prompts per capability including edge cases (typo, paraphrase, mixed language).
- Iteration 3: add `planned_unimplemented` outcome and populate fixtures for planned capabilities.
- Iteration 4: extend to `voyage_semantic.rs` with API-key-gated tests.
- Iteration 5 onward: add fixture rows as each new capability lands.

## Cross References

- Coverage matrix: `docs/capability-coverage-matrix.md`
- Existing harness: `crates/chat/tests/common/mod.rs`
- Classifier: `crates/chat/src/chat/classifier.rs`, `crates/chat/src/chat/service/job.rs`
