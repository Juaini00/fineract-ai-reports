SELECT
    c.office_id,
    o.name AS office_name,
    COUNT(*) FILTER (WHERE c.status_enum = 300)::bigint AS active_count,
    COUNT(*) FILTER (WHERE c.status_enum = 100)::bigint AS pending_count,
    COUNT(*) FILTER (WHERE c.status_enum = 600)::bigint AS closed_count,
    COUNT(*)::bigint AS total_count
FROM m_client c
JOIN m_office o ON o.id = c.office_id
WHERE c.office_id = ANY($1::bigint[])
GROUP BY c.office_id, o.name
ORDER BY total_count DESC, c.office_id ASC
LIMIT $2;
