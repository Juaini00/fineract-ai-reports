SELECT
    COUNT(c.id)::bigint AS client_count,
    COUNT(c.id) FILTER (WHERE c.status_enum = 300)::bigint AS active_client_count,
    COUNT(c.id) FILTER (WHERE c.status_enum = 100)::bigint AS pending_client_count,
    COUNT(c.id) FILTER (WHERE c.status_enum = 600)::bigint AS closed_client_count
FROM m_client c
WHERE c.office_id = ANY($1::bigint[]);
