SELECT
    sa.id AS savings_account_id,
    c.id AS client_id,
    c.display_name AS client_display_name,
    sa.deposit_type_enum::bigint AS deposit_type_enum,
    sa.currency_code,
    t.deposit_amount AS term_deposit_amount,
    t.maturity_amount,
    t.maturity_date,
    t.deposit_period,
    t.deposit_period_frequency_enum::bigint AS deposit_period_frequency_enum,
    r.mandatory_recommended_deposit_amount,
    r.is_mandatory,
    r.total_overdue_amount,
    r.no_of_overdue_installments
FROM m_savings_account sa
JOIN m_client c ON c.id = sa.client_id
LEFT JOIN m_deposit_account_term_and_preclosure t ON t.savings_account_id = sa.id
LEFT JOIN m_deposit_account_recurring_detail r ON r.savings_account_id = sa.id
WHERE sa.deposit_type_enum IN (200, 300)
  AND c.office_id = ANY($1::bigint[])
