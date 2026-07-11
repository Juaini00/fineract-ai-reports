# AI Reporting Service Design: 5. Knowledge Model

Source: `docs-old/ai-reporting-design.md`

## 5. Knowledge Model

The system uses four knowledge layers.

```text
Domain Knowledge
  -> Capability Knowledge
  -> Query Knowledge
  -> Schema Knowledge
```

### 5.1 Domain Knowledge

Domain knowledge describes a business area, not database tables.

Examples:

```text
savings
loan
accounting
tax
```

Domain knowledge contains:

1. Domain description.
2. Core business concepts.
3. Synonyms.
4. Behavioral descriptions.
5. Supported intents.
6. Unsupported intents.
7. Default business rules.

Example meaning mappings:

```text
deposit = money in = savings credit from customer
interest = bunga = automatic balance increase from interest posting
withdrawal = penarikan = money out
```

### 5.2 Capability Knowledge

Capability knowledge describes reports/actions that the system actually supports.

Examples:

```text
savings_deposit_total
savings_deposit_top_n
savings_deposit_monthly_breakdown
savings_deposit_monthly_top_n
loan_repayment_total
```

If a user request does not match an approved capability, the system must return an unsupported or clarification response. It must not generate a new SQL query at runtime.

### 5.3 Query Knowledge

Query knowledge describes the approved query behind a capability.

It contains:

1. SQL file path.
2. Required parameters.
3. Optional parameters.
4. Allowed output fields.
5. Output contract.
6. Guard rules.
7. Timeout.
8. Cost class.

### 5.4 Schema Knowledge

Schema knowledge is a summarized explanation of important Fineract tables, columns, and relationships.

It is not the main runtime source for user reporting. It is mainly used for:

1. Developer mode.
2. Debugging.
3. Query review.
4. AI-assisted capability creation.
5. Documentation.
