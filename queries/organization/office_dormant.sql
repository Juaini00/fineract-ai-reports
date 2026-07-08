SELECT
    o.id AS office_id,
    o.name AS office_name,
    o.opening_date,
    MAX(t.transaction_date) AS last_transaction_date,
    COUNT(t.id)::bigint AS transaction_count
FROM m_office o
LEFT JOIN m_savings_account_transaction t
       ON t.office_id = o.id
      AND t.is_reversed = false
      AND t.transaction_date BETWEEN $2::date AND $3::date
WHERE o.id = ANY($1::bigint[])
GROUP BY o.id, o.name, o.opening_date
HAVING COUNT(t.id) = 0
ORDER BY o.opening_date ASC NULLS LAST, o.id ASC
LIMIT $4;
