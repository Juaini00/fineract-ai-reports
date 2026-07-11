# Modern RAG Architecture Blueprint: Step 5 --- Retrieval Planning

Source: `docs-old/Modern_RAG_Architecture_Blueprint.md`

## Step 5 --- Retrieval Planning

Instead of embedding the raw user text, generate retrieval plans.

Example:

Vector Query

    multi legal entity invoice billing

Keyword Query

    AE1 AE2 invoice LSD-6172

Graph Query

    AE1 -> Invoice -> LegalEntity

Metadata Filter

    project=connector
    document=ticket

------------------------------------------------------------------------
