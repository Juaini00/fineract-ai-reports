WITH scoped_offices AS (
    SELECT id AS office_id
    FROM m_office
    WHERE id = ANY($1::bigint[])
)
SELECT
    COUNT(o.id)::bigint AS office_count,
    COUNT(o.id) FILTER (WHERE o.parent_id IS NULL)::bigint AS root_office_count,
    MIN(o.opening_date) AS oldest_opening_date,
    COUNT(s.id) FILTER (WHERE s.is_active = true)::bigint AS active_staff_count
FROM scoped_offices scope
JOIN m_office o ON o.id = scope.office_id
LEFT JOIN m_staff s ON s.office_id = o.id
;
