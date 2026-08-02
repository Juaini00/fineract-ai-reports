SELECT
    sa.id AS savings_account_id,
    COUNT(*)::bigint AS transaction_count
FROM m_savings_account_transaction t
JOIN m_savings_account sa ON sa.id = t.savings_account_id
JOIN m_savings_product sp ON sp.id = sa.product_id
JOIN m_client c ON c.id = sa.client_id
WHERE t.is_reversed = false
  AND t.office_id = ANY($1::bigint[])
  AND lower(c.display_name) = lower($2::text)
  AND lower(sp.name) = lower($3::text)
GROUP BY sa.id
HAVING (
    ARRAY_AGG(t.amount ORDER BY t.transaction_date DESC, t.id DESC)
)[1] = $4::text::numeric
ORDER BY sa.id;
