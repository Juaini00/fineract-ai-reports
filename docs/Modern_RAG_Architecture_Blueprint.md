# Modern RAG Architecture Blueprint

> Goal: Build a production-grade RAG where the **system orchestrates**
> and the **LLM reasons**, instead of letting a single LLM call decide
> everything.

------------------------------------------------------------------------

# High-Level Architecture

``` text
User
 │
 ▼
Conversation Context
 │
 ▼
Semantic Parser (LLM Structured Output)
 │
 ▼
Intent Router (Deterministic Rules)
 │
 ▼
Entity & Constraint Resolver
 │
 ▼
Ambiguity Detector
 │
 ▼
Retrieval Planner
 │
 ├── Vector Search
 ├── Keyword/BM25
 ├── Graph Search
 └── Metadata Filter
 │
 ▼
Hybrid Retrieval
 │
 ▼
Reranker
 │
 ▼
Evidence Evaluator
 │
 ▼
Answer Planner
 │
 ▼
LLM Answer Generator
 │
 ▼
Grounded Response
```

------------------------------------------------------------------------


## Knowledge Operation
                          User
                            │
                            ▼
                ┌────────────────────┐
                │ Conversation State │
                └────────────────────┘
                            │
                            ▼
                 ┌────────────────────┐
                 │ Semantic Parser    │  (LLM)
                 └────────────────────┘
                            │
                            ▼
                ┌─────────────────────┐
                │ Intent Engine       │
                └─────────────────────┘
                            │
             ┌──────────────┼──────────────┐
             ▼              ▼              ▼
     Knowledge Query    Tool Action    Clarification
             │              │              │
             └──────────────┼──────────────┘
                            ▼
                  Context Builder
                            │
                            ▼
                  Retrieval Planner
                            │
       ┌────────────┬──────────────┬────────────┐
       ▼            ▼              ▼            ▼
    Vector       Keyword         Graph      Metadata
       └────────────┴──────────────┴────────────┘
                            │
                            ▼
                     Hybrid Retrieval
                            │
                            ▼
                        Reranker
                            │
                            ▼
                  Evidence Evaluator
                            │
                            ▼
                    Reasoning Planner
                            │
                            ▼
                      Answer Planner
                            │
                            ▼
                     Response Generator

---

# Responsibilities

  ----------------------------------------------------------------------------
  Component         Main Responsibility               Technology
  ----------------- --------------------------------- ------------------------
  Semantic Parser   Convert natural language into     LLM (JSON output)
                    structured intent/entities        

  Intent Router     Select workflow                   Rules / State Machine

  Entity Resolver   Resolve project, module, ticket,  Rules + Knowledge Base
                    user, environment                 

  Ambiguity         Detect missing/conflicting        Rules + Confidence
  Detector          information                       

  Retrieval Planner Generate retrieval strategies and LLM + Templates
                    queries                           

  Hybrid Retrieval  Fetch evidence                    Vector DB + BM25 + Graph

  Reranker          Rank evidence                     Cross-Encoder/Reranker
                                                      Model

  Evidence          Check evidence quality and        Rules + LLM (optional)
  Evaluator         coverage                          

  Answer Planner    Build response structure          LLM

  Answer Generator  Produce grounded answer           LLM
  ----------------------------------------------------------------------------

------------------------------------------------------------------------

# Pipeline

## Step 1 --- Semantic Parsing

Input:

    "Kenapa ticket AE1 harus support AE2?"

Output:

``` json
{
  "intent":"EXPLANATION",
  "entities":["AE1","AE2"],
  "domain":"invoice",
  "requires_retrieval":true,
  "confidence":0.91
}
```

No retrieval yet.

------------------------------------------------------------------------

## Step 2 --- Intent Routing

Example:

  Intent                 Route
  ---------------------- -----------------------
  FACT_LOOKUP            Retrieval
  TROUBLESHOOTING        Error workflow
  DECISION_SUPPORT       Comparison workflow
  IMPLEMENTATION_GUIDE   Architecture workflow
  ACTION_REQUEST         Tool/MCP workflow

------------------------------------------------------------------------

## Step 3 --- Entity Resolution

Resolve identifiers against internal knowledge.

Example:

    AE1
     → Legal Entity

    AE2
     → Billing Entity

    Invoice
     → Connector Project

------------------------------------------------------------------------

## Step 4 --- Ambiguity Detection

Questions evaluated:

-   Missing entities?
-   Low confidence?
-   Multiple possible interpretations?
-   Conflicting constraints?

If ambiguity is high:

    Return clarification request

Otherwise continue.

------------------------------------------------------------------------

## Step 5 --- Retrieval Planning

Instead of embedding the raw user text, generate retrieval plans.

Example:

Vector Query

    multi legal entity invoice billing

Keyword Query

    AE1 AE2 invoice LSD-6172

Graph Query

    AE1 -> Invoice -> LegalEntity

Metadata Filter

    project=connector
    document=ticket

------------------------------------------------------------------------

## Step 6 --- Hybrid Retrieval

    Vector DB
          +
    BM25
          +
    Graph
          +
    Metadata

Merge results.

------------------------------------------------------------------------

## Step 7 --- Reranking

Example score

    Final Score

    =
    0.45 Semantic
    +
    0.35 Keyword
    +
    0.15 Metadata
    +
    0.05 Freshness

Return Top-K evidence.

------------------------------------------------------------------------

## Step 8 --- Evidence Evaluation

Check:

-   Enough evidence?
-   Contradictions?
-   Missing required sources?

If weak:

    Retry Retrieval

------------------------------------------------------------------------

## Step 9 --- Answer Planning

Example

``` json
{
  "sections":[
    "Problem",
    "Business Context",
    "Technical Reason",
    "Recommendation"
  ]
}
```

------------------------------------------------------------------------

## Step 10 --- Answer Generation

The LLM receives only:

-   User request
-   Parsed intent
-   Entities
-   Retrieved evidence
-   Planned response

The model should **not invent unsupported facts**.

------------------------------------------------------------------------

# Controller State Machine

``` text
Receive Request
        │
        ▼
Semantic Parse
        │
        ▼
Intent Valid?
   │          │
  No         Yes
   │          │
Clarify   Plan Retrieval
              │
              ▼
Retrieve Evidence
              │
              ▼
Evidence Enough?
      │             │
     No            Yes
      │             │
Retry/Search     Plan Answer
                      │
                      ▼
Generate Response
                      │
                      ▼
Evaluate
                      │
                      ▼
Return
```

------------------------------------------------------------------------

# Design Principles

1.  LLM is a reasoning component, not the workflow controller.
2.  The backend owns routing, retries, confidence, and orchestration.
3.  Retrieval uses multiple strategies (vector + keyword + graph +
    metadata).
4.  Evidence quality is validated before answer generation.
5.  Every stage produces structured outputs that are testable and
    debuggable.
6.  **The system of record owns domain vocabulary.** For this project
    the system of record is Apache Fineract. We never hardcode
    currency codes, transaction-type enums, product identifiers,
    office identifiers, charge codes, payment-type ids, client
    statuses, group statuses, or any other value whose canonical list
    is maintained inside Fineract. Fineract can add or rename any of
    these tomorrow — our code must survive that without a redeploy.

------------------------------------------------------------------------

# Section 11 — Response Format Standard

Response format is the contract between the reasoning pipeline and every
consumer (frontend, API client, Postman, integration test). It is
deliberately *both* structured JSON and rendered markdown — never one
without the other — so tests assert against fields while humans read
prose.

## 11.0 Foundational rule — do not hardcode Fineract-owned data

Anything whose canonical list lives inside Apache Fineract flows through
query results at runtime. This includes at minimum:

  Category                            Fineract source
  ----------------------------------- --------------------------------------
  Currency code + fraction digits     `m_organisation_currency`, per-txn `currency_code`
  Transaction type semantics          `transaction_type_enum` on each txn table
  Savings/loan product identifiers    `m_savings_product`, `m_loan_product`
  Office identifiers and names        `m_office`
  Client / group / staff identifiers  `m_client`, `m_group`, `m_staff`
  Charge codes                        `m_charge`
  Payment types                       `m_payment_type`
  Client / loan / savings statuses    respective `status_enum` columns

**Rules that follow from this:**

- Formatter code must not contain a Rust `match` on currency codes,
  product ids, or office names. If a value is missing, render as-is
  (with a `null`-safe fallback) — never guess.
- Any mapping table we *do* need (e.g. `transaction_type_enum → semantic
  bucket`) lives under `knowledge/domain/` YAML and is loaded at
  startup. Adding a new enum value = YAML change, not code change +
  redeploy.
- Currency arithmetic uses the currency-code string that comes back on
  each row. Rows in different currencies are grouped separately. **Never
  sum across currencies.**

## 11.1 Envelope

Every reporting response returns:

``` json
{
  "answer_plan": {
    "capability": "savings_activity_list",
    "sections": ["overview", "deposits", "withdrawals", "charges_paid", "interest_and_dividends", "holds", "other", "weekly_aggregation", "period_aggregation"],
    "coverage": {
      "requested_range": { "from": "2026-05-05", "to": "2026-07-05" },
      "returned_rows": 10,
      "limit_applied": 10,
      "truncated": true,
      "known_total_rows": null,
      "currencies_returned": ["USD", "AED"],
      "offices_returned": [1]
    }
  },
  "structured": {
    "by_currency": {
      "USD": {
        "deposits":               { "count": 0, "total": "0.00" },
        "withdrawals":            { "count": 0, "total": "0.00" },
        "charges_paid":           { "count": 2, "total": "7.12" },
        "interest_and_dividends": { "count": 0, "total": "0.00" },
        "holds":                  { "count": 0, "total": "0.00" },
        "other":                  { "count": 6, "total": "0.85" }
      },
      "AED": {
        "charges_paid": { "count": 1, "total": "0.09" },
        "other":        { "count": 1, "total": "0.12" }
      }
    },
    "rows": [
      { "transaction_id": 123, "transaction_date": "2026-07-05", "transaction_type_raw": "withdrawal_fee", "semantic_bucket": "charges_paid", "amount": "7.03", "currency_code": "USD", "product_id": 4, "product_name": "Saving Product - USD", "office_id": 1, "office_name": "Head Office" }
    ],
    "weekly_aggregation": [
      { "week_start": "2026-06-29", "week_end": "2026-07-05",
        "by_currency": { "USD": { "charges_paid": "7.12", "other": "0.85" },
                         "AED": { "charges_paid": "0.09", "other": "0.12" } } }
    ],
    "period_aggregation": {
      "bucket_size_days": 2,
      "reason": "range 61 days → 2-day buckets",
      "buckets": [
        { "start": "2026-05-05", "end": "2026-05-06",
          "by_currency": { "USD": { "no_activity": true } } }
      ]
    }
  },
  "message": "…rendered markdown…"
}
```

Consumers pick what they need. Frontend can render `message` directly.
Integration tests assert against `structured.by_currency.USD.deposits.count`,
never grep the markdown.

## 11.2 Currency rules (concrete example)

Given the response mixes `USD` and `AED`:

**WRONG — current output.** Header total sums USD and AED as if they
were the same unit:

    ### Charges paid (3 row(s), total: USD 7.210000)
    1. 2026-07-05 — USD 7.030000 (Saving Product - USD, office: Head Office)
    2. 2026-07-05 — AED 0.090000 (Current Account With OD - AED, office: Head Office)
    3. 2026-07-05 — USD 0.090000 (Current Account USD, office: Head Office)

**RIGHT — required output.** Sub-sections per currency, no cross-currency
math anywhere:

    ### Charges paid

    #### USD (2 transactions, total 7.12)
    1. 2026-07-05 — 7.03 (Saving Product - USD, office: Head Office)
    2. 2026-07-05 — 0.09 (Current Account USD, office: Head Office)

    #### AED (1 transaction, total 0.09)
    1. 2026-07-05 — 0.09 (Current Account With OD - AED, office: Head Office)

Formatter never invents a currency, never normalises to a "primary"
currency, never picks one for the header. If a row has no
`currency_code` (Fineract shouldn't allow this but we assume nothing),
render as `unknown` — do not drop.

The same rule applies to product name, office name, and every other
Fineract-owned label: pass through as-is, no substitution, no
inference.

## 11.3 Coverage transparency

Silent truncation is a bug. If any of the following are true, the
`coverage` block AND the message header MUST say so explicitly:

- `LIMIT` was applied (`returned_rows == limit_applied`).
- Requested office scope was narrowed by policy (`policy_decision`
  changed `office_ids`).
- A currency filter was applied.
- A product filter was applied.

Required header sentence when truncated:

> "Showing 10 most recent transactions. Result limited by `limit=10`;
> narrow the date range or raise the limit to see more."

`known_total_rows` is `null` unless the capability defines a cheap
COUNT query. When present, it becomes:

> "Showing 10 of 1,601 transactions in this range."

## 11.4 Semantic bucket taxonomy (loaded from YAML, not hardcoded)

Config: `knowledge/domain/savings-transaction-types.yaml`

``` yaml
id: savings_transaction_type_enum
source_doc: Apache Fineract m_savings_account_transaction.transaction_type_enum
buckets:
  deposits:
    enums: [1]
    label: "Deposits"
  withdrawals:
    enums: [2]
    label: "Withdrawals"
  charges_paid:
    enums: [4, 5, 17]
    label: "Charges paid"
  interest_and_dividends:
    enums: [3, 8]
    label: "Interest and dividends"
  holds:
    enums: [20, 21]
    label: "Holds"
fallback:
  id: other
  label: "Other activity"
```

- Executor SQL keeps returning `transaction_type_enum` int + a
  best-effort label; the formatter does the bucketing at render time
  by consulting this YAML.
- New Fineract enum value → add to YAML, no code change, no redeploy.
- `other` is the fallback for enums not listed. It is NOT a place we
  dump things we know but couldn't be bothered to label.

## 11.5 Number formatting

- Decimal amounts render with the fraction digits Fineract associates
  with the currency (via `m_organisation_currency.decimal_places` —
  fetched, not hardcoded). If unavailable, default to 2.
- Trim trailing zeros only after the currency's minimum fraction digits.
  So USD `7.030000` → `7.03`; JPY `100.00` → `100` if JPY.decimal_places
  is 0.
- Thousands separator: comma. `1,234.56`.
- Never render `-0.00`; collapse to `0`.
- Percentages: 1 decimal + `%`.

## 11.6 Time bucket rules

- Aggregation ranges follow the **requested** date range, not the
  returned rows. Empty buckets render as "no activity". Users need to
  see the shape of quiet periods, not just the loud ones.
- Adaptive bucket size for period aggregation:

  Range span            Bucket size
  --------------------- -----------
  ≤ 14 days             daily
  15–60 days            2-day
  61–180 days           weekly
  181–730 days          monthly
  > 730 days            quarterly

  The chosen size + the reason ("range 61 days → 2-day buckets") goes
  into `structured.period_aggregation.reason`.
- Weekly aggregation is *in addition to* period aggregation when the
  range spans multiple ISO weeks. Weekly always starts Monday.

## 11.7 Message header (always present)

First sentence of every non-empty reporting response, generated
deterministically from `answer_plan` (no LLM required):

> "Savings activity from **2026-05-05** to **2026-07-05**, across
> **1 office** (Head Office), in currencies **USD**, **AED**.
> Showing **10** transactions (truncated by `limit=10`)."

Fields:

- Range: from `coverage.requested_range`.
- Office count + names: from `coverage.offices_returned` joined against
  the rows' `office_name` (never a hardcoded lookup).
- Currency list: from `coverage.currencies_returned`, sorted
  alphabetically.
- Truncation note: only when `truncated == true`.

## 11.8 Machine-readability contract

Consequence of Design Principle 5.

- Integration tests MUST assert against `structured.by_currency.<CODE>.<bucket>.count`
  and `structured.coverage.truncated`, never grep the markdown message.
- Renaming a section header ("Deposits" → "Setoran") must not break
  any integration test.
- Message markdown IS a human artefact and may reword freely between
  versions; the JSON structured payload is a versioned contract.

## 11.9 Empty result

When `rows.len() == 0` after policy filtering:

``` json
{
  "answer_plan": { ..., "coverage": { "returned_rows": 0, ... } },
  "structured": { "by_currency": {}, "rows": [], "weekly_aggregation": [], "period_aggregation": { "buckets": [] } },
  "message": "No savings activity from 2026-05-05 to 2026-07-05 in your authorised offices."
}
```

No fake sections. No "Deposits (0 rows)" cluttering the output.

## 11.10 Test contract summary

- Unit tests own: bucket mapping YAML round-trip, decimal rendering,
  header sentence generation.
- Integration tests own: end-to-end payload shape, coverage
  truncation flag, per-currency grouping, empty result behaviour.
- Neither layer greps the message string for domain vocabulary owned
  by Fineract.
