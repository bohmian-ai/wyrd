-- Social enrollment and returning-sign-in anchors (authorization-model spec
-- revision 4, "Persistence and authority changes": the existing tenant
-- identity store, wyrd.auth_login_state and wyrd.auth_browser_sessions).
--
-- A linked identity gains a stable id that credentials anchor to, so removing
-- the identity removes every session it created. A social sign-in begun from
-- an invitation link binds the SHA-256 of that link's code; the callback
-- consumes the invitation by it. A social browser session names the provider,
-- the exact provider revision it signed in through, and the linked identity.
ALTER TABLE wyrd.auth_user_identities
    ADD COLUMN id UUID NOT NULL DEFAULT gen_random_uuid(),
    ADD CONSTRAINT auth_user_identities_tenant_id UNIQUE (data_tenant_id, id);

ALTER TABLE wyrd.auth_login_state
    ADD COLUMN social_invitation_hash BYTEA
        CHECK (octet_length(social_invitation_hash) = 32),
    ADD CONSTRAINT auth_login_state_social_invitation
        CHECK ((social_invitation_hash IS NOT NULL)
               = (social_purpose IS NOT DISTINCT FROM 'invitation'));

ALTER TABLE wyrd.auth_browser_sessions
    ADD COLUMN social_provider TEXT REFERENCES platform.social_providers(provider),
    ADD COLUMN social_provider_revision BIGINT CHECK (social_provider_revision > 0),
    ADD COLUMN social_identity_id UUID,
    ADD CONSTRAINT auth_browser_sessions_social_identity
        FOREIGN KEY (data_tenant_id, social_identity_id)
        REFERENCES wyrd.auth_user_identities(data_tenant_id, id) ON DELETE CASCADE;

ALTER TABLE wyrd.auth_browser_sessions DROP CONSTRAINT auth_browser_sessions_authority;

ALTER TABLE wyrd.auth_browser_sessions
    ADD CONSTRAINT auth_browser_sessions_authority
        CHECK ((connection_id IS NULL) = (connection_revision IS NULL)
               AND num_nonnulls(social_provider, social_provider_revision,
                                social_identity_id) IN (0, 3)
               AND num_nonnulls(connection_id, api_key_id, password_revision,
                                social_identity_id) = 1);

CREATE INDEX auth_browser_sessions_by_social_identity
    ON wyrd.auth_browser_sessions (data_tenant_id, social_identity_id)
    WHERE social_identity_id IS NOT NULL;
