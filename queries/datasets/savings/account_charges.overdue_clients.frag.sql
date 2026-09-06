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
    ($2::date - due_date)::bigint AS days_overdue
FROM base
WHERE waived = false
  AND is_paid_derived = false
  AND is_active = true
  AND amount_outstanding > 0
  AND due_date < $2::date
ORDER BY due_date ASC, amount_outstanding DESC, savings_account_charge_id
LIMIT $3
