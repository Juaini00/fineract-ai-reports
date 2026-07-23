ALTER TABLE assistant_llm_traces
    ALTER COLUMN input_tokens DROP NOT NULL,
    ALTER COLUMN output_tokens DROP NOT NULL,
    ALTER COLUMN total_tokens DROP NOT NULL,
    ADD COLUMN usage_status TEXT NOT NULL DEFAULT 'provider_reported'
        CHECK (usage_status IN ('provider_reported', 'estimated', 'unavailable'));

UPDATE assistant_llm_traces
SET usage_status = 'provider_reported'
WHERE usage_status IS NULL;
