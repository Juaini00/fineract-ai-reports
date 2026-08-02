SELECT
    client_id,
    client_display_name,
    office_id,
    office_name,
    savings_account_id,
    savings_account_charge_id,
    charge_definition_id,
    charge_name,
    is_penalty,
    charge_timing_enum,
    currency_code,
    currency_digits,
    currency_display_symbol,
    amount_due_current,
    amount_paid,
    amount_waived,
    amount_written_off,
    amount_levied_total,
    amount_outstanding,
    due_date
FROM base
ORDER BY created_on_utc DESC, savings_account_charge_id DESC
LIMIT $2
