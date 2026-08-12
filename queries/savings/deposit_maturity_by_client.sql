WITH source AS (
SELECT
    sa.id AS savings_account_id,
    c.id AS client_id,
    c.display_name AS client_display_name,
    sa.deposit_type_enum::bigint AS deposit_type_enum,
    sa.currency_code,
    t.deposit_amount AS term_deposit_amount,
    t.maturity_amount,
    t.maturity_date,
    t.deposit_period,
    t.deposit_period_frequency_enum::bigint AS deposit_period_frequency_enum,
    r.mandatory_recommended_deposit_amount,
    r.is_mandatory,
    r.total_overdue_amount,
    r.no_of_overdue_installments
FROM m_savings_account sa
JOIN m_client c ON c.id = sa.client_id
LEFT JOIN m_deposit_account_term_and_preclosure t ON t.savings_account_id = sa.id
LEFT JOIN m_deposit_account_recurring_detail r ON r.savings_account_id = sa.id
WHERE sa.deposit_type_enum IN (200, 300)
  AND c.office_id = ANY($1::bigint[])
),
base AS (
  SELECT *
  FROM source
  WHERE TRUE
  AND ($2::bigint IS NULL OR client_id = $2::bigint)
)
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
