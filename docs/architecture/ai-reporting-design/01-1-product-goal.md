# AI Reporting Service Design: 1. Product Goal

Source: `docs-old/ai-reporting-design.md`

## 1. Product Goal

Build an AI-assisted reporting service that allows users to ask natural-language questions such as:

```text
Who made the largest savings deposit today?
What is the total deposit amount from January to September 2026?
Show the largest deposit for each month from January to September.
```

The system should understand the request, determine whether the report is supported, execute only approved read-only queries, and return a clear answer.
