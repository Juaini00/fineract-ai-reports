# Hardcode Cleanup Issues

Goal: remove runtime hardcodes that can make the reporting system infer, format, block, or describe financial data incorrectly. Test fixtures may use concrete savings examples, but production behavior must be driven by catalog metadata, query contracts, policy metadata, and database result data.

## HC-001: Replace Static Approved SQL Bindings

Severity: Critical
Status: Done

Files:
- `crates/chat/src/chat/executor.rs`

Problem:
- `approved_sql()` matches explicit query ids like `savings.deposit_total` and uses `include_str!` per SQL file.
- Every new loan, tax, accounting, organization, or client capability requires Rust changes.
- Runtime behavior can silently exclude approved catalog queries if the Rust match is not updated.

Acceptance:
- Executor resolves SQL from `QueryKnowledge.sql_file` under the approved `queries/` root.
- Path traversal is rejected.
- Catalog validation proves file existence, SELECT-only safety, placeholders, and output contract before execution.
- No query id list exists in Rust runtime code.

Done:
- Executor now resolves SQL from `QueryKnowledge.sql_file` under `catalog.query_path` and rejects absolute/path-traversal paths.
- Dynamic SQL execution is explicitly audited with `AssertSqlSafe` after catalog/path validation.

## HC-002: Replace Hardcoded Query Parameter Binding

Severity: Critical
Status: Done

Files:
- `crates/chat/src/chat/executor.rs`

Problem:
- Executor supports only `from_date`, `to_date`, `office_ids`, `limit`, `currency_code`, and `product_ids` by string match.
- New query parameters for loans, taxes, accounting, clients, products, staff, branches, statuses, or enums will fail or be omitted.

Acceptance:
- Parameter binding is driven by `QueryKnowledge.parameters` type metadata.
- Supported parameter types are centralized and validated.
- Missing required params fail with sanitized errors.
- Optional params bind null correctly by declared type.

Done:
- Binding now uses declared parameter `type` and `source`, not parameter names like `currency_code` or `product_ids`.
- `source: authorized_scope` binds policy office ids.
- Invalid array element types are rejected instead of silently dropped.

Still pending:
- Move supported parameter type schema into typed catalog validation.

## HC-003: Replace Hardcoded Output Type Decoder

Severity: High
Status: Partially done

Files:
- `crates/chat/src/chat/executor.rs`

Problem:
- Executor decodes only `date`, `decimal`, `integer`, `bigint`, and `string`.
- Other safe database output types cannot be used without Rust changes.

Acceptance:
- Supported output field types are defined in one typed catalog schema.
- Runtime decoder supports the approved type set or rejects unsupported types during catalog validation before execution.

Done:
- Runtime rejects unsupported output field types instead of guessing.

Still pending:
- Move supported output type schema into typed catalog validation and fail before runtime.

## HC-004: Replace PII Policy Heuristic Based On Output Mode

Severity: Critical
Status: Done

Files:
- `crates/chat/src/chat/planner.rs`

Problem:
- PII gate uses `plan.output_mode.ends_with("top_n")`.
- A total, summary, breakdown, or future output mode can expose PII and bypass policy.
- A top-N report without PII can be unnecessarily blocked.

Acceptance:
- PII requirement is computed from selected query `output_fields.sensitivity` and policy metadata.
- API keys without PII access can still run queries whose selected output fields are non-PII.
- If masking is supported later, formatter receives an explicit allowed/masked field set.

Done:
- Policy now computes PII requirement from selected query output field sensitivity instead of `output_mode.ends_with("top_n")`.

## HC-005: Replace Activity/Output-Mode Clarification Heuristic

Severity: High

Files:
- `crates/chat/src/chat/service/job.rs`

Problem:
- `is_activity_request()` detects `activity`, `activities`, `transaction`, `transactions`.
- `activity_options()` chooses output modes using words like `monthly`, `per month`, `breakdown` and hardcoded output mode groups.
- This biases clarification toward current savings transaction shapes and can miss loan/tax/accounting concepts.

Acceptance:
- Clarification options are generated from close catalog candidates and query/capability metadata.
- Output-mode grouping lives in catalog metadata, not string heuristics in Rust.
- Prompt terms do not force a fixed output-mode shortlist unless catalog synonyms support that mapping.

## HC-006: Replace Capability Option Label Derivation From Id Suffixes

Severity: High

Files:
- `crates/chat/src/chat/service/job.rs`

Problem:
- `capability_option_label()` maps `total`, `top_n`, `monthly_breakdown`, `monthly_top_n` to English words and derives subject from capability id suffixes.
- Id names become user-facing semantics.
- New domains or output modes can produce misleading labels.

Acceptance:
- Capability label, metric label, and output label come from catalog fields.
- Fallback labels are generic and clearly non-interpretive.
- Capability ids are never used as business language unless no label exists and the output is marked as technical/debug only.

## HC-007: Replace Period Label Heuristic

Severity: Medium

Files:
- `crates/chat/src/chat/service/job.rs`

Problem:
- `period_label()` maps selected phrases to user-facing text like `this week`, `last month`, `this year`.
- Specific date ranges like two days, custom ranges, or month ranges degrade to `for the requested period`.
- The label can differ from actual extracted `from_date` / `to_date`.

Acceptance:
- Clarification option labels use extracted params (`from_date`, `to_date`) or a canonical `date_range_label` from parameter extraction.
- No user-facing period text is produced from raw keyword heuristics alone.

## HC-008: Replace Off-Domain Override Keyword Rules

Severity: Critical

Files:
- `crates/chat/src/chat/service/job.rs`

Problem:
- `has_off_domain_cue()` uses hardcoded keywords for `loan`, `accounting`, `tax`, and `group`.
- This can incorrectly suppress valid capabilities or fail to suppress unsupported domains.
- The code comment still references “savings capability” as the assumed false-positive case.

Acceptance:
- Off-domain detection is based on catalog domain/data-area statuses and retrieved candidate metadata.
- Domain synonyms and unsupported intents come from catalog.
- No domain-specific keyword list exists in job service.

## HC-009: Replace Write-Intent Guard Keyword Rules

Severity: High

Files:
- `crates/chat/src/chat/service/job.rs`

Problem:
- `is_write_intent()` blocks messages containing `create/open/add/new` plus `account/customer/client`.
- It is English-only and domain-specific.
- It can block read-only reporting questions such as “new accounts opened this month” or miss write intents in other terms/languages.

Acceptance:
- Write/command intent is catalog/policy driven or handled by an explicit intent classifier.
- Reporting-safe phrases like “new accounts opened” are not blocked if an approved read-only capability exists.
- Mutating intents fail by policy before planning/execution.

## HC-010: Replace Static Confidence Thresholds

Severity: High

Files:
- `crates/chat/src/chat/service/job.rs`

Problem:
- Retrieval decisions use fixed values like `0.40`, `0.55`, `0.05`, `0.50`, `0.10`, and `0.38`.
- These thresholds are not calibrated per domain, embedding model, data volume, or query type.
- Low accuracy and excessive clarification/unsupported outcomes can result.

Acceptance:
- Thresholds are configuration or catalog policy values.
- Threshold decisions are logged with candidate scores.
- Domain/capability-specific calibration is possible without code changes.

## HC-011: Replace Lexical Retrieval Scoring Constants

Severity: Medium

Files:
- `crates/chat/src/chat/service/job.rs`

Problem:
- Lexical fallback uses token overlap and hardcoded acceptance threshold.
- It can over-match generic words and under-match domain-specific synonyms.

Acceptance:
- Lexical fallback uses catalog synonyms/keywords and configurable thresholds.
- It remains secondary to vector retrieval and returns transparent candidate evidence.

## HC-012: Replace Date Parser Literal Lists With Metadata-Aware Parameter Extraction

Severity: High

Files:
- `crates/chat/src/chat/classifier.rs`

Problem:
- Date parsing hardcodes English and Indonesian literals for today/week/month/year and month names.
- It does not use query parameter metadata to decide which date fields are required.
- It cannot safely parse all valid business date filters.

Acceptance:
- Parameter extraction reads required/optional params from selected `QueryKnowledge.parameters`.
- Date range extraction returns canonical `from_date`, `to_date`, and `date_range_label`.
- Unsupported date expressions trigger clarification instead of guessed dates.

## HC-013: Replace Top-N Limit Defaults In Classifier

Severity: Medium

Files:
- `crates/chat/src/chat/classifier.rs`

Problem:
- Default limit is `10`, except `monthly_top_n` is `1`.
- This may be wrong for future capabilities or risk policies.

Acceptance:
- Default and maximum limits come from capability/query metadata.
- Missing limit uses catalog default or asks clarification if no safe default exists.

## HC-014: Replace Output Mode Switch In Formatter Runtime

Severity: Medium

Files:
- `crates/chat/src/chat/formatter/mod.rs`

Problem:
- Formatter still switches on output modes: `total`, `summary`, `top_n`, `monthly_breakdown`, `monthly_top_n`.
- It is now contract-driven by fields, but output mode behavior remains hardcoded.

Acceptance:
- Output mode rendering templates come from `knowledge/responses/*.yaml`.
- Unknown output modes use a safe generic table/list renderer.
- Adding a new output mode does not require Rust changes unless it introduces a new renderer type.

## HC-015: Replace Formatter Currency Field Assumption

Severity: High

Files:
- `crates/chat/src/chat/formatter/render.rs`

Problem:
- Decimal fields are prefixed only when a field named `currency_code` exists in the row or params.
- Queries may use different currency field names or return mixed currencies.

Acceptance:
- Currency association is declared in output field metadata, not inferred by field name.
- Aggregates that can mix currencies must group by currency or fail validation.
- Formatter never invents currency and never hides mixed-currency ambiguity.

## HC-016: Replace Formatter PII Skip With Policy-Aware Field Selection

Severity: Critical
Status: Done

Files:
- `crates/chat/src/chat/formatter/render.rs`

Problem:
- Generic formatter always omits `pii` and `secret` fields.
- This is safe but incomplete: authorized users cannot receive approved PII fields, and masking rules are not represented.

Acceptance:
- Formatter receives policy decision or an explicit field visibility decision.
- PII fields are omitted, masked, or shown according to API key and capability policy.
- Field-level decisions are auditable.

Done:
- Formatter receives `PolicyDecision` and includes `pii` fields only when policy allows PII.
- `secret` fields remain hidden.

## HC-017: Replace Catalog Validation SQL Pattern Checks

Severity: High

Files:
- `crates/chat/src/knowledge/catalog/validator.rs`

Problem:
- SQL validation checks hardcoded strings such as `TRANSACTION_DATE`, `BETWEEN`, `LIMIT`, and placeholder casts.
- Valid non-transaction queries can fail, and unsafe queries can pass if they match the strings.

Acceptance:
- Query guards are declared in query/catalog metadata and validated per guard type.
- Date guard validation references the declared date parameter/column relationship.
- Limit/window validation is tied to declared max-row policy.

## HC-018: Replace Static Parameter Type Lists

Severity: Medium

Files:
- `crates/chat/src/knowledge/catalog/validator.rs`

Problem:
- Allowed parameter/output/status values are Rust constants.
- Catalog schema evolution requires code changes.

Acceptance:
- Typed schemas define allowed enum values in one place.
- Schema validation errors identify invalid catalog values before runtime.

## HC-019: Replace Response Text Hardcoded Fallbacks

Severity: Medium

Files:
- `crates/chat/src/chat/formatter/labels.rs`
- `crates/chat/src/chat/formatter/render.rs`
- `crates/chat/src/chat/service/job.rs`
- `crates/chat/src/chat/classifier.rs`

Problem:
- Fallback strings like “No data was found...”, “Report returned...”, “Please choose...”, and unsupported messages are hardcoded.
- Text can drift from response catalog and localization policy.

Acceptance:
- User-facing text comes from response catalog or typed error catalog.
- Missing text keys fail validation or use a safe generic error envelope code, not business prose.

## HC-020: Replace LLM Planner Prompt Contract Hardcodes

Severity: High

Files:
- `crates/chat/src/chat/llm.rs`

Problem:
- Planner fallback prompt and JSON schema are hardcoded around capability selection, clarification, and unsupported outcomes.
- It may not include enough catalog policy/context for broader domains.

Acceptance:
- Prompt is assembled from catalog metadata, response policy, and allowed capabilities.
- LLM output schema remains constrained but is versioned and tested.
- Prompt text is not domain-specific unless sourced from catalog.

## HC-021: Replace Static Retrieval Limits

Severity: Medium

Files:
- `crates/chat/src/chat/service/job.rs`

Problem:
- Runtime retrieval uses fixed limits such as top 3 capabilities and top 5 context rows.
- This affects recall and confidence across larger catalogs.

Acceptance:
- Retrieval limits are config values with safe defaults.
- Limits can be tuned without code changes.

## HC-022: Replace Generic Knowledge Loading For Typed Layers

Severity: High

Files:
- `crates/chat/src/knowledge/model.rs`
- `crates/chat/src/knowledge/catalog/loader.rs`
- `crates/chat/src/knowledge/catalog/validator.rs`

Problem:
- Schema, metrics, policies, and responses are loaded as `GenericKnowledge`.
- Unknown fields and missing required semantics can slip through.

Acceptance:
- Typed Rust schemas exist for response, metric, policy, and schema layers.
- Unknown YAML fields are rejected after schema stabilization.
- Formatter/planner/policy consume typed metadata instead of raw maps.

## HC-023: Replace Test Fixtures That Encode Runtime Assumptions

Severity: Low

Files:
- `crates/chat/src/chat/formatter/mod.rs`
- `crates/chat/src/chat/planner.rs`
- `crates/chat/src/chat/service/job.rs`
- `crates/chat/src/chat/classifier.rs`
- `crates/chat/src/policy/authorization.rs`

Problem:
- Tests still use savings-specific capabilities and fixed dates.
- This is acceptable as fixture data, but can hide accidental production hardcodes.

Acceptance:
- Tests clearly separate fixture ids from runtime logic.
- Add at least one synthetic non-savings catalog fixture for generic formatter/planner paths.
- Tests assert that production code does not branch on specific capability ids.

## HC-024: Replace Static Unsupported/Error Codes Where Needed

Severity: Medium

Files:
- `crates/chat/src/chat/service/job.rs`

Problem:
- Unsupported reasons such as `write_intent`, `no_allowed_capabilities`, `vector_no_match`, `off_domain_match`, and `catalog_no_match` are hardcoded.
- Codes are useful, but they are not typed or documented as an API contract.

Acceptance:
- Error/reason codes are typed constants or enum values.
- Client-facing messages come from error/response catalog.
- Internal reasons remain auditable without leaking implementation detail.

## HC-025: Replace Policy Status Lists With Typed Policy Catalog

Severity: Medium

Files:
- `crates/chat/src/knowledge/catalog/validator.rs`
- `crates/chat/src/chat/service/job.rs`

Problem:
- Status strings such as `approved_mvp`, `candidate`, `deferred`, `rejected`, `deferred_group`, `rejected_group`, and `out_of_scope` are hardcoded.
- Status semantics affect runtime filtering and off-domain behavior.

Acceptance:
- Status values and runtime semantics are typed and documented in one catalog schema.
- Runtime behavior uses typed status semantics, not scattered string matches.

## Resolution Order

1. HC-001, HC-002, HC-004, HC-016: remove highest-risk execution and PII hardcodes.
2. HC-005, HC-006, HC-008, HC-010: remove planner/classification heuristics that reduce accuracy.
3. HC-012, HC-013, HC-017: make parameter extraction and validation metadata-driven.
4. HC-014, HC-015, HC-019, HC-020: finish response/prompt catalog usage.
5. HC-018, HC-022, HC-025: stabilize typed catalog schemas.
6. HC-021, HC-023, HC-024: tuning, tests, and API contract cleanup.
