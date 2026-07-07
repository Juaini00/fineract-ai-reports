SELECT
    date_trunc('month', c.activation_date)::date AS month_start,
    COUNT(c.id)::bigint AS activation_count
FROM m_client c
WHERE c.office_id = ANY($3::bigint[])
  AND c.activation_date BETWEEN $1::date AND $2::date
  AND c.status_enum IN (300, 600)
GROUP BY date_trunc('month', c.activation_date)
ORDER BY month_start ASC;
