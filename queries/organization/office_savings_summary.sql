SELECT
    o.id AS office_id,
    o.name AS office_name,
    sa.currency_code,
    COUNT(sa.id) FILTER (WHERE sa.status_enum = 300)::bigint AS active_account_count,
    COUNT(sa.id)::bigint AS total_account_count,
    COALESCE(SUM(sa.account_balance_derived) FILTER (WHERE sa.status_enum = 300), 0)::numeric AS total_balance
FROM m_office o
LEFT JOIN m_savings_account sa ON sa.office_id = o.id
WHERE o.id = ANY($1::bigint[])
  AND ($2::text IS NULL OR sa.currency_code = $2::text)
GROUP BY o.id, o.name, sa.currency_code
HAVING COUNT(sa.id) > 0
ORDER BY total_balance DESC, o.id ASC
LIMIT $3;
