SELECT DISTINCT
    sp.id AS savings_product_id,
    sp.name AS savings_product_name,
    c.id AS client_id,
    sp.currency_code,
    sp.deposit_type_enum::bigint AS deposit_type_enum,
    sp.nominal_annual_interest_rate
FROM m_savings_account sa
JOIN m_client c ON c.id = sa.client_id
JOIN m_savings_product sp ON sp.id = sa.product_id
WHERE c.office_id = ANY($1::bigint[])
  AND c.id = $2::bigint
