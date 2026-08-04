SELECT
    savings_product_id,
    savings_product_name,
    client_id,
    currency_code,
    deposit_type_enum,
    nominal_annual_interest_rate
FROM base
ORDER BY savings_product_id
LIMIT 25
