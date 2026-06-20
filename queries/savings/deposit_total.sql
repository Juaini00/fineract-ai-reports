SELECT
    $1::date AS from_date,
    $2::date AS to_date,
    COALESCE(SUM(t.amount), 0) AS total_deposit_amount,
    COUNT(t.id) AS deposit_count
FROM m_savings_account_transaction t
JOIN m_savings_account sa ON sa.id = t.savings_account_id
JOIN m_office o ON o.id = t.office_id
WHERE t.transaction_type_enum = 1
  AND t.is_reversed = false
  AND t.transaction_date BETWEEN $1::date AND $2::date
  AND t.office_id = ANY($3::bigint[])
  AND ($4::text IS NULL OR sa.currency_code = $4::text)
  AND ($5::bigint[] IS NULL OR sa.product_id = ANY($5::bigint[]));
