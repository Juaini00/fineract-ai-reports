SELECT
    savings_account_id,
    masked_account_number,
    client_id,
    client_display_name,
    office_id,
    office_name,
    savings_product_id,
    savings_product_name,
    savings_status_enum,
    currency_code
FROM base
ORDER BY savings_account_id
