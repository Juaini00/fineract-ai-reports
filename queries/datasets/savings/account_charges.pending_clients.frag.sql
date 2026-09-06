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
    due_date,
    CASE
        WHEN due_date IS NULL THEN NULL
        WHEN $2::date > due_date THEN ($2::date - due_date)::bigint
        ELSE 0
    END AS days_overdue
FROM base
WHERE waived = false
  AND is_paid_derived = false
  AND is_active = true
  AND amount_outstanding > 0
ORDER BY amount_outstanding DESC, due_date NULLS LAST, savings_account_charge_id
LIMIT $3
