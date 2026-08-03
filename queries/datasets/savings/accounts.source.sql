SELECT
    sa.id AS savings_account_id,
    sa.account_no AS account_no,
    CONCAT('****', RIGHT(sa.account_no, 4)) AS masked_account_number,
    c.id AS client_id,
    c.display_name AS client_display_name,
    o.id AS office_id,
    o.name AS office_name,
    sp.id AS savings_product_id,
    sp.name AS savings_product_name,
    sa.status_enum::bigint AS savings_status_enum,
    sa.currency_code,
    sa.nominal_annual_interest_rate AS account_nominal_annual_interest_rate,
    sp.nominal_annual_interest_rate AS product_nominal_annual_interest_rate,
    sa.allow_overdraft AS account_allow_overdraft,
    sp.allow_overdraft AS product_allow_overdraft,
    sa.overdraft_limit AS account_overdraft_limit,
    sp.overdraft_limit AS product_overdraft_limit
FROM m_savings_account sa
JOIN m_client c ON c.id = sa.client_id
JOIN m_office o ON o.id = c.office_id
JOIN m_savings_product sp ON sp.id = sa.product_id
WHERE c.office_id = ANY($1::bigint[])
