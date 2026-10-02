-- Platform human login verifies the ID token's audience against the
-- connection's client_id (OpenID Connect Core 1.0 §3.1.3.7 step 3), as tenant
-- human login does, so the separately entered audience is no longer stored.
ALTER TABLE platform.oidc_connection DROP COLUMN expected_audience;
