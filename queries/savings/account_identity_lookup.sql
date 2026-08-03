SELECT
    sa.id AS savings_account_id,
    CONCAT('****', RIGHT(sa.account_no, 4)) AS masked_account_number,
    c.id AS client_id,
    c.display_name AS client_display_name,
    o.id AS office_id,
    o.name AS office_name,
    sp.id AS savings_product_id,
    sp.name AS savings_product_name,
    sa.status_enum::bigint AS savings_status_enum,
    sa.currency_code
FROM m_savings_account sa
JOIN m_client c ON c.id = sa.client_id
JOIN m_office o ON o.id = c.office_id
JOIN m_savings_product sp ON sp.id = sa.product_id
WHERE c.office_id = ANY($1::bigint[])
  AND sa.account_no = $2::text
ORDER BY sa.id;
