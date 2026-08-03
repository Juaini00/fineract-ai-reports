SELECT
    COUNT(*)::bigint AS office_count,
    COUNT(*) FILTER (WHERE o.parent_id IS NULL)::bigint AS root_office_count,
    MIN(o.opening_date) AS oldest_opening_date,
    COALESCE(SUM((
        SELECT COUNT(*)
        FROM m_staff s
        WHERE s.office_id = o.id
          AND s.is_active = true
    )), 0)::bigint AS active_staff_count
FROM m_office o
WHERE o.id = ANY($1::bigint[])
  AND lower(o.name) = lower($2::text)
