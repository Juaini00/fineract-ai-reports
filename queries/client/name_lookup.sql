SELECT
    c.id AS client_id,
    c.display_name,
    o.name AS office_name,
    CASE c.status_enum
        WHEN 100 THEN 'pending'
        WHEN 300 THEN 'active'
        WHEN 600 THEN 'closed'
        ELSE 'other'
    END AS status_label
FROM m_client c
JOIN m_office o ON o.id = c.office_id
WHERE c.office_id = ANY($1::bigint[])
  AND c.display_name ILIKE '%' || $2::text || '%'
ORDER BY c.display_name ASC, c.id ASC
LIMIT 20;
