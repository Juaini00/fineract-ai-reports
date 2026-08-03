SELECT
    c.id AS client_id,
    c.display_name,
    o.id AS office_id,
    o.name AS office_name,
    COUNT(sa.id) FILTER (WHERE sa.status_enum = 300) OVER (PARTITION BY c.id)::bigint AS active_savings_account_count,
    sa.id AS savings_account_id,
    CASE WHEN sa.account_no IS NULL THEN NULL ELSE CONCAT('****', RIGHT(sa.account_no, 4)) END AS masked_account_number,
    sa.status_enum::bigint AS savings_status_enum,
    sa.currency_code,
    sp.id AS savings_product_id,
    sp.name AS savings_product_name
FROM m_client c
JOIN m_office o ON o.id = c.office_id
LEFT JOIN m_savings_account sa ON sa.client_id = c.id
LEFT JOIN m_savings_product sp ON sp.id = sa.product_id
WHERE c.office_id = ANY($1::bigint[])
  AND c.id = $2::bigint
ORDER BY sa.id;
