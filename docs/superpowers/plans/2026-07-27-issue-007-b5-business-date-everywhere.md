# Bundle 5 (W-B) — Business date correctness end to end — Plan

**Spec:** `docs/superpowers/specs/2026-07-27-issue-007-b5-business-date-everywhere-design.md`
**Goal:** Re-anchor every relative temporal expression on `business_today`; add a
reporting-date response note when business date ≠ wall clock; pin the
audit-timestamp-uses-`Utc::now()` invariant with a test.

**Architecture:** deterministic extractor
(`assistant/understanding/extraction/`) is the only relative-date resolver.
`CanonicalRuntimeContext` already carries `business_today` +
`business_date_source`; thread the first into the extractor, read the second at
the single response-build site.

**Global constraints:** no new deps/migrations/YAML; SQL untouched; English-only
copy; audit timestamps stay `Utc::now()`. No commit steps — the user commits.

Work top-to-bottom. Every code step is TDD: write/adjust the failing test, run it
RED, implement, run it GREEN.

---

## Task 1 — Re-anchor `resolve_temporal` on `business_today`

File: `crates/chat/src/assistant/understanding/extraction/temporal.rs`

- [ ] **1.1 (signature + anchor).** Add the `business_today` parameter and replace
  the wall-clock anchor. Change the top of `resolve_temporal`:

  From:
  ```rust
  pub(super) fn resolve_temporal(
      message: &str,
      reference_instant: DateTime<Utc>,
      max_range_days: i64,
  ) -> Result<Option<ResolvedTemporal>, TemporalValidationError> {
      let lower = message.to_ascii_lowercase();
      let tokens = tokens_with_spans(&lower);
      let jakarta = FixedOffset::east_opt(7 * 3600).expect("valid Jakarta offset");
      let today = reference_instant.with_timezone(&jakarta).date_naive();
  ```
  To:
  ```rust
  pub(super) fn resolve_temporal(
      message: &str,
      reference_instant: DateTime<Utc>,
      business_today: NaiveDate,
      max_range_days: i64,
  ) -> Result<Option<ResolvedTemporal>, TemporalValidationError> {
      let lower = message.to_ascii_lowercase();
      let tokens = tokens_with_spans(&lower);
      let today = business_today;
  ```

- [ ] **1.2 (drop unused import).** In the `use chrono::{...}` line at the top of
  the file, remove `FixedOffset` (now unused). Result:
  ```rust
  use chrono::{DateTime, Datelike, Duration, NaiveDate, Utc};
  ```
  `reference_instant` is still stored into `TemporalProvenance` (unchanged), and
  the `timezone: "Asia/Jakarta"` provenance label stays as-is.

- [ ] **1.3 (compile gate).** `cargo check -p chat` — expect errors only at the
  callers of `resolve_temporal` and `extract_message_facts_at` (fixed in Task 2).
  Confirm no error about `FixedOffset` unused and none inside `temporal.rs` itself.

---

## Task 2 — Thread `business_today` through the extractor

File: `crates/chat/src/assistant/understanding/extraction/mod.rs`

- [ ] **2.1 (import).** Ensure `NaiveDate` is imported. The file already imports
  from `chrono` (`DateTime`, `Utc`). Add `NaiveDate` to that import group.

- [ ] **2.2 (convenience fn).** Update `extract_message_facts`:
  ```rust
  pub fn extract_message_facts(message: &str) -> DeterministicExtraction {
      let now = Utc::now();
      extract_message_facts_at(message, now, now.date_naive(), 366)
  }
  ```

- [ ] **2.3 (main fn signature + call).** Update `extract_message_facts_at`:
  ```rust
  pub fn extract_message_facts_at(
      message: &str,
      reference_instant: DateTime<Utc>,
      business_today: NaiveDate,
      max_range_days: i64,
  ) -> DeterministicExtraction {
  ```
  and its call into the resolver:
  ```rust
  match resolve_temporal(message, reference_instant, business_today, max_range_days) {
  ```

File: `crates/chat/src/assistant/execution/runtime/extraction.rs`

- [ ] **2.4 (production call site).** Replace the `extract_for_context` body's
  `.map(...)` line:

  From:
  ```rust
  .map(|context| extract_message_facts_at(message, context.reference_instant, 366))
  ```
  To:
  ```rust
  .map(|context| {
      extract_message_facts_at(
          message,
          context.reference_instant,
          context.business_today,
          366,
      )
  })
  ```

- [ ] **2.5 (compile gate).** `cargo check -p chat` — expect remaining errors only
  in test files (`extraction/tests.rs`) at `extract_message_facts_at(...)` call
  sites. Fixed in Task 3.

---

## Task 3 — Per-expression, per-language tests (RED → GREEN)

File: `crates/chat/src/assistant/understanding/extraction/tests.rs`

- [ ] **3.1 (fix existing 3-arg call sites).** Every existing
  `extract_message_facts_at("...", instant, 366)` call now needs a
  `business_today` argument. For the calls that assert wall-clock-derived dates,
  pass the same date those tests expect so behaviour is preserved. Concretely:

  - `temporal_uses_jakarta_date_and_exact_period_boundaries`: the `today`/`year`
    cases used `instant = 2026-01-01T17:30:00Z` (Jakarta date 2026-01-02). Pass
    `NaiveDate::from_ymd_opt(2026, 1, 2).unwrap()`:
    ```rust
    let bt = NaiveDate::from_ymd_opt(2026, 1, 2).unwrap();
    let today = extract_message_facts_at("show deposits today", instant, bt, 366);
    // ...
    let year = extract_message_facts_at("laporan tahun ini", instant, bt, 366);
    ```
    The `week` case: pass `NaiveDate::from_ymd_opt(2026, 3, 11).unwrap()`:
    ```rust
    let week = extract_message_facts_at(
        "last week",
        reference("2026-03-11T12:00:00Z"),
        NaiveDate::from_ymd_opt(2026, 3, 11).unwrap(),
        366,
    );
    ```
  - `temporal_validates_dates_ranges_and_counts` (`instant = 2026-03-11T12:00:00Z`):
    pass `NaiveDate::from_ymd_opt(2026, 3, 11).unwrap()` as the 3rd arg to each
    `extract_message_facts_at(..., instant, <bt>, 366)`. The explicit-range and
    error cases do not depend on the anchor; passing the matching date keeps the
    `last 3 days` expectation (`2026-03-09..2026-03-11`) intact.
  - `temporal_reuses_the_same_job_reference_after_clarification`
    (`job_reference = 2026-12-31T18:00:00Z`, Jakarta date 2027-01-01): pass
    `NaiveDate::from_ymd_opt(2027, 1, 1).unwrap()` to both calls so the two
    resolutions still agree.

  Ensure `use chrono::NaiveDate;` (or the existing chrono import) covers `NaiveDate`
  in this test module.

- [ ] **3.2 (RED — new business-date matrix test).** Add this test. It uses a
  `business_today` that differs from the wall-clock `reference_instant`, so it
  FAILS under the old anchor.
  ```rust
  #[test]
  fn relative_expressions_derive_from_business_date_both_languages() {
      // Wall clock says the 25th; tenant business date is the 23rd (a Thursday).
      let wall = reference("2026-07-25T02:00:00Z");
      let bt = NaiveDate::from_ymd_opt(2026, 7, 23).unwrap();

      // (phrase, expected_from, expected_to) — one row per expression per language.
      let cases: &[(&str, &str, &str)] = &[
          ("today", "2026-07-23", "2026-07-23"),
          ("hari ini", "2026-07-23", "2026-07-23"),
          ("yesterday", "2026-07-22", "2026-07-22"),
          ("kemarin", "2026-07-22", "2026-07-22"),
          ("this week", "2026-07-20", "2026-07-26"),
          ("minggu ini", "2026-07-20", "2026-07-26"),
          ("last week", "2026-07-13", "2026-07-19"),
          ("minggu lalu", "2026-07-13", "2026-07-19"),
          ("this month", "2026-07-01", "2026-07-31"),
          ("bulan ini", "2026-07-01", "2026-07-31"),
          ("last month", "2026-06-01", "2026-06-30"),
          ("bulan lalu", "2026-06-01", "2026-06-30"),
          ("this quarter", "2026-07-01", "2026-09-30"),
          ("kuartal ini", "2026-07-01", "2026-09-30"),
          ("last quarter", "2026-04-01", "2026-06-30"),
          ("kuartal lalu", "2026-04-01", "2026-06-30"),
          ("this year", "2026-01-01", "2026-12-31"),
          ("tahun ini", "2026-01-01", "2026-12-31"),
          ("last year", "2025-01-01", "2025-12-31"),
          ("tahun lalu", "2025-01-01", "2025-12-31"),
          ("last 3 days", "2026-07-21", "2026-07-23"),
          ("3 hari terakhir", "2026-07-21", "2026-07-23"),
      ];

      for (phrase, from, to) in cases {
          let e = extract_message_facts_at(phrase, wall, bt, 366);
          assert_eq!(
              e.constraints.from_date.as_deref(),
              Some(*from),
              "from mismatch for {phrase}"
          );
          assert_eq!(
              e.constraints.to_date.as_deref(),
              Some(*to),
              "to mismatch for {phrase}"
          );
      }
  }
  ```

- [ ] **3.3 (RED — provenance keeps wall clock).** Add:
  ```rust
  #[test]
  fn provenance_reference_instant_stays_wall_clock() {
      let wall = reference("2026-07-25T02:00:00Z");
      let bt = NaiveDate::from_ymd_opt(2026, 7, 23).unwrap();
      let e = extract_message_facts_at("kemarin", wall, bt, 366);
      // dates come from business_today...
      assert_eq!(e.constraints.from_date.as_deref(), Some("2026-07-22"));
      // ...but the audit provenance still carries the wall-clock instant.
      let p = e.temporal_provenance.unwrap();
      assert_eq!(p.reference_instant, wall);
      assert_eq!(p.timezone, "Asia/Jakarta");
  }
  ```

- [ ] **3.4 (RED run).** `cargo test -p chat relative_expressions_derive_from_business_date_both_languages`
  then `cargo test -p chat provenance_reference_instant_stays_wall_clock`.
  Expect FAIL (old anchor produces 2026-07-24/25-based dates). This is only a valid
  RED if Task 1/2 are already applied and compiling — if so, run against the new
  code and they now pass. If you sequenced tests first, temporarily revert 1.1's
  `let today = business_today;` to the old line to observe RED, then restore.

- [ ] **3.5 (GREEN run).** `cargo test -p chat temporal` — all temporal tests pass,
  including the updated `temporal_uses_jakarta_date_and_exact_period_boundaries`
  and `temporal_validates_dates_ranges_and_counts`. Expected tail:
  `test result: ok.`

---

## Task 4 — Reporting-date response note

File: `crates/chat/src/assistant/presentation/builder.rs`

- [ ] **4.1 (imports).** Add to the top-of-file imports:
  ```rust
  use crate::assistant::temporal::BusinessDateSource;
  use chrono::NaiveDate;
  ```
  (`ResponseWarning` is already imported at line 10.)

- [ ] **4.2 (RED — helper test).** Add a test module entry (or extend the existing
  `#[cfg(test)] mod tests` in this file) with:
  ```rust
  #[test]
  fn reporting_date_note_only_when_fineract_and_differing() {
      let biz = NaiveDate::from_ymd_opt(2026, 7, 23).unwrap();
      let wall_same = NaiveDate::from_ymd_opt(2026, 7, 23).unwrap();
      let wall_diff = NaiveDate::from_ymd_opt(2026, 7, 25).unwrap();

      // Fineract + differing -> note present.
      let note = ResponseBuilder::reporting_date_note(
          biz,
          BusinessDateSource::Fineract,
          wall_diff,
      );
      let note = note.expect("expected a reporting-date note");
      assert_eq!(note.code, "reporting_date");
      assert!(note.message.contains("2026-07-23"));
      assert!(note.message.contains("2026-07-25"));

      // Same date -> no note.
      assert!(
          ResponseBuilder::reporting_date_note(biz, BusinessDateSource::Fineract, wall_same)
              .is_none()
      );

      // Wall-clock fallback -> no note (we only trust an actual Fineract date).
      assert!(
          ResponseBuilder::reporting_date_note(
              biz,
              BusinessDateSource::WallClockFallback,
              wall_diff,
          )
          .is_none()
      );
  }
  ```

- [ ] **4.3 (GREEN — implement helper).** Add inside `impl ResponseBuilder`:
  ```rust
  /// Note surfaced when the reporting date (tenant business date) differs from
  /// the calendar date, so an analyst is never silently answered "today" with a
  /// date they did not expect. Only emitted for a real Fineract business date —
  /// a wall-clock fallback already *is* the calendar date.
  pub fn reporting_date_note(
      business_today: NaiveDate,
      source: BusinessDateSource,
      wall_today: NaiveDate,
  ) -> Option<ResponseWarning> {
      (source == BusinessDateSource::Fineract && business_today != wall_today).then(|| {
          ResponseWarning {
              code: "reporting_date".into(),
              message: format!(
                  "Reporting date is the Fineract business date {business_today}, \
                   which differs from the calendar date {wall_today}."
              ),
          }
      })
  }
  ```

- [ ] **4.4 (RED run then GREEN).** `cargo test -p chat reporting_date_note_only_when_fineract_and_differing`.
  Expect FAIL before 4.3 (method missing → compile error), PASS after.

File: `crates/chat/src/assistant/execution/runtime/execution.rs`

- [ ] **4.5 (inject at the single success site).** At the `Ok(result) =>` arm where
  the table response is built (around line 238), capture the response as mutable
  and push the note using the `canonical` context already in scope. Change:
  ```rust
          let response = ResponseBuilder::from_tool_result(
              intent.as_ref().expect("successful execution has intent"),
              &plan,
              &policy,
              &tool_result,
              catalog,
          );
  ```
  to:
  ```rust
          let mut response = ResponseBuilder::from_tool_result(
              intent.as_ref().expect("successful execution has intent"),
              &plan,
              &policy,
              &tool_result,
              catalog,
          );
          if let Some(context) = canonical {
              let jakarta = chrono::FixedOffset::east_opt(7 * 3600)
                  .expect("valid Jakarta offset");
              let wall_today = context
                  .reference_instant
                  .with_timezone(&jakarta)
                  .date_naive();
              if let Some(note) = ResponseBuilder::reporting_date_note(
                  context.business_today,
                  context.business_date_source,
                  wall_today,
              ) {
                  response.warnings.push(note);
              }
          }
  ```
  (Confirm `ResponseBuilder` is already imported in this file — it is, used at
  `execution.rs:19`. `canonical` is the function parameter already read at
  `execution.rs:98,116`.)

- [ ] **4.6 (compile gate).** `cargo check -p chat`. Expect clean.

---

## Task 5 — Audit-timestamp invariant guard

File: `crates/chat/src/assistant/temporal/business_date.rs`

- [ ] **5.1 (RED — new test in the existing `mod tests`).** Add:
  ```rust
  #[tokio::test]
  async fn resolved_at_is_wall_clock_not_business_date() {
      // A deliberately stale business date must not leak into the audit timestamp.
      let before = Utc::now();
      let provider = StaticBusinessDateProvider {
          value: NaiveDate::from_ymd_opt(2000, 1, 1).unwrap(),
          source: BusinessDateSource::Fineract,
      };
      let result = provider.today().await.unwrap();
      let after = Utc::now();

      // resolved_at (used as the audit `occurred_at`) tracks Utc::now()...
      assert!(result.resolved_at >= before && result.resolved_at <= after);
      // ...never the (stale) business date.
      assert_ne!(result.resolved_at.date_naive(), result.date);
  }
  ```

- [ ] **5.2 (run — already GREEN).** `cargo test -p chat business_date`. This passes
  against current code (every provider sets `resolved_at: Utc::now()`); the test
  exists so a future refactor that swaps in `business_today` fails here. Expected
  tail: `test result: ok.`

---

## Task 6 — Full verification

- [ ] **6.1** `cargo fmt`
- [ ] **6.2** `cargo check` — expect clean (whole workspace).
- [ ] **6.3** `cargo test -p chat temporal` — expect `test result: ok.`
- [ ] **6.4** `cargo test -p chat business_date` — expect `test result: ok.`
- [ ] **6.5** `cargo test -p chat reporting_date` — expect `test result: ok.`
- [ ] **6.6** `cargo test -p chat` — expect all green. Pay attention to
  `assistant_response.rs`, `scenario_matrix.rs`, and any test asserting a fixed
  `warnings` array: a table response whose canonical fixture has
  `business_today != Jakarta(reference_instant)` and `source == Fineract` will now
  carry an extra `reporting_date` warning. If a fixture fails, either align its
  `business_today` to its `reference_instant`'s Jakarta date (no note) or update
  the expected `warnings`. Do not weaken the note logic to satisfy a fixture.
