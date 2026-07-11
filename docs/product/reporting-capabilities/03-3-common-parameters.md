# Reporting Capabilities: 3. Common Parameters

Source: `docs-old/reporting-capabilities.md`

## 3. Common Parameters

These parameters are shared across the currently implemented savings capabilities and are expected to remain the common shape for planned savings capabilities.

| Parameter | Type | Required | Rule |
| --- | --- | --- | --- |
| `from_date` | `date` | yes | Inclusive business date lower bound. |
| `to_date` | `date` | yes | Inclusive business date upper bound. |
| `office_ids` | `array<bigint>` | no | Must be subset of API key `allowed_office_ids`. If omitted, use all allowed offices. |
| `currency_code` | `string` | no | Optional exact currency filter. |
| `product_ids` | `array<bigint>` | no | Optional savings product filter. |
| `limit` | `integer` | top/list only | Must be bounded by service max limit. |

Default validation:

- `from_date <= to_date`.
- Date range must not exceed the configured maximum range for the capability.
- `office_ids` must not broaden the caller's office scope.
- `limit` must be greater than zero and less than or equal to the configured max limit.
