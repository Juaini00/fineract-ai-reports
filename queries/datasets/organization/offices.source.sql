SELECT
    o.id AS office_id,
    o.name AS office_name,
    o.parent_id AS parent_office_id,
    o.opening_date
FROM m_office o
WHERE o.id = ANY($1::bigint[])
