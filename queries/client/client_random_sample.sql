SELECT
    c.id AS client_id,
    c.display_name AS client_display_name,
    c.office_id,
    o.name AS office_name,
    c.activation_date
FROM m_client c
JOIN m_office o ON o.id = c.office_id
WHERE c.office_id = ANY($1::bigint[])
  AND c.status_enum = 300
ORDER BY random()
LIMIT $2;
