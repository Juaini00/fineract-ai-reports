# Bundle 14 — W-F + W-N: Frontend contract docs + cross-repo link

**Goal:** Publish, in the client-facing contract doc, the exact `answers.<field>` value
shape per clarification `field_type` (with a worked request/response pair each), make the
`clarification_validation_error` `details.fields` shape precise and documented, and record
in issue 007 the outstanding cross-repo action (open + link an `ai_report_dashboard`
issue at E5). Docs only — no code, no migrations, no YAML.

**Architecture:** Documentation change only. The clarification contract is already
partially documented in `docs/current/chat-client-integration.md` §"Structured
clarification contract (v1)" (line 9) and §"Same-job clarification" (line 249). This
bundle expands that existing section with the per-`field_type` value shapes and worked
examples. It does **not** touch `docs/current/management-dashboard-integration.md`.

**Global constraints (do not violate):**
- Docs only. No changes under `crates/`, `queries/`, `knowledge/`, `migrations/`.
- English-only copy.
- Document the contract that the code **actually enforces today** — every shape below was
  read out of `crates/chat/src/job/service/clarification_response.rs::validate_field`
  and `crates/chat/src/assistant/context/clarification.rs`. Do not document aspirational
  behaviour.
- Response-shape items W-N #2–#5 (wide-table, multi-currency cards, truncation warning,
  money formatting) depend on Bundle 9 (W-G/W-J). They are **out of scope here** — see the
  gated note in Task 4. Do not invent those payload shapes now.

## Current state (verified 2026-07-27)

Read against the working tree, not the issue text.

**Drift vs issue 007 (F1 / W-N "Files likely touched"):** The issue says publish the
`field_type` shapes in `docs/current/management-dashboard-integration.md`. That file is
verified to be **entirely about the `GET /management/dashboard` endpoint** — it contains
zero clarification content and is the wrong home. The live clarification contract is in
`docs/current/chat-client-integration.md`. This plan targets that file. (Open decision —
see return summary.)

**`field_type` enum (`crates/chat/src/assistant/context/clarification.rs:19-25`):**
`ClarificationFieldType` = `DateRange` | `Integer` | `Text`, serialized snake_case as
`date_range`, `integer`, `text`.

**Per-`field_type` value shape actually enforced by `validate_field`
(`crates/chat/src/job/service/clarification_response.rs:137-248`):**

- `date_range` → JSON **object** with **exactly two** string keys `from` and `to`, each an
  ISO `YYYY-MM-DD` date (parsed with `%Y-%m-%d`). Rejected if: not an object; key count ≠ 2;
  either key missing or non-string; either date unparseable; `from > to`; or span exceeds
  `validation.max_range_days` when that metadata is present. It is **not** a string — this
  is the E5 mismatch.
- `integer` → JSON **number** coercible to i64. Rejected if not an integer, below
  `validation.min_integer`, or above `validation.max_integer`. For the field keyed `limit`,
  a valid value patches `LimitMode::TopN` + `LimitValue`.
- `text` → non-empty JSON **string** (trimmed). Rejected if not a string, empty after trim,
  or longer than `validation.max_length` characters. (Text answers are validated but not yet
  mapped to constraints — `TODO(issue-003)` in code.)

**`ClarificationValidation` metadata surfaced to clients
(`clarification.rs:27-37`):** `min_integer`, `max_integer`, `max_length`, `max_range_days`
(all optional).

**`details.fields` shape (verified):** `ClarificationValidationError.fields` is a
`Vec<String>` (`clarification_response.rs:16-27`); the handler wraps it as
`details: { "fields": [ ... ] }` with code `clarification_validation_error`, HTTP 400,
message "Clarification response is invalid."
(`crates/chat/src/api/handlers/job.rs:221-227`). Each string is a **dotted path** to the
offending input. Observed values from `validate_submission`/`validate_field`:
`"message"`, `"option_id"`, `"clarification_id"`, `"clarification_revision"`,
`"answers"` (an answer key not offered by the payload), and `"answers.<field-key>"` (a
specific field failed type/range validation). It is a flat list of paths, **not** a
per-field object with reason codes — precise enough to highlight the offending control by
path, but it carries no machine-readable reason. Document exactly this; do not claim
reason codes exist.

**Success response:** a valid `POST /chat/jobs/{job_id}/responses` returns `201` whose
`data` is the inserted clarification `ChatMessage` (`id`, `session_id`, `job_id`, `role`,
`metadata_json`, `content`, `created_at`), **not** a job result — already documented at
`chat-client-integration.md:298`. Worked examples must reflect this, not a fabricated job
payload.

**Issue 007 W-N already recorded** (roadmap progress log, Bundle 1 DONE): 007 documents
backend-independent resolution and E5-tracked-to-dashboard. The **outstanding** W-N item is
the cross-repo user action: open the `ai_report_dashboard` issue and link its id at E5. As
of today no id is linked (issue 007 lines 132-145, 986-1044 carry a placeholder, no id).

---

## Task 1 — Publish per-`field_type` value shapes in the clarification contract

**File:** `docs/current/chat-client-integration.md`

Insert a new subsection immediately after the existing §"Structured clarification contract
(v1)" paragraph (currently ends at line ~17, before `## Endpoints`/next `##`). Use the
exact content below.

- [ ] Add the subsection heading and per-`field_type` value-shape table.

```markdown
### Clarification answer value shapes per `field_type`

`answers` is a JSON object keyed by the field's `key`. The value shape is fixed by the
field's `field_type`. The server validates each value and rejects the whole submission on
the first offending field. Optional `validation` metadata on the field constrains the
value further.

| `field_type` | `answers.<key>` value | Constraints (from `validation`) |
| --- | --- | --- |
| `date_range` | object `{ "from": "YYYY-MM-DD", "to": "YYYY-MM-DD" }` — **not** a string | exactly the two keys `from` and `to`; ISO `YYYY-MM-DD`; `from <= to`; span `<= max_range_days` when present |
| `integer` | JSON number (integer) | `>= min_integer` and `<= max_integer` when present |
| `text` | non-empty JSON string | length `<= max_length` characters when present |

A `date_range` value must be an object with exactly the keys `from` and `to`. A plain
string, a missing key, an extra key, a non-ISO date, or `from` after `to` is rejected with
`400 clarification_validation_error` and `details.fields: ["answers.<key>"]`.
```

- [ ] **Verify** the documented shape still matches the code:

```bash
grep -n 'object.len() != 2\|"%Y-%m-%d"\|get("from")\|get("to")' \
  crates/chat/src/job/service/clarification_response.rs
```

Expected output includes the `object.len() != 2` guard, both `%Y-%m-%d` parses, and the
`from`/`to` lookups — confirming the `date_range` object-with-two-keys shape is current.

## Task 2 — Add one worked request/response pair per `field_type`

**File:** `docs/current/chat-client-integration.md`

- [ ] Directly below the table from Task 1, add the worked pairs. Use exactly this content.

```markdown
#### Worked request/response pairs

Each pair is a `POST /chat/jobs/{job_id}/responses` body and the resulting response. A
valid submission returns `201` whose `data` is the inserted clarification `ChatMessage`
(see §"Same-job clarification"); it is not a job result. Fetch `GET /chat/jobs/{job_id}`
afterwards to read durable job status.

**`date_range`** — capability offered a `date_range` field keyed `date_range`:

Request:

```json
{
  "clarification_id": "3f2b1c00-0000-4000-8000-000000000001",
  "clarification_revision": 1,
  "answers": { "date_range": { "from": "2026-07-01", "to": "2026-07-31" } }
}
```

Response `201`:

```json
{
  "success": true,
  "data": {
    "id": "9a7c...",
    "session_id": "…",
    "job_id": "…",
    "role": "user",
    "content": "…",
    "metadata_json": { },
    "created_at": "2026-07-27T10:00:00Z"
  },
  "error": null
}
```

**`integer`** — capability offered an `integer` field keyed `limit`:

Request:

```json
{
  "clarification_id": "3f2b1c00-0000-4000-8000-000000000002",
  "clarification_revision": 1,
  "answers": { "limit": 5 }
}
```

Response `201`: same inserted-`ChatMessage` envelope as above.

**`text`** — capability offered a `text` field keyed `note`:

Request:

```json
{
  "clarification_id": "3f2b1c00-0000-4000-8000-000000000003",
  "clarification_revision": 1,
  "answers": { "note": "Head office only" }
}
```

Response `201`: same inserted-`ChatMessage` envelope as above.
```

- [ ] **Verify** the field keys used in the examples are real (`date_range`, `limit` map to
  live constraints; `note` is illustrative text):

```bash
grep -n '"limit"\|"date_range"\|ConstraintField::FromDate\|LimitMode::TopN' \
  crates/chat/src/job/service/clarification_response.rs
```

Expected: hits on the `limit` key patch (`LimitMode::TopN`) and the `date_range`
`FromDate`/`ToDate` patch — confirming the two constraint-bearing keys in the examples.

## Task 3 — Document the `clarification_validation_error` `details.fields` shape

**File:** `docs/current/chat-client-integration.md`

The existing text (line ~17 and line ~298) mentions the error exists but never specifies
`details.fields`. Add a precise spec.

- [ ] Below the worked pairs from Task 2, add:

```markdown
#### `clarification_validation_error` details

An invalid submission returns:

```json
{
  "success": false,
  "data": null,
  "error": {
    "code": "clarification_validation_error",
    "message": "Clarification response is invalid.",
    "details": { "fields": ["answers.date_range"] }
  }
}
```

`details.fields` is a flat array of dotted paths identifying the offending input(s), so a
client can highlight the exact control. Possible values:

| Path | Meaning |
| --- | --- |
| `answers.<key>` | that field's value failed its `field_type`/`validation` check (or a required field was omitted with no default) |
| `answers` | an `answers` key was sent that the clarification did not offer |
| `option_id` | option missing, unknown, unauthorized, or sent on a non-`select_option` clarification |
| `message` | required free text missing (free-text mode, or `option_id: "others"`) |
| `clarification_id` | structured submission missing the id |
| `clarification_revision` | structured submission missing the revision |

The array carries paths only, not machine-readable reason codes. Show only these safe
paths to the user; never surface raw server error text. Stale/inactive clarifications
return `409 clarification_stale` / `409 clarification_not_active` instead — reconcile with
`GET /chat/jobs/{id}`.
```

- [ ] **Verify** the documented paths match the code:

```bash
grep -n 'ClarificationValidationError::field(' \
  crates/chat/src/job/service/clarification_response.rs
```

Expected: entries for `"message"`, `"clarification_id"`, `"clarification_revision"`,
`"option_id"`, `"answers"`, and `format!("answers.{}", field.key)` — the exact set of
paths documented above.

- [ ] **Verify** the handler envelope (code + message + details key) is unchanged:

```bash
grep -n 'clarification_validation_error\|Clarification response is invalid\|"fields": fields' \
  crates/chat/src/api/handlers/job.rs
```

Expected: the code string, the message, and the `{ "fields": fields }` details wrapper.

## Task 4 — Gated note for response-shape items (finalize after W-G / Bundle 9)

**File:** `docs/current/chat-client-integration.md`

W-N items #2–#5 (wide-table rendering, multi-currency cards, truncation indicator, money
formatting) depend on the presentation/money work in Bundle 9 (W-G + W-J). Their payload
shapes do not exist in the code yet, so do not invent them. Record the dependency only.

- [ ] Add a short forward-reference note at the end of the clarification section:

```markdown
> **Analyst-grade response shapes (pending).** Wide-table columns/rows, per-currency
> subtotal cards, the `result_truncated` warning, and payload-carried money formatting are
> defined by the presentation/money work (issue 007 W-G/W-J). This document will publish
> their exact shapes once that work lands; until then, `table.columns`/`table.rows` remain
> the authoritative tabular surface and `rendered_markdown` is a capped fallback.
```

- [ ] Do **not** add example payloads for these items in this bundle. (Ponytail: no
  speculative shapes; add when Bundle 9 ships them.)

## Task 5 — Record the outstanding cross-repo action in issue 007

**File:** `docs/issues/active/007-analyst-grade-knowledge-and-request-mapping.md`

Bundle 1 already recorded W-N's backend-independent resolution. The remaining item is the
**user action** to open and link the dashboard issue. Make that explicit and leave a link
placeholder at E5.

- [ ] At E5 (line ~144, after "Tracked here for completeness…"), append:

```markdown
> **Cross-repo action (user, not backend):** Open a matching issue in the
> `ai_report_dashboard` repository for the `{from,to}` `date_range` control (submit the
> object shape, not a string; see `docs/current/chat-client-integration.md`
> §"Clarification answer value shapes per `field_type`"). Link its identifier here:
> `ai_report_dashboard#TBD`. E5 stays open until that issue is linked and the picker lands.
```

- [ ] In the W-N §"Acceptance" (line ~1038), leave the first criterion but annotate its
  status. Replace the bullet:

```markdown
- A corresponding issue exists in the `ai_report_dashboard` repository and is linked from
  E5 by identifier.
```

with:

```markdown
- A corresponding issue exists in the `ai_report_dashboard` repository and is linked from
  E5 by identifier. **Status: pending user action** — backend docs (W-F1/F2) are published
  in `docs/current/chat-client-integration.md`; the dashboard issue id is not yet linked
  (`ai_report_dashboard#TBD` at E5).
```

- [ ] **Verify** the placeholder and doc reference are present:

```bash
grep -n 'ai_report_dashboard#TBD\|Clarification answer value shapes' \
  docs/issues/active/007-analyst-grade-knowledge-and-request-mapping.md \
  docs/current/chat-client-integration.md
```

Expected: the `#TBD` placeholder appears twice in issue 007 (E5 + acceptance) and the
doc-section reference resolves in `chat-client-integration.md`.

## Final verification

- [ ] Confirm no code/YAML/migration files were touched:

```bash
git status --porcelain -- crates queries knowledge migrations
```

Expected: **no output** (this bundle changes only the two docs).

- [ ] Confirm both docs changed:

```bash
git status --porcelain -- docs/current/chat-client-integration.md \
  docs/issues/active/007-analyst-grade-knowledge-and-request-mapping.md
```

Expected: both listed as modified (` M`).

## Out of scope

- The date-range picker itself and any dashboard code — lives in `ai_report_dashboard`.
- Response-shape items W-N #2–#5 — deferred to Bundle 9 (W-G/W-J); Task 4 records the
  dependency only.
- Any change to `docs/current/management-dashboard-integration.md` — that doc is the
  `/management/dashboard` contract and is unrelated to clarification (see drift note).
- Backend behaviour changes: the clarification contract is documented as-is, not modified.
