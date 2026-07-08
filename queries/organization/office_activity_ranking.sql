SELECT
    o.id AS office_id,
    o.name AS office_name,
    COUNT(t.id)::bigint AS transaction_count,
    COALESCE(SUM(CASE WHEN t.transaction_type_enum = 1 THEN t.amount ELSE 0 END), 0)::numeric AS deposit_total,
    COALESCE(SUM(CASE WHEN t.transaction_type_enum = 2 THEN t.amount ELSE 0 END), 0)::numeric AS withdrawal_total
FROM m_office o
JOIN m_savings_account_transaction t ON t.office_id = o.id
WHERE o.id = ANY($1::bigint[])
  AND t.is_reversed = false
  AND t.transaction_date BETWEEN $2::date AND $3::date
GROUP BY o.id, o.name
HAVING COUNT(t.id) > 0
ORDER BY transaction_count DESC, o.id ASC
LIMIT $4;
