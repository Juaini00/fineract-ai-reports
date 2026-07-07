SELECT
    date_trunc('month', o.opening_date)::date AS month_start,
    COUNT(o.id)::bigint AS opened_office_count
FROM m_office o
WHERE o.id = ANY($3::bigint[])
  AND o.opening_date BETWEEN $1::date AND $2::date
GROUP BY date_trunc('month', o.opening_date)
ORDER BY month_start ASC;
