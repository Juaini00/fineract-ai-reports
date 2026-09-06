SELECT
    client_id,
    client_display_name,
    office_id,
    office_name,
    client_status_enum
FROM base
ORDER BY client_id
LIMIT 25
