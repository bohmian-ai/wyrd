-- The platform derivation key (authorization-model specification, revision 8,
-- "Persistence and authority changes": new platform derivation-key record,
-- REQ-019).
--
-- One random key, generated once, that signup and sign-in flow cookies, email
-- proofs and keyed pseudonyms derive their per-use keys from. Only its
-- `WyrdKms` envelope is stored, never the raw key. Rotating the wrapping key
-- rewraps the envelope through stored-secret maintenance (a compare-and-swap
-- on the envelope bytes) and never changes the key, so pseudonyms stay stable.
-- Replicas that start together race on the singleton insert and all read the
-- one row that won.
--
-- From this revision the provider, SMTP and challenge secret columns
-- (`platform.social_providers.client_secret_enc`,
-- `platform.login_settings.smtp_password_enc` and
-- `signup_challenge_secret`) hold `WyrdKms` envelopes.
CREATE TABLE platform.derivation_key (
    singleton   BOOLEAN     PRIMARY KEY DEFAULT TRUE CHECK (singleton),
    envelope    BYTEA       NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT statement_timestamp()
);

ALTER TABLE platform.derivation_key ENABLE ROW LEVEL SECURITY;
ALTER TABLE platform.derivation_key FORCE ROW LEVEL SECURITY;
CREATE POLICY operator_access ON platform.derivation_key TO CURRENT_USER
    USING (wyrd.operator_session()) WITH CHECK (wyrd.operator_session());
