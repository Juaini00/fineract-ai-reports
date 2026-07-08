SELECT
    c.id AS client_id,
    c.display_name AS client_display_name,
    c.office_id,
    o.name AS office_name,
    sa.currency_code,
    COUNT(sa.id)::bigint AS account_count,
    COALESCE(SUM(sa.account_balance_derived), 0)::numeric AS total_balance
FROM m_client c
JOIN m_savings_account sa ON sa.client_id = c.id
JOIN m_office o ON o.id = c.office_id
WHERE c.office_id = ANY($1::bigint[])
  AND sa.status_enum = 300
  AND ($2::text IS NULL OR sa.currency_code = $2::text)
GROUP BY c.id, c.display_name, c.office_id, o.name, sa.currency_code
HAVING COALESCE(SUM(sa.account_balance_derived), 0) > 0
ORDER BY total_balance DESC, c.id ASC
LIMIT $3;
