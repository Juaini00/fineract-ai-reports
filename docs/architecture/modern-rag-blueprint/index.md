# Modern RAG Architecture Blueprint

Source: `docs-old/Modern_RAG_Architecture_Blueprint.md`

This is the split, readable version of the original document. Content was migrated section-by-section so no old context is dropped.

## Original introduction


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

## Sections

- [Knowledge Operation](./01-knowledge-operation.md)
- [Step 1 --- Semantic Parsing](./02-step-1-semantic-parsing.md)
- [Step 2 --- Intent Routing](./03-step-2-intent-routing.md)
- [Step 3 --- Entity Resolution](./04-step-3-entity-resolution.md)
- [Step 4 --- Ambiguity Detection](./05-step-4-ambiguity-detection.md)
- [Step 5 --- Retrieval Planning](./06-step-5-retrieval-planning.md)
- [Step 6 --- Hybrid Retrieval](./07-step-6-hybrid-retrieval.md)
- [Step 7 --- Reranking](./08-step-7-reranking.md)
- [Step 8 --- Evidence Evaluation](./09-step-8-evidence-evaluation.md)
- [Step 9 --- Answer Planning](./10-step-9-answer-planning.md)
- [Step 10 --- Answer Generation](./11-step-10-answer-generation.md)
- [11.0 Foundational rule — do not hardcode Fineract-owned data](./12-11-0-foundational-rule-do-not-hardcode-fineract-owned-data.md)
- [11.1 Envelope](./13-11-1-envelope.md)
- [11.2 Currency rules (concrete example)](./14-11-2-currency-rules-concrete-example.md)
- [11.3 Coverage transparency](./15-11-3-coverage-transparency.md)
- [11.4 Semantic bucket taxonomy (loaded from YAML, not hardcoded)](./16-11-4-semantic-bucket-taxonomy-loaded-from-yaml-not-hardcoded.md)
- [11.5 Number formatting](./17-11-5-number-formatting.md)
- [11.6 Time bucket rules](./18-11-6-time-bucket-rules.md)
- [11.7 Message header (always present)](./19-11-7-message-header-always-present.md)
- [11.8 Machine-readability contract](./20-11-8-machine-readability-contract.md)
- [11.9 Empty result](./21-11-9-empty-result.md)
- [11.10 Test contract summary](./22-11-10-test-contract-summary.md)
