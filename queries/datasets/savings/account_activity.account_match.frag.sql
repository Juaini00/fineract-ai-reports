SELECT DISTINCT
    savings_account_id,
    transaction_count
FROM base
WHERE latest_rank = 1
ORDER BY savings_account_id
