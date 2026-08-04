SELECT
    COUNT(*)::bigint AS charge_count,
    COUNT(DISTINCT sa.id)::bigint AS savings_account_count,
    COALESCE(SUM(sac.amount_outstanding_derived), 0) AS amount_outstanding_total
FROM m_savings_account_charge sac
JOIN m_savings_account sa ON sa.id = sac.savings_account_id
JOIN m_client c ON c.id = sa.client_id
JOIN m_charge ch ON ch.id = sac.charge_id
WHERE c.office_id = ANY($1::bigint[])
  AND sac.is_active = true
  AND lower(ch.name) = lower($2::text)
