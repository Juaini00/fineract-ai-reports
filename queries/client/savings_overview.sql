SELECT
    c.id AS client_id,
    c.display_name,
    o.id AS office_id,
    o.name AS office_name,
    accounts.savings_account_count,
    accounts.active_savings_account_count,
    charges.active_unpaid_charge_count,
    charges.active_unpaid_charge_amount_outstanding,
    transactions.transaction_count
FROM m_client c
JOIN m_office o ON o.id = c.office_id
LEFT JOIN LATERAL (
    SELECT
        COUNT(*)::bigint AS savings_account_count,
        COUNT(*) FILTER (WHERE sa.status_enum = 300)::bigint AS active_savings_account_count
    FROM m_savings_account sa
    WHERE sa.client_id = c.id
) accounts ON true
LEFT JOIN LATERAL (
    SELECT
        COUNT(*)::bigint AS active_unpaid_charge_count,
        COALESCE(SUM(sac.amount_outstanding_derived), 0) AS active_unpaid_charge_amount_outstanding
    FROM m_savings_account_charge sac
    JOIN m_savings_account sa ON sa.id = sac.savings_account_id
    WHERE sa.client_id = c.id
      AND sac.is_active = true
      AND sac.waived = false
      AND sac.is_paid_derived = false
      AND sac.amount_outstanding_derived > 0
) charges ON true
LEFT JOIN LATERAL (
    SELECT COUNT(*)::bigint AS transaction_count
    FROM m_savings_account_transaction t
    JOIN m_savings_account sa ON sa.id = t.savings_account_id
    WHERE sa.client_id = c.id
      AND t.is_reversed = false
) transactions ON true
WHERE c.office_id = ANY($1::bigint[])
  AND c.display_name ILIKE '%' || $2::text || '%'
ORDER BY c.display_name, c.id
LIMIT $3
