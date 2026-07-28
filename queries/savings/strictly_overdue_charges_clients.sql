SELECT
    c.id AS client_id,
    c.display_name AS client_display_name,
    c.office_id,
    o.name AS office_name,
    sa.id AS savings_account_id,
    sac.id AS savings_account_charge_id,
    ch.id AS charge_definition_id,
    ch.name AS charge_name,
    sac.is_penalty,
    sac.charge_time_enum AS charge_timing_enum,
    sa.currency_code,
    sa.currency_digits,
    cur.display_symbol AS currency_display_symbol,
    sac.amount AS amount_due_current,
    COALESCE(sac.amount_paid_derived, 0) AS amount_paid,
    COALESCE(sac.amount_waived_derived, 0) AS amount_waived,
    COALESCE(sac.amount_writtenoff_derived, 0) AS amount_written_off,
      COALESCE(sac.amount_paid_derived, 0)
    + COALESCE(sac.amount_waived_derived, 0)
    + COALESCE(sac.amount_writtenoff_derived, 0)
    + sac.amount_outstanding_derived AS amount_levied_total,
    sac.amount_outstanding_derived AS amount_outstanding,
    sac.charge_due_date AS due_date,
    $2::date - sac.charge_due_date AS days_overdue
FROM m_savings_account_charge sac
JOIN m_savings_account sa ON sa.id = sac.savings_account_id
JOIN m_client c ON c.id = sa.client_id
JOIN m_office o ON o.id = c.office_id
JOIN m_charge ch ON ch.id = sac.charge_id
LEFT JOIN LATERAL (
    SELECT display_symbol
    FROM m_organisation_currency
    WHERE code = sa.currency_code
    LIMIT 1
) cur ON true
WHERE sac.waived = false
  AND sac.is_paid_derived = false
  AND sac.is_active = true
  AND sac.amount_outstanding_derived > 0
  AND sac.charge_due_date < $2::date
  AND c.office_id = ANY($1::bigint[])
ORDER BY sac.charge_due_date ASC, sac.amount_outstanding_derived DESC, sac.id
LIMIT $3;
