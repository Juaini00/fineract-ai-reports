SELECT
    COUNT(*)::bigint AS charge_count,
    COUNT(DISTINCT savings_account_id)::bigint AS savings_account_count,
    COALESCE(SUM(amount_outstanding), 0) AS amount_outstanding_total
FROM base
WHERE is_active = true
