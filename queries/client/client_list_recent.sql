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
  AND c.activation_date IS NOT NULL
  AND ($3::text IS NULL OR lower(o.name) = lower($3::text))
ORDER BY c.activation_date DESC, c.id DESC
LIMIT $2;
