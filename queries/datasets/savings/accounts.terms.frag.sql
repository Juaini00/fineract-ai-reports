SELECT
    savings_account_id,
    masked_account_number,
    account_nominal_annual_interest_rate,
    product_nominal_annual_interest_rate,
    account_allow_overdraft,
    product_allow_overdraft,
    account_overdraft_limit,
    product_overdraft_limit
FROM base
ORDER BY savings_account_id
