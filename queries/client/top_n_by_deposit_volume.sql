SELECT
    c.id AS client_id,
    c.display_name AS client_display_name,
    c.office_id,
    o.name AS office_name,
    sa.currency_code,
    COUNT(t.id)::bigint AS deposit_count,
    COALESCE(SUM(t.amount), 0)::numeric AS total_deposit
FROM m_client c
JOIN m_savings_account sa ON sa.client_id = c.id
JOIN m_savings_account_transaction t ON t.savings_account_id = sa.id
JOIN m_office o ON o.id = c.office_id
WHERE c.office_id = ANY($1::bigint[])
  AND t.is_reversed = false
  AND t.transaction_type_enum = 1
  AND t.transaction_date BETWEEN $2::date AND $3::date
  AND ($4::text IS NULL OR sa.currency_code = $4::text)
GROUP BY c.id, c.display_name, c.office_id, o.name, sa.currency_code
HAVING COALESCE(SUM(t.amount), 0) > 0
ORDER BY total_deposit DESC, c.id ASC
LIMIT $5;
