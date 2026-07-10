ALTER TABLE api_keys
    ADD COLUMN allow_all_capabilities BOOLEAN NOT NULL DEFAULT false,
    ADD COLUMN allow_all_offices      BOOLEAN NOT NULL DEFAULT false;
