SELECT
    charge_definition_id,
    charge_name,
    currency_code,
    is_penalty,
    charge_timing_enum
FROM base
ORDER BY charge_name, charge_definition_id
LIMIT 25
