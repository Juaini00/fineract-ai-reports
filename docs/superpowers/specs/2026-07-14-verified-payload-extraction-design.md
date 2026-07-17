# Verified payload extraction design

Date: 2026-07-14
Status: draft for implementation
Issue: `docs/issues/active/003-verified-payload-extraction.md`

## Goal

Build a verified extraction pipeline for the RAG assistant so the executed catalog payload is grounded in raw user text, accepted clarification, catalog metadata, and retrieval evidence. The LLM may assist, but it cannot be the authority for executable facts.

## Source of truth

Priority order:

1. Raw `chat_messages` user text for the current turn.
2. Accepted clarification responses for the same job.
3. Bounded session context selected for the current job.
4. Catalog/query metadata and approved defaults.
5. LLM structured claims, only as untrusted candidates until grounded by 1-4.

Raw messages remain immutable audit records. Clarification updates append evidence; they do not rewrite the original prompt.

## Field model

Represent every extracted value as a candidate field:

```text
field_key          catalog/query param or planner field
value              typed value
source             user_text | clarification_option | clarification_text | session_context | catalog_default | retrieval | llm_claim
evidence           text span, option_id, catalog id, retrieval chunk id, or default id
confidence         numeric score from extractor/retrieval/LLM
trust              trusted | untrusted | rejected
conflicts          ids of incompatible candidates
reason             short machine-readable explanation
```

The final execution payload stores only trusted winners. Rejected and losing candidates remain in the audit trail.

## Extraction stages

1. **Deterministic hard facts**
   - Extract quantities/limits, ISO dates and common date phrases once supported, currencies, entity names/ids, domains, metrics, sort directions, groupings, and query filters.
   - Use raw text spans where possible.
   - Treat these as trusted when parsing is exact enough for the target type.

2. **Catalog semantic matching**
   - Enrich catalog entries with concepts, aliases, paraphrases, language variants, metric aliases, entity aliases, and filter aliases.
   - Retrieval returns candidate capabilities, params, and concepts with catalog ids.
   - Language handling belongs in these catalog-owned aliases/concepts, not global keyword maps.

3. **LLM structured claims**
   - Ask the LLM for structured claims only: intended domain, metric, filters, sort, quantity, time range, requested output shape, and uncertainty.
   - Store claims as untrusted candidates.
   - Promote a claim only when it is supported by deterministic evidence, accepted clarification, session context, retrieval agreement, or approved catalog default.

4. **Conflict detection**
   - Detect incompatible values for the same field, incompatible capability choices, and unsupported hard facts.
   - Examples: `top 10` vs `limit 20`, savings metric vs loan capability, two date ranges, or option reply that contradicts free text.

5. **Catalog/query validation**
   - Query metadata declares required params, types, allowed values, defaults, and clarification config.
   - Required params without trusted values block execution.
   - Defaults may fill optional params only when marked safe and recorded as `catalog_default`.

6. **Verified payload build**
   - Produce a single typed payload for the selected approved query.
   - Execution accepts only this payload.
   - Executor fallbacks are allowed only for technical limits that do not alter user intent and must be explicit defaults in metadata.

## Clarification behavior

Clarify when:

- a required param is missing;
- a candidate is untrusted;
- candidates conflict;
- retrieval cannot agree with the selected catalog concept;
- LLM proposes a hard fact not grounded by trusted evidence.

Clarification response contract:

- Option replies require `option_id` for every non-`other` choice.
- `other` replies require `message`.
- Ambiguous replies fail validation with a sanitized `ApiError`.
- The same job continues through `POST /chat/jobs/{job_id}/responses`.

## Audit trail

Persist enough job event/checkpoint data to answer:

- What did the user ask?
- Which candidates were extracted?
- Which evidence supported each candidate?
- Which candidates were rejected and why?
- Which conflicts blocked execution?
- Which clarification resolved the field?
- What exact verified payload reached the executor?

## Testing requirements

- Unit tests for deterministic field extraction.
- Catalog validation tests for required params, defaults, aliases, and allowed values.
- Conflict tests for quantity, metric/capability mismatch, date range mismatch, and clarification contradictions.
- Scenario/golden tests proving final payload semantics for paraphrases and language variants.
- Regression test for `show 10 clients with the most savings accounts`.
- Negative tests showing unsupported LLM hard facts are not executed.

## Non-goals

- No LLM-generated SQL.
- No global word maps outside catalog concepts and aliases.
- No Rust post-filtering to repair wrong SQL payloads.
- No broad assistant graph rewrite beyond this verified extraction path.
