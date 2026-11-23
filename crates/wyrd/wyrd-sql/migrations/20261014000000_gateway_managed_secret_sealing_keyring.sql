-- Store gateway managed-secret provider keys as the common stored-secret
-- envelope.
--
-- `secret_ciphertext` holds the envelope, which names its wrapping key and
-- carries its own data key and nonce bound to the tenant, credential name,
-- and provider, so the separate key version and nonce columns are gone. Boot
-- rewrap moves this column to the active `[kms]` key with every other
-- auth-owned stored secret.
ALTER TABLE wyrd.gateway_provider_credentials
    DROP COLUMN secret_key_version,
    DROP COLUMN secret_nonce;
