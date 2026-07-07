SELECT
    c.office_id,
    o.name AS office_name,
    COUNT(c.id)::bigint AS activation_count
FROM m_client c
JOIN m_office o ON o.id = c.office_id
WHERE c.office_id = ANY($3::bigint[])
  AND c.activation_date BETWEEN $1::date AND $2::date
  AND c.status_enum IN (300, 600)
GROUP BY c.office_id, o.name
HAVING COUNT(c.id) > 0
ORDER BY activation_count DESC, c.office_id ASC
LIMIT $4;
