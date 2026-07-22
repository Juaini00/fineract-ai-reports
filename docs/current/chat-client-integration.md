# Chat Client Integration

All `/chat/**` requests require an administrator bearer token. Responses use `{ success, data, error }`; never display an `internal_error` message. `GET /chat/jobs/{job_id}` is the durable recovery source. Redis/SSE is a lossy, non-durable, deduplicated live hint.

## Structured clarification contract (v1)

When `AssistantResponse.response_type === "clarification"`, render its public `clarification` object, not prose or the deprecated top-level `options` projection. The same structured response is available in waiting-job `result_json.structured_response`, SSE `update.payload.structured_response`, and assistant-message `metadata_json.assistant_response`.

```ts
type ClarificationKind = "select_option" | "collect_fields" | "free_text";
type ClarificationFieldType = "date_range" | "integer" | "text";

type ClarificationValidation = {
  min_integer?: number;
  max_integer?: number;
  max_length?: number;
  max_range_days?: number;
};

type ClarificationField = {
  key: string;
  label: string;
  field_type: ClarificationFieldType;
  required: boolean;
  value?: unknown;
  default_value?: unknown;
  help_text?: string;
  validation: ClarificationValidation;
  errors: string[];
};

type ClarificationChoice = {
  id: string;
  label: string;
  description: string | null;
  fields: ClarificationField[];
};

type Clarification = {
  version: 1;
  id: string; // UUID
  revision: number; // u32
  kind: ClarificationKind;
  question: string;
  fields: ClarificationField[];
  options: ClarificationChoice[];
  allow_free_text: boolean;
};

type AssistantResponse = {
  response_type: string;
  clarification?: Clarification;
  options: unknown[]; // deprecated compatibility projection
  message: string;
};
```

`field_type` is the current JSON member name. Field help is `help_text`; choice help is not part of the currently published v1 JSON shape. Treat unknown kinds or field types as an unsupported safe fallback: do not submit, show safe generic copy, and reconcile with `GET job`.

Render with an exhaustive switch. A `select_option` wizard derives its local steps from shared `fields` plus the selected option's `fields`; selecting, back/continue, help/detail expansion, and defaults are local actions. Submit only once at the final step. “Others” is only a report/request escape; help is local detail, not an answer. English-only copy is required.

```ts
function renderClarification(c: Clarification) {
  switch (c.kind) {
    case "select_option": return renderOptionWizard(c, c.fields);
    case "collect_fields": return renderFields(c.fields);
    case "free_text": return renderFreeText(c.allow_free_text);
    default: return renderUnsupportedAndReconcile();
  }
}

function fieldsFor(c: Clarification, optionId?: string) {
  const option = c.options.find(({ id }) => id === optionId);
  return [...c.fields, ...(option?.fields ?? [])];
}

async function submitFinal(jobId: string, c: Clarification, optionId: string | undefined,
                           answers: Record<string, unknown>, message?: string) {
  return apiFetch(`/chat/jobs/${jobId}/responses`, {
    method: "POST",
    body: JSON.stringify({
      clarification_id: c.id,
      clarification_revision: c.revision,
      ...(optionId && { option_id: optionId }),
      answers,
      ...(message && { message }),
    }),
  });
}
```

Historical assistant messages are read-only. Enable controls only when the durable job is `waiting_for_user_input` **and** its active clarification id and revision match the rendered object. On `400 clarification_validation_error`, display only returned safe `fields` errors and leave the job waiting. Handle `409 clarification_stale` by recovery, `409 clarification_not_active` by recovery, and `404` as resource hiding/unavailable.

## Response submission compatibility

`POST /chat/jobs/{job_id}/responses` structured mode accepts `clarification_id`, `clarification_revision`, `option_id`, `answers`, and optional `message`. Legacy mode retains required `message` semantics, with optional `option_id`. A successful response is `201` with the inserted `ChatMessage`, not a job snapshot: immediately fetch `GET /chat/jobs/{job_id}` and branch on durable status.

## Recovery and live updates

On initial job creation, after a response `201`, on reload, and when an SSE stream closes, fetch the job:

- `queued`/`running`: open a header-capable SSE stream;
- `waiting_for_user_input`: render the durable structured response;
- `completed`: refresh session messages and prefer assistant metadata over markdown;
- `failed`: show safe generic failure copy.

SSE has event names `status` and `update`. An `update` clarification carries the exact structured response but is only a dedupe hint; deduplicate idempotently, then reconcile the job. Do not derive durable state from SSE, rely on replay, or put credentials in an EventSource URL.

## UI rules

Disable normal prompt submission while that session's job is queued, running, or waiting. While waiting, permit only matching active clarification controls and disable final submit while it is in flight. Render server rows/metrics/options as received; do not infer a control from markdown, empty arrays, or message text. Sanitize markdown and never expose internal errors, SQL, prompts, or diagnostics.
