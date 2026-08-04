SELECT
    savings_transaction_id,
    savings_account_id,
    client_id,
    client_display_name,
    transaction_type_enum,
    transaction_date,
    amount,
    running_balance
FROM base
ORDER BY transaction_date DESC, savings_transaction_id DESC
LIMIT 100
