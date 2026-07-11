# Reporting PII Policy: 8. Behavior By API Key

Source: `docs-old/reporting-pii-policy.md`

## 8. Behavior By API Key

### 8.1 `can_view_pii=false`

Allowed:

- Aggregate totals.
- Counts.
- Office/product/currency dimensions.
- Non-identifying numeric ids only if declared by the capability.
- Masked identity fields only if declared by the capability.

Not allowed:

- Client names.
- Staff names.
- App user names.
- Email addresses.
- Phone numbers.
- Account numbers.
- External ids.
- Payment references.
- Raw free text.

### 8.2 `can_view_pii=true`

Allowed only when declared by the selected capability:

- Client display names.
- Staff display names.
- App user display names.
- Selected row-level identifying fields.

Still not allowed:

- Passwords.
- Tokens.
- Temporary credentials.
- Raw command JSON.
- Raw command results.
- Idempotency keys.
- Unapproved payment references.
- Unapproved account/external ids.
