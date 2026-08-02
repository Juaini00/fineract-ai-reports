SELECT
    sa.id AS savings_account_id,
    c.display_name AS client_display_name,
    sp.name AS product_name,
    COUNT(*) OVER (PARTITION BY sa.id)::bigint AS transaction_count,
    FIRST_VALUE(t.amount) OVER (
        PARTITION BY sa.id
        ORDER BY t.transaction_date DESC, t.id DESC
    ) AS latest_transaction_amount,
    ROW_NUMBER() OVER (
        PARTITION BY sa.id
        ORDER BY t.transaction_date DESC, t.id DESC
    ) AS latest_rank
FROM m_savings_account_transaction t
JOIN m_savings_account sa ON sa.id = t.savings_account_id
JOIN m_savings_product sp ON sp.id = sa.product_id
JOIN m_client c ON c.id = sa.client_id
WHERE t.is_reversed = false
  AND t.office_id = ANY($1::bigint[])
