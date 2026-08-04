SELECT
    o.id AS office_id,
    o.name AS office_name,
    o.parent_id AS parent_office_id,
    o.opening_date AS opening_date,
    (
        SELECT COUNT(*)
        FROM m_staff s
        WHERE s.office_id = o.id
          AND s.is_active = true
    )::bigint AS office_active_staff_count
FROM m_office o
WHERE o.id = ANY($1::bigint[])
