WITH source AS (
SELECT
    t.id AS savings_transaction_id,
    sa.id AS savings_account_id,
    c.id AS client_id,
    c.display_name AS client_display_name,
    t.transaction_type_enum::bigint AS transaction_type_enum,
    t.transaction_date,
    t.amount,
    t.running_balance_derived AS running_balance
FROM m_savings_account_transaction t
JOIN m_savings_account sa ON sa.id = t.savings_account_id
JOIN m_client c ON c.id = sa.client_id
WHERE t.is_reversed = false
  AND t.office_id = ANY($1::bigint[])
),
base AS (
  SELECT *
  FROM source
  WHERE TRUE
  AND ($2::bigint IS NULL OR client_id = $2::bigint)
)
SELECT
    savings_transaction_id,
    savings_account_id,
    client_id,
    client_display_name,
    transaction_type_enum,
    transaction_date,
    amount,
    running_balance
FROM base
ORDER BY transaction_date DESC, savings_transaction_id DESC
LIMIT 100
