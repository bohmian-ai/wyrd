-- An SSO browser session names the first refresh token its login issued.
--
-- Logout retires exactly that login's rotation chain — this row and every
-- descendant through rotated_from — under the User's refresh-family lock,
-- without opening a sealed credential and without touching the User's other
-- logins. The id is not a secret, so revocation keeps it. An API-key session
-- has no refresh chain.
ALTER TABLE wyrd.auth_browser_sessions
    ADD COLUMN refresh_chain_id UUID,
    ADD CONSTRAINT auth_browser_sessions_refresh_chain
        CHECK ((mode = 'oidc_refresh') = (refresh_chain_id IS NOT NULL)),
    ADD CONSTRAINT auth_browser_sessions_refresh_chain_fk
        FOREIGN KEY (data_tenant_id, refresh_chain_id)
        REFERENCES wyrd.auth_refresh_tokens (data_tenant_id, id);
