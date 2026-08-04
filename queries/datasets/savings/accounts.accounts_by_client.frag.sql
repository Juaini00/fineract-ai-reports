SELECT
    savings_account_id,
    masked_account_number,
    client_id,
    savings_product_id,
    savings_product_name,
    savings_status_enum
FROM base
ORDER BY savings_account_id
LIMIT 25
