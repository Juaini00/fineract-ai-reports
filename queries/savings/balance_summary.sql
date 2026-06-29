SELECT
    COUNT(sa.id)::bigint AS account_count,
    COALESCE(SUM(sa.account_balance_derived), 0)::numeric AS total_balance,
    COALESCE(AVG(sa.account_balance_derived), 0)::numeric AS average_balance,
    COALESCE(MAX(sa.account_balance_derived), 0)::numeric AS max_balance
FROM m_savings_account sa
JOIN m_client c ON c.id = sa.client_id
WHERE sa.status_enum = 300
  AND c.office_id = ANY($1::bigint[])
  AND ($2::text IS NULL OR sa.currency_code = $2::text)
  AND ($3::bigint[] IS NULL OR sa.product_id = ANY($3::bigint[]));
