SELECT
    t.id AS transaction_id,
    t.transaction_date,
    t.amount,
    sa.currency_code,
    t.office_id,
    o.name AS office_name,
    sa.product_id,
    sp.name AS product_name
FROM m_savings_account_transaction t
JOIN m_savings_account sa ON sa.id = t.savings_account_id
JOIN m_savings_product sp ON sp.id = sa.product_id
JOIN m_office o ON o.id = t.office_id
WHERE t.transaction_type_enum = 1
  AND t.is_reversed = false
  AND t.transaction_date BETWEEN $1::date AND $2::date
  AND t.office_id = ANY($3::bigint[])
  AND ($4::text IS NULL OR sa.currency_code = $4::text)
  AND ($5::bigint[] IS NULL OR sa.product_id = ANY($5::bigint[]))
ORDER BY t.amount DESC, t.transaction_date DESC, t.id DESC
LIMIT $6;
