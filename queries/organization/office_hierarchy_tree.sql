WITH RECURSIVE tree AS (
    SELECT
        o.id AS office_id,
        o.name AS office_name,
        o.parent_id,
        1::bigint AS depth,
        ARRAY[o.id]::bigint[] AS path
    FROM m_office o
    WHERE o.id = ANY($1::bigint[])
      AND o.parent_id IS NULL
    UNION ALL
    SELECT
        child.id,
        child.name,
        child.parent_id,
        tree.depth + 1,
        tree.path || child.id
    FROM m_office child
    JOIN tree ON child.parent_id = tree.office_id
    WHERE child.id = ANY($1::bigint[])
      AND tree.depth < 10
)
SELECT
    office_id,
    office_name,
    parent_id,
    depth
FROM tree
ORDER BY path
LIMIT $2;
