SELECT
    c.id AS client_id,
    c.display_name AS client_display_name,
    COUNT(sa.id)::bigint AS savings_account_count
FROM m_client c
LEFT JOIN m_savings_account sa ON sa.client_id = c.id
WHERE c.office_id = ANY($1::bigint[])
GROUP BY c.id, c.display_name
