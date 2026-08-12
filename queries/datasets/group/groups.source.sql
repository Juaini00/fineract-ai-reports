SELECT
    g.id AS group_id,
    g.display_name AS group_display_name,
    g.status_enum::bigint AS group_status_enum,
    g.office_id,
    g.staff_id,
    g.parent_id AS parent_group_id,
    g.hierarchy,
    COUNT(gc.client_id)::bigint AS member_count
FROM m_group g
LEFT JOIN m_group_client gc ON gc.group_id = g.id
WHERE g.office_id = ANY($1::bigint[])
GROUP BY g.id, g.display_name, g.status_enum, g.office_id, g.staff_id, g.parent_id, g.hierarchy
