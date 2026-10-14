-- Seal gateway managed-secret provider keys with the deployment sealing keyring.
--
-- `secret_ciphertext` now holds the keyring's versioned envelope, which names
-- its sealing key and carries its own nonce, so the separate per-tenant key
-- version and nonce columns are gone. Boot rewrap reseals this column with
-- every other stored secret. An envelope sealed under a removed per-tenant
-- keyring no longer opens; its administrator resubmits the credential.
ALTER TABLE wyrd.gateway_provider_credentials
    DROP COLUMN secret_key_version,
    DROP COLUMN secret_nonce;
