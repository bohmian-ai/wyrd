-- SHA-256 credential verifiers.
--
-- Tenant API keys and platform credentials are server-generated 256-bit
-- secrets, so the server now stores and compares their SHA-256 digest, as it
-- already does for refresh tokens. A row still holding an Argon2 PHC verifier
-- can never match again; revoke it so listings and the active-credential
-- indexes stop counting it. Holders re-issue their keys.
UPDATE wyrd.auth_api_keys
   SET revoked_at = now()
 WHERE revoked_at IS NULL
   AND key_hash LIKE '$argon2%';

UPDATE platform.credentials
   SET revoked_at = now()
 WHERE revoked_at IS NULL
   AND secret_hash LIKE '$argon2%';
