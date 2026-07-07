WITH scoped AS (
    SELECT id AS office_id, parent_id, hierarchy
    FROM m_office
    WHERE id = ANY($1::bigint[])
)
SELECT
    COUNT(s.office_id)::bigint AS total_office_count,
    COUNT(s.office_id) FILTER (WHERE s.parent_id IS NULL)::bigint AS root_office_count,
    COUNT(s.office_id) FILTER (
        WHERE NOT EXISTS (SELECT 1 FROM scoped c WHERE c.parent_id = s.office_id)
    )::bigint AS leaf_office_count,
    COALESCE(MAX(length(s.hierarchy) - length(replace(s.hierarchy, '.', '')) - 1), 0)::bigint AS max_hierarchy_depth
FROM scoped s;
