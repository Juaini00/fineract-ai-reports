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
