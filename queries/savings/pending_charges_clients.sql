SELECT
    c.id AS client_id,
    c.display_name AS client_display_name,
    c.office_id,
    o.name AS office_name,
    sac.id AS charge_id,
    ch.name AS charge_name,
    sa.currency_code,
    sac.amount_outstanding_derived AS amount_outstanding,
    sac.charge_due_date AS due_date
FROM m_savings_account_charge sac
JOIN m_savings_account sa ON sa.id = sac.savings_account_id
JOIN m_client c ON c.id = sa.client_id
JOIN m_office o ON o.id = c.office_id
JOIN m_charge ch ON ch.id = sac.charge_id
WHERE sac.waived = false
  AND sac.is_paid_derived = false
  AND sac.is_active = true
  AND sac.amount_outstanding_derived > 0
  AND (sac.charge_due_date IS NULL OR sac.charge_due_date <= $2::date)
  AND c.office_id = ANY($1::bigint[])
ORDER BY sac.charge_due_date NULLS FIRST, sac.id
LIMIT $3;
