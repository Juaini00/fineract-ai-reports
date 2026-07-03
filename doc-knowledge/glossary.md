---
type: Glossary
title: Business & Domain Glossary
description: Fineract / microfinance vocabulary used across capabilities, policies, and reporting. Read this before drafting a new capability or reviewing a query.
tags: [glossary, fineract, microfinance]
---

# Glossary

The assistant answers reporting questions over an [Apache Fineract](https://fineract.apache.org/) core banking database. Fineract's schema uses `m_*` prefixes for master/business tables and a few conventions that recur below.

---

## Organizational structure

- **Office** (`m_office`) — a branch, sub-branch, or head office. Forms a tree via `parent_id`. **Everything reportable is scoped to one or more offices** — this is the primary tenancy axis.
- **Staff** (`m_staff`) — a person employed at an office. Loan officers, tellers. Not currently returned in reports; identity classified `pii`.
- **Allowed office scope** — the subset of offices an API key may read (`api_keys.allowed_office_ids`). Enforced *inside* SQL via `ANY($office_ids::bigint[])`, never as post-fetch Rust filter. See [policies/office-scope](./policies/office-scope.md).

## People

- **Client** (`m_client`) — a natural person or legal entity (via `m_client_non_person`) that owns accounts. In microfinance context these are typically low-income individuals or small-group members.
- **Group** (`m_group`) — a collection of clients managed together, common in Grameen-style group lending. Optional; only enable [group-center-foundation](./data-areas/group-center-foundation.md) when the deployment uses it.
- **Center** — a higher-level grouping that contains multiple groups. Same table (`m_group`) with `level_id`.
- **Account owner** — the client OR group that owns a savings/loan account. Queries must not assume the owner is a client.

## Products & accounts

- **Savings product** (`m_savings_product`) — the *template* (interest rate, minimum balance, allowed charges) from which savings accounts are instantiated.
- **Savings account** (`m_savings_account`) — one client's or group's actual account. Has a lifecycle: `submitted → approved → active → closed`. **Active** = `status_enum=300`.
- **Derived balance** — `account_balance_derived` and `available_balance_derived` on `m_savings_account` are Fineract's cached running balances. These are the **snapshot** used by [savings.account_balance](./metrics/savings.account_balance.md); they are **not** date-bounded and cannot answer "balance as of last week".

## Transactions

- **Savings transaction** (`m_savings_account_transaction`) — every money movement on a savings account. Key columns:
  - `amount` — always positive.
  - `transaction_type_enum` — **1 = deposit** (money in), **2 = withdrawal** (money out). Other values (interest posting, fee charge, transfer, etc.) exist but MVP capabilities only touch 1 and 2.
  - `is_reversed` — a *soft-delete* flag. Reversed transactions represent operator corrections; **excluded by default** to avoid double-counting. Setting a filter is a business rule, not a bug.
  - `transaction_date` — business date; the field date-range parameters bind to.
  - `office_id` — copied onto the transaction row for reporting fast-path; the office scope filter binds here.
- **Reversal** — a compensating transaction that flips `is_reversed=true` on the original. Fineract never physically deletes.
- **Payment detail** (`m_payment_detail`) — bank name, check number, receipt reference. Conditional; contains `sensitive_business_identifier` fields (`account_number`, `check_number`, `receipt_number`, `bank_number`) — never returned in MVP output.

## Currency & products

- **Currency code** — ISO 4217 (`IDR`, `USD`). A Fineract deployment may run multi-currency; capabilities accept `currency_code` as an optional filter.
- **Product filter** — `product_ids` narrows to specific savings products.

## Time semantics

- **Snapshot** — a "right now" answer (e.g. [balance_summary](./capabilities/savings.balance_summary.md)). No date range required.
- **Date-bounded** — a "between X and Y" answer (e.g. deposit totals). Requires `from_date` and `to_date`. Bounded by `max_date_range_days=366`.
- **Monthly breakdown / top-N per month** — date-bounded with an additional `GROUP BY month_start` (breakdown) or `PARTITION BY month ORDER BY amount DESC LIMIT N` (top-n).

## Money & data

- **Aggregate** — SUM/COUNT/AVG output; carries no PII risk on its own.
- **Row-level** — individual transaction or account rows; may carry conditional PII (client identity).
- **PII** — Personally Identifiable Information. In this codebase: `client_display_name`, `client_id` (row-level identity), and `full_name`, `mobile_no`, `email_address`, `date_of_birth` (currently never returned). Governed by [policies/pii](./policies/pii.md).
- **Secret-never-expose** — `account_no`, `external_id`, `ref_no`, `payment_detail_id` — never fetched, returned, logged, or sent to the AI. Not the same as PII.

## AI & catalog

- **Capability** — a named, approved unit of work the assistant may execute (e.g. `savings_deposit_total`). Not "any SQL Fineract can run". See [capabilities/](./capabilities/index.md).
- **Query** — the reviewed SQL file bound 1:1 to a capability. AI never writes SQL at runtime. See [queries/](./queries/index.md).
- **Metric** — a named business measurement (e.g. `savings.deposit_amount`). Capabilities declare which metrics they output; queries implement them.
- **Data area** — a bounded slice of the Fineract schema (e.g. [savings-transactions](./data-areas/savings-transactions.md)). Tables outside the current area whitelist can't be referenced by an approved query.
- **Domain** — a business area (`savings`, `loan`, `client`, ...). Domains have status `approved_mvp | candidate | deferred`. Deferred domains hard-reject.
- **Approved MVP** — the capability/metric passes the runtime `catalog/validate` checks and is enabled at startup. Everything else is `candidate` or `deferred`.
- **Authorized scope** — the set of `allowed_office_ids` on the API key; SQL parameters with `source: authorized_scope` are bound from here, never from the user.
- **Output mode** — how the assistant frames the answer: `summary | total | top_n | monthly_breakdown | monthly_top_n`. Each mode has a template in [responses/reporting](./responses/reporting.md).

## Auth

- **API key** — the credential the caller presents (`Authorization: Bearer …` or `X-API-Key: …`). DB stores `key_hash` + `key_prefix` only. Raw key returned exactly once at creation.
- **Bootstrap admin token** — a separate static secret (`AUTH_BOOTSTRAP_ADMIN_TOKEN`) used only to create new API keys via `POST /auth/api-keys`. Not usable for reporting requests.
- **can_view_pii** — a per-API-key flag; necessary but not sufficient to return identity fields (the selected capability must also allow each PII field).

## Chat & jobs

- **Chat session** — a persistent conversation (`chat_sessions`). One per user context.
- **Chat job** — a single reporting request within a session (`chat_jobs`). Auto-created with the first user message.
- **Clarification** — the job's response asks a follow-up (e.g. "which date range?"). Answer via `POST /chat/jobs/{job_id}/responses` — **the same job**, never a new one.
- **Checkpoint** — durable state on `chat_job_checkpoints`. Written at meaningful boundaries; not on every heartbeat.
- **Live state** — ephemeral SSE coordination in Redis (`chat_job:{id}:live_state`). Never source of truth.
