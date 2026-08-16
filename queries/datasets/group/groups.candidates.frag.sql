SELECT
    group_id,
    group_display_name,
    group_status_enum,
    office_id,
    staff_id,
    parent_group_id,
    hierarchy,
    member_count
FROM base
ORDER BY group_id
LIMIT 25
