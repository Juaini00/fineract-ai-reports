# 003 — Robust verified payload extraction

Status: active — design required before implementation
Severity: blocker
Area: chat | retrieval | planner | clarification | catalog | tests
Created: 2026-07-14
Resolved:

## Problem

The RAG assistant can understand a user prompt semantically while executing a different payload. The current extraction path lets unsupported or missing fields fall back to defaults even when the user's raw message contains hard facts.

Observed failure:

- User prompt: `show 10 clients with the most savings accounts`
- LLM reason: `top 10`
- `constraints.quantity`: `null`
- Tool fallback: `limit = 20`
- Response: `Found 20 rows`

This is a trust boundary bug. The executed payload must be derived from verified evidence, not from best-effort planner state plus tool defaults.

## Impact

- User-visible row counts can contradict the request.
- Prompt interpretation, selected catalog query, and execution parameters are not auditable field-by-field.
- LLM claims can silently influence planning without provenance.
- Required catalog/query params can be omitted and replaced by executor defaults.
- Clarification cannot reliably distinguish missing, conflicting, and untrusted parameters.
- Language and paraphrase support risks becoming a global keyword map instead of catalog-owned semantics.

## Current partial hardening

The working tree has deterministic extraction for several hard facts: quantity/limit, ISO date ranges, currency, domain hints, and selected metric hints. That reduces this bug class but does not solve verified payload construction. The remaining gap is a full extraction model with provenance, conflict handling, catalog validation, and tests that prove the executed payload matches the user's verified intent.

## Expected behavior

The raw user message is the source of truth. Every executable field must be one of:

- deterministically extracted from `user_text` or accepted clarification text;
- selected through a frontend clarification option with a valid `option_id`;
- semantically supported by catalog/retrieval evidence;
- supplied by an approved catalog default whose provenance is explicit.

LLM output may propose structured claims, but claims are untrusted until supported by user text, clarification state, retrieval context, or catalog defaults. Unsupported LLM hard facts must not enter execution.

Before SQL execution, the assistant must build a verified execution payload and reject or clarify when any required field is missing, untrusted, or conflicting.

## Required design outcomes

- Raw message and accepted clarification state remain durable audit inputs.
- Deterministic hard-fact extraction covers quantities, dates, currencies, entity names/ids, domain terms, metric terms, sort direction, grouping, and filters needed by approved queries.
- Semantic retrieval uses enriched catalog concepts and aliases, including language/paraphrase handling, without global word maps.
- LLM structured claims are stored separately from verified facts.
- Every candidate field has provenance: source, evidence text/span or catalog id, confidence, trust level, and conflict set.
- Catalog/query metadata declares required params, optional params, defaults, allowed values, type validators, semantic concepts, aliases, and clarification options.
- Validation happens before execution and fails closed.
- Clarification is generated for missing, untrusted, or conflicting params.
- Audit trail records extraction candidates, winning values, rejected values, conflicts, clarification decisions, and final verified payload.
- Scenario tests cover semantic correctness, not only Rust type behavior.

## Clarification API contract

- Clarification options must include stable `option_id` values.
- For non-`other` option replies, clients must send `option_id`.
- For `other` replies, clients must send `message`.
- The server must reject ambiguous replies that include neither a usable `option_id` nor an `other` message.
- Clarification continues the same job via `POST /chat/jobs/{job_id}/responses`.

## Acceptance gates

- The failing `show 10 clients with the most savings accounts` scenario executes with limit `10` or asks clarification; it never silently returns 20 rows.
- No executable payload field lacks provenance.
- Missing required params trigger clarification before execution.
- Conflicting values trigger clarification before execution.
- Unsupported LLM hard facts are rejected unless grounded by user text, clarification, retrieval, or catalog default.
- Semantic claims require catalog/retrieval agreement before selecting a capability or param value.
- Language/paraphrase cases are resolved through catalog concepts and aliases.
- Catalog/query required-param validation is covered by unit and scenario tests.
- Audit output can explain why every final payload field was trusted.

## Non-goals

- Do not let the LLM generate SQL.
- Do not add global multilingual word maps outside the catalog concept/alias layer.
- Do not rely on executor fallback defaults for required user intent.
- Do not post-filter SQL results in Rust to compensate for bad extraction.
- Do not redesign the whole assistant graph beyond the extraction, validation, clarification, and audit path needed here.
