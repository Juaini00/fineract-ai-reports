# Modern RAG Architecture Blueprint: Knowledge Operation

Source: `docs-old/Modern_RAG_Architecture_Blueprint.md`

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
