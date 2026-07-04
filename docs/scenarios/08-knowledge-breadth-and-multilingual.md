# 08 — Knowledge Breadth & Multilingual Probe

**Phase covered:** Knowledge catalog expansion (Phase 10 + Phase 18 broader retrieval) + off-domain override (`JobService::context_overrides_capability`).
**Precondition:** Catalog rebuilt after the domain/concept/intent expansion. Run `POST /vector-index/rebuild` first so concept `synonyms` (English + Bahasa Indonesia) are embedded into the retrieval index.

## Why this scenario exists

After the knowledge update, every domain (`savings`, `client`, `organization`, `group_center`, `loan`, `accounting`, `tax`) now contributes:

- `display_name` and `description` lines to its retrieval document.
- Concept ids + `meaning` + `synonyms` (including Bahasa Indonesia terms like `kredit`, `pinjaman`, `angsuran`, `jurnal`, `tutup buku`, `pajak`, `kelompok`, `pusat`).
- Multiple `supported_intents` and `unsupported_intents` per domain.

This widens the retrieval surface from 7 short documents to 7 rich documents per domain plus 13 data area documents. The scenario verifies that the broader vocabulary is indexed and that decision policy still routes each prompt to the right outcome.

## A. Domain match probe (English)

For each prompt below, fire `POST /chat/jobs` with `{{API_KEY}}` (full scope) and inspect `state_json.classification`.

| # | Prompt | Expected `outcome` | Expected `source` | Top non-capability candidate (in `candidates`) |
| --- | --- | --- | --- | --- |
| A1 | "Total savings deposit this month" | `matched` | `vector` | `savings (domain)` or empty (savings already chosen as capability) |
| A2 | "Largest deposits today" | `matched` | `vector` | savings capability — no override |
| A3 | "Who made the largest deposit today" | `matched` | `vector` | savings; result rows include `client_id` + `client_display_name` |
| A4 | "Show me journal entries today" | `unsupported` | `off_domain_match` | `accounting (domain)`, `accounting_gl (data_area)` |
| A5 | "Total loan disbursement this month" | `unsupported` | `off_domain_match` | `loan (domain)`, `loans (data_area)` |
| A6 | "Trial balance for last quarter" | `unsupported` | `off_domain_match` or `vector_no_match` | `accounting (domain)` |
| A7 | "Tax collected this quarter" | `unsupported` | `vector_no_match` | `tax (domain)`, `tax (data_area)` |
| A8 | "Outstanding loans by office" | `unsupported` | `off_domain_match` | `loan (domain)` |
| A9 | "Groups with most deposits this month" | `unsupported` | `off_domain_match` | `group_center (domain)`, `group_center_foundation (data_area)` |
| A10 | "Banana republic" | `unsupported` | `vector_no_match` | no high-confidence row |

For each row, the success signal is that `state_json.classification.candidates` contains the expected top non-capability source (and `source_type` is correct). `source` may diverge between `off_domain_match` and `vector_no_match` depending on whether the savings capability scored above 0.40 — the scenario passes if the **outcome** is correct.

## B. Multilingual probe (Bahasa Indonesia synonyms)

Concept synonyms now include Bahasa Indonesia terms. These prompts should reach the same decisions as their English equivalents.

| # | Prompt | Expected `outcome` | Expected `source` | Mapped domain |
| --- | --- | --- | --- | --- |
| B1 | "Berapa total deposit bulan ini" | `matched` | `vector` | `savings` |
| B2 | "Tunjukkan setoran terbesar hari ini" | `matched` | `vector` | `savings` |
| B3 | "Total pinjaman cair bulan ini" | `unsupported` | `off_domain_match` | `loan` (synonym: `pinjaman`, `cair` ≈ disbursement) |
| B4 | "Pembayaran angsuran terbesar bulan ini" | `unsupported` | `off_domain_match` | `loan` (synonyms: `angsuran`, `pembayaran`) |
| B5 | "Tutup buku bulan lalu" | `unsupported` | `off_domain_match` | `accounting` (synonym: `tutup buku`) |
| B6 | "Posting jurnal terbaru" | `unsupported` | `off_domain_match` | `accounting` (synonym: `jurnal`, `posting`) |
| B7 | "Pajak yang dikumpulkan kuartal ini" | `unsupported` | `vector_no_match` | `tax` (synonym: `pajak`) |
| B8 | "Kelompok dengan setoran terbanyak" | `unsupported` | `off_domain_match` | `group_center` (synonym: `kelompok`) |

If B1 or B2 ends up `unsupported`, the embedding did not pick up the Indonesian deposit synonyms — run `POST /vector-index/rebuild` and verify the catalog re-sync included the latest `concepts.synonyms`.

## C. Write-intent guard breadth

The classifier short-circuits on write verbs across any domain before embedding tokens are spent (`JobService::classify_with_retrieval`).

| # | Prompt | Expected `source` |
| --- | --- | --- |
| C1 | "Create a new client account" | `write_intent` |
| C2 | "Open a savings account for John" | `write_intent` |
| C3 | "Add a new office" | `write_intent` |
| C4 | "Disburse a loan today" | depends — if write-intent words (`create`, `open`, `add`, `new`) don't match, falls through to `off_domain_match` |
| C5 | "Post a journal entry" | falls through to `off_domain_match` (no write keyword match yet for `post`) |

C4 + C5 are documented limitations of the current write-intent keyword list — they still end `unsupported` via the off-domain override, just with a different `source` than C1–C3. ponytail: tolerable, upgrade keyword list when a real write request slips through.

## D. PII exposure check (top_n with two API keys)

Two API keys, both with `savings_deposit_top_n` capability and office scope `[1, 2, 3]`:

- `{{API_KEY_PII}}` with `can_view_pii: true`.
- `{{API_KEY_NO_PII}}` with `can_view_pii: false`.

Run the same top-N job from `05-chat-session-and-job.md` Top-N variant with each key.

| Key | Expected `result_json.rows[0].client_display_name` |
| --- | --- |
| `{{API_KEY_PII}}` | populated (e.g. `"John Doe"`) |
| `{{API_KEY_NO_PII}}` | no result rows; job fails policy before execution because selected query output declares `client_display_name` as `pii` |

The catalog declares `client_display_name` with `sensitivity: pii` in `knowledge/queries/savings/deposit_top_n.yaml`. Runtime policy computes PII requirement from selected query output fields, so `can_view_pii=false` keys are blocked before execution.

## E. Catalog-validate breadth assertion

```bash
curl -X POST {{BASE_URL}}/catalog/validate -H "Authorization: Bearer {{API_KEY}}"
```

After the knowledge expansion + Phase 19 savings and foundation slices the response should report:

```json
{ "success": true, "data": { "valid": true, "data_areas": 13, "domains": 7, "capabilities": 11, "queries": 11 }, "error": null }
```

If counts drop, a YAML file is failing to parse — check `RUST_LOG=debug` for the load error.

## F. Withdrawal capability probe (Phase 19 first slice)

API key must include `savings_withdrawal_total` and/or `savings_withdrawal_top_n` in `allowed_capabilities`.

| # | Prompt | Expected `outcome` | Expected `capability` | Notes |
| --- | --- | --- | --- | --- |
| F1 | "What is the total withdrawal this month?" | `matched` | `savings_withdrawal_total` | result has `total_withdrawal_amount` + `withdrawal_count` |
| F2 | "Show the largest withdrawals today" | `matched` | `savings_withdrawal_top_n` | result includes `client_id` + `client_display_name` (LEFT JOIN m_client) |
| F3 | "Berapa total penarikan bulan ini" | `matched` | `savings_withdrawal_total` | ID synonym `penarikan` indexed via `concepts.synonyms` |
| F4 | "Penarikan terbesar bulan ini" | `matched` | `savings_withdrawal_top_n` | ID synonym `penarikan` + `terbesar` |
| F5 | API key without withdrawal scope asks F1 | `unsupported` or `clarification_required` | n/a | capability filter excludes withdrawal; falls back to deposit context |

Formatter templates added in `crates/chat/src/chat/formatter.rs`:
- `savings_withdrawal_total`: "The total savings withdrawal from {from} to {to} is {amount} across {count} withdrawal transaction(s)."
- `savings_withdrawal_top_n`: "Found {n} savings withdrawal transaction(s). The largest amount is {amount} on {date}."

## G. Monthly breakdown capability probe (Phase 19 slice 2)

New output_mode `monthly_breakdown` registered in `OUTPUT_MODES`. SQL groups by `date_trunc('month', transaction_date)`. Result rows are one per month; formatter joins them into a multi-line response.

API key must include `savings_deposit_monthly_breakdown` in `allowed_capabilities`.

After slice 3 (date-range parser upgrade in `classifier.rs::date_range`), the prompts below match directly without a clarification step.

| # | Prompt | Expected `outcome` | Expected `capability` | Notes |
| --- | --- | --- | --- | --- |
| G1 | "Monthly deposit totals for this year" | `matched` | `savings_deposit_monthly_breakdown` | `this year` parser hits → `from=2026-01-01, to=today`. |
| G2 | "Show savings deposits per month from January to September 2026" | `matched` | `savings_deposit_monthly_breakdown` | `month_range` parses "January to September 2026" → 9 rows expected. |
| G3 | "Month-by-month deposit breakdown last 6 months" | `matched` | `savings_deposit_monthly_breakdown` | `last 6 months` → today minus 6 calendar months, up to 7 rows. |
| G4 | "Month-by-month deposit breakdown this month" | `matched` | `savings_deposit_monthly_breakdown` | "this month" parser hits; returns 1 row. |
| G5 | "Berapa setoran tabungan per bulan dari Januari sampai September 2026" | `matched` | `savings_deposit_monthly_breakdown` | `sampai` is treated as separator; `month_range` picks Jan + Sep with explicit year. |
| G6 | "Monthly deposit totals from January to September" (no year) | `matched` | `savings_deposit_monthly_breakdown` | Year defaults to `today.year()` → 2026. |
| G7 | "Show savings deposits in 2025" | `matched` | `savings_deposit_total` | Bare year → full 2025 range, total mode (no "per month"/"breakdown"). |

### Expected result shape

```json
{
  "query_id": "savings.deposit_monthly_breakdown",
  "row_count": 9,
  "rows": [
    { "month_start": "2026-01-01", "total_deposit_amount": "...", "deposit_count": <n> },
    { "month_start": "2026-02-01", "total_deposit_amount": "...", "deposit_count": <n> },
    ...
  ],
  "latency_ms": <n>
}
```

Formatter output (per `crates/chat/src/chat/formatter.rs::format_monthly_breakdown`):

```text
Savings deposit by month (9 month(s)):
- 2026-01-01: 1500000 across 12 transaction(s).
- 2026-02-01: 1800000 across 18 transaction(s).
...
```

Rows beyond 24 months are truncated with `... and N more month(s).` (ponytail: simple cap, lift if real reports request multi-year breakdowns).

### Known limits

- **Date-range parser ceiling** — slice 3 covers `today / yesterday / this month / this week / this year / last year / last month / last week / last N days|weeks|months / bare year / month range (with/without year, EN+ID)`. Free-form natural language outside these forms (e.g. "the quarter ending June") still falls through to clarification. ponytail: upgrade to a full NL date parser only when real prompts demand it.

## H. Monthly top-N capability probe (Phase 19 slice 4)

New output_mode `monthly_top_n` registered in `OUTPUT_MODES`. SQL uses a CTE with `ROW_NUMBER() OVER (PARTITION BY date_trunc('month', transaction_date) ORDER BY amount DESC)` to pick top-N per month. Validator now accepts SQL that starts with `WITH` and bounds results via `ROW_NUMBER()` instead of a trailing `LIMIT`.

API key must include the matching monthly top-N capability in `allowed_capabilities`.

| # | Prompt | Expected `outcome` | Expected `capability` | Notes |
| --- | --- | --- | --- | --- |
| H1 | "Largest deposit for each month from January to September 2026" | `matched` | `savings_deposit_monthly_top_n` | `limit` defaults to 1 (one row per month) — 9 result rows. |
| H2 | "Top 3 deposits per month this year" | `matched` | `savings_deposit_monthly_top_n` | `limit=3`, range = Jan 1 to today. |
| H3 | "Setoran terbesar setiap bulan tahun ini" | `matched` | `savings_deposit_monthly_top_n` | ID synonyms hit; default limit 1. |
| H4 | API key with `can_view_pii=false` runs H1 | `unsupported` | n/a | `planner::evaluate_policy` gates selected query output fields and denies when `client_display_name (pii)` would be exposed without `can_view_pii`. |
| H5 | "Top withdrawals per month this month" | `matched` | `savings_withdrawal_monthly_top_n` | Mirrors deposit monthly top-N with `transaction_type_enum = 2`. |
| H6 | "Monthly withdrawal breakdown this month" | `matched` | `savings_withdrawal_monthly_breakdown` | Mirrors deposit monthly breakdown with withdrawal metrics. |

### Expected result shape

```json
{
  "query_id": "savings.deposit_monthly_top_n",
  "row_count": <n>,
  "rows": [
    {
      "month_start": "2026-01-01",
      "transaction_id": <n>,
      "transaction_date": "2026-01-21",
      "amount": "...",
      "currency_code": "...",
      "office_id": <n>, "office_name": "...",
      "product_id": <n>, "product_name": "...",
      "client_id": <n>, "client_display_name": "..."
    },
    { "month_start": "2026-02-01", ... },
    ...
  ],
  "latency_ms": <n>
}
```

Formatter output (`crates/chat/src/chat/formatter.rs::format_monthly_top_n`):

```text
Top savings deposits per month (9 month(s), 27 transaction(s)):
2026-01-01:
  - 50000000 on 2026-01-21
  - 35000000 on 2026-01-04
  - 22000000 on 2026-01-15
2026-02-01:
  - ...
```

120-row global cap with overflow line `... and N more transaction(s).`

### Side effects
- SQL parses successfully through `validate_runtime` (CTE allowed; `ROW_NUMBER()` recognized as a bounding mechanism instead of trailing `LIMIT`).
- Same audit rows as other capabilities.

## I. Balance summary capability probe (Phase 19 slice 5 — snapshot mode)

New output_mode `summary` (snapshot, no time dimension). SQL aggregates `m_savings_account.account_balance_derived` over active client-owned accounts filtered by office scope. No `from_date`/`to_date` required.

Validator changes for this slice:
- `OUTPUT_MODES` accepts `summary`.
- Approved capability with `output_mode == "summary"` is allowed to declare empty `required_parameters`.

Classifier change: `classify_retrieved_capability` skips `date_range` extraction when `output_mode == "summary"`. Missing dates no longer trigger clarification for these capabilities.

API key must include `savings_balance_summary` in `allowed_capabilities`.

| # | Prompt | Expected `outcome` | Expected `capability` | Notes |
| --- | --- | --- | --- | --- |
| I1 | "What is the total savings balance right now?" | `matched` | `savings_balance_summary` | No date in prompt; no clarification fires. |
| I2 | "Show the savings portfolio summary" | `matched` | `savings_balance_summary` | Pure snapshot. |
| I3 | "Berapa saldo total tabungan aktif saat ini?" | `matched` | `savings_balance_summary` | ID synonyms hit; bypass date parser. |
| I4 | "Current active savings balance in USD" | `matched` | `savings_balance_summary` | Currency filter currently does not flow through from the prompt; SQL `currency_code` param is NULL → all currencies. ponytail: wire currency extraction from prompt when needed. |

### Expected result shape

```json
{
  "query_id": "savings.balance_summary",
  "row_count": 1,
  "rows": [{
    "account_count": <n>,
    "total_balance": "...",
    "average_balance": "...",
    "max_balance": "..."
  }],
  "latency_ms": <n>
}
```

Formatter output:

```text
Active client-owned savings portfolio: 1234 account(s). Total balance 5000000000. Average 4051863. Largest 250000000.
```

### Known scope limits (slice 5)

- **Client-owned only.** Group-owned accounts (`sa.client_id IS NULL AND sa.group_id IS NOT NULL`) are excluded by the inner `JOIN m_client`. Adding group support requires joining `m_group` from `group_center_foundation` (conditional area). Slice can be widened once that area is promoted out of conditional.
- **Active status only.** `WHERE sa.status_enum = 300`. Closed/pending accounts are not counted. Hardcoded for safety; not user-tunable yet.
- **No as-of-date.** Uses live `account_balance_derived`. Historical balance reports require summing transactions up to a target date — separate slice.
- **No currency or product filter from prompt.** Both pass through as `NULL` (= all). User can't yet say "in USD only" via natural language; runtime executor binds whatever the planner extracted. ponytail: extract currency/product hints from the prompt when a real use case appears.

## Side effects (per probe)

Each `POST /chat/jobs` call writes the usual rows. The probes are read-only against the catalog; no Fineract execution happens except for A1/A2/A3/B1/B2 (savings) which run the approved SQL.

## Failure modes

| Trigger | Expected |
| --- | --- |
| Vector index empty (no rebuild after expansion) | All probes fall to lexical fallback; domain candidates absent in `candidates` |
| Voyage API down | Embedding step fails; classifier uses catalog lexical fallback (`source = "catalog_no_match"` or matched capability without context attached) |
| API key has empty `allowed_capabilities` | Every prompt short-circuits to `source = "no_allowed_capabilities"` regardless of probe row |
