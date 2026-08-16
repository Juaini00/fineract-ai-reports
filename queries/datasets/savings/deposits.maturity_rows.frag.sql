SELECT
    savings_account_id,
    client_id,
    client_display_name,
    deposit_type_enum,
    currency_code,
    term_deposit_amount,
    maturity_amount,
    maturity_date,
    deposit_period,
    deposit_period_frequency_enum,
    mandatory_recommended_deposit_amount,
    is_mandatory,
    total_overdue_amount,
    no_of_overdue_installments
FROM base
ORDER BY savings_account_id
LIMIT 100
