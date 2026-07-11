# Modern RAG Architecture Blueprint: Step 10 --- Answer Generation

Source: `docs-old/Modern_RAG_Architecture_Blueprint.md`

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
7.  **Every production request must be auditable.** The backend records
    durable, structured audit events for the stages it executes and for
    blueprint stages it intentionally skips. Audit writes must be
    event-driven and non-blocking. The detailed design is in
    `docs/audit-trail-design.md`.

------------------------------------------------------------------------

# Section 11 — Response Format Standard

Response format is the contract between the reasoning pipeline and every
consumer (frontend, API client, Postman, integration test). It is
deliberately *both* structured JSON and rendered markdown — never one
without the other — so tests assert against fields while humans read
prose.
