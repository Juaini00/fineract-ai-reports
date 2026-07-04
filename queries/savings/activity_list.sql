SELECT
    t.id AS transaction_id,
    t.transaction_date,
    CASE t.transaction_type_enum
        WHEN 1 THEN 'deposit'
        WHEN 2 THEN 'withdrawal'
        WHEN 3 THEN 'interest_posting'
        WHEN 4 THEN 'withdrawal_fee'
        WHEN 5 THEN 'annual_fee'
        WHEN 8 THEN 'dividend_payout'
        WHEN 17 THEN 'withhold_tax'
        WHEN 19 THEN 'escheat'
        WHEN 20 THEN 'amount_hold'
        WHEN 21 THEN 'amount_release'
        ELSE 'other'
    END AS transaction_type,
    t.amount,
    sa.currency_code,
    t.office_id,
    o.name AS office_name,
    sa.product_id,
    sp.name AS product_name,
    sa.client_id,
    c.display_name AS client_display_name
FROM m_savings_account_transaction t
JOIN m_savings_account sa ON sa.id = t.savings_account_id
JOIN m_savings_product sp ON sp.id = sa.product_id
JOIN m_office o ON o.id = t.office_id
LEFT JOIN m_client c ON c.id = sa.client_id
WHERE t.is_reversed = false
  AND t.transaction_date BETWEEN $1::date AND $2::date
  AND t.office_id = ANY($3::bigint[])
  AND ($4::text IS NULL OR sa.currency_code = $4::text)
  AND ($5::bigint[] IS NULL OR sa.product_id = ANY($5::bigint[]))
ORDER BY t.transaction_date DESC, t.id DESC
LIMIT $6;
