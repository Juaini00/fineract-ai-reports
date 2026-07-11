# RAG Architecture: 1. What "RAG" Means In This System

Source: `docs-old/rag-architecture.md`

## 1. What "RAG" Means In This System

This service uses **constrained RAG**. Retrieval does not generate answers or SQL. It only helps the planner pick from a fixed, human-approved set of reporting capabilities.

Standard RAG vs. this system:

| Aspect | Standard RAG | This system |
| --- | --- | --- |
| What is retrieved | Free-form document chunks | Structured knowledge entries (data area, domain, capability, query, schema, metric) |
| What the LLM does with retrieval | Synthesize a free-form answer | Pick a `capability_id`; the answer comes from approved SQL output |
| Source of executable logic | Whatever the LLM writes | Pre-reviewed SQL files in `queries/` |
| Authority of vector search | Drives the answer | Only ranks candidates; Rust policy guard decides |
| Failure mode if retrieval is wrong | Hallucinated answer | `unsupported` or `clarify` response |

The principle: **vector search finds relevant knowledge, never executes a decision**. Authorization, capability selection, and SQL execution stay in Rust.
