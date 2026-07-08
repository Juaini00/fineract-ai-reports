SELECT
    c.id AS client_id,
    c.display_name AS client_display_name,
    c.office_id,
    o.name AS office_name,
    COUNT(sa.id)::bigint AS account_count
FROM m_client c
JOIN m_savings_account sa ON sa.client_id = c.id
JOIN m_office o ON o.id = c.office_id
WHERE c.office_id = ANY($1::bigint[])
  AND sa.status_enum = 300
GROUP BY c.id, c.display_name, c.office_id, o.name
HAVING COUNT(sa.id) > 0
ORDER BY account_count DESC, c.id ASC
LIMIT $2;
