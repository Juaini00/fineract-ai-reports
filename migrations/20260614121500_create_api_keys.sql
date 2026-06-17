CREATE TABLE IF NOT EXISTS api_keys (
    id UUID PRIMARY KEY,
    name TEXT NOT NULL,
    owner TEXT NOT NULL,
    key_prefix TEXT NOT NULL,
    key_hash TEXT NOT NULL UNIQUE,
    allowed_office_ids JSONB NOT NULL DEFAULT '[]'::jsonb,
    allowed_capabilities JSONB NOT NULL DEFAULT '[]'::jsonb,
    can_view_pii BOOLEAN NOT NULL DEFAULT false,
    expires_at TIMESTAMPTZ NULL,
    revoked_at TIMESTAMPTZ NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_used_at TIMESTAMPTZ NULL
);

CREATE INDEX IF NOT EXISTS idx_api_keys_key_hash ON api_keys (key_hash);
CREATE INDEX IF NOT EXISTS idx_api_keys_revoked_at ON api_keys (revoked_at);
