SELECT
    COUNT(*)::bigint AS office_count,
    COUNT(*) FILTER (WHERE parent_office_id IS NULL)::bigint AS root_office_count,
    MIN(opening_date) AS oldest_opening_date,
    COALESCE(SUM(office_active_staff_count), 0)::bigint AS active_staff_count
FROM base
