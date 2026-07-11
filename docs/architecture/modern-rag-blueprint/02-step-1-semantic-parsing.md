# Modern RAG Architecture Blueprint: Step 1 --- Semantic Parsing

Source: `docs-old/Modern_RAG_Architecture_Blueprint.md`

## Step 1 --- Semantic Parsing

Input:

    "Kenapa ticket AE1 harus support AE2?"

Output:

``` json
{
  "intent":"EXPLANATION",
  "entities":["AE1","AE2"],
  "domain":"invoice",
  "requires_retrieval":true,
  "confidence":0.91
}
```

No retrieval yet.

------------------------------------------------------------------------
