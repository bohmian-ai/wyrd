-- The deployment's optional SMTP relay for application mail.
--
-- Authorized by the authorization-model specification, revision 4,
-- "Persistence and authority changes": new platform application-mail/signup
-- settings records (SMTP host/port/TLS/sender and sealed credentials). The
-- relay joins the existing one-row `platform.login_settings` record, which
-- already admits only operator sessions under forced row-level security.
--
-- All relay columns are NULL until an operator configures one. The password
-- is a sealing-keyring envelope and requires a username.
ALTER TABLE platform.login_settings
    ADD COLUMN smtp_host         TEXT    CHECK (smtp_host <> ''),
    ADD COLUMN smtp_port         INTEGER CHECK (smtp_port BETWEEN 1 AND 65535),
    ADD COLUMN smtp_tls          TEXT    CHECK (smtp_tls IN ('none', 'starttls', 'tls')),
    ADD COLUMN smtp_username     TEXT    CHECK (smtp_username <> ''),
    ADD COLUMN smtp_password_enc BYTEA,
    ADD COLUMN smtp_sender       TEXT    CHECK (smtp_sender <> ''),
    ADD CONSTRAINT login_settings_smtp_complete CHECK (
        (smtp_host IS NULL) = (smtp_port IS NULL)
        AND (smtp_host IS NULL) = (smtp_tls IS NULL)
        AND (smtp_host IS NULL) = (smtp_sender IS NULL)
        AND (smtp_host IS NOT NULL OR smtp_username IS NULL)
        AND (smtp_password_enc IS NULL OR smtp_username IS NOT NULL)
    );
