SELECT
    o.id AS office_id,
    o.name AS office_name,
    COUNT(c.id) FILTER (WHERE c.status_enum = 300)::bigint AS active_clients,
    COUNT(c.id) FILTER (WHERE c.status_enum = 100)::bigint AS pending_clients,
    COUNT(c.id) FILTER (WHERE c.status_enum = 600)::bigint AS closed_clients,
    COUNT(c.id)::bigint AS total_clients
FROM m_office o
LEFT JOIN m_client c ON c.office_id = o.id
WHERE o.id = ANY($1::bigint[])
  AND ($3::text IS NULL OR lower(o.name) = lower($3::text))
GROUP BY o.id, o.name
ORDER BY total_clients DESC, o.id ASC
LIMIT $2;
