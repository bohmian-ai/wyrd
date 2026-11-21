-- Purpose-bound shared social sign-in transactions (authorization-model spec
-- revision 3, "Persistence and authority changes": wyrd.auth_login_state).
--
-- A tenant sign-in through the deployment's shared Google or GitHub.com
-- client is the same one-use, five-minute row under flow 'social'. It binds
-- the exact platform provider revision the redirect was issued for, the
-- purpose it may complete, and the digest of the beginning browser's
-- login-transaction secret. It names no tenant connection. The company OIDC
-- and GitHub App callbacks filter on their own flow, so neither consumes a
-- social row, and the social callback consumes only its own.
ALTER TABLE wyrd.auth_login_state
    ADD COLUMN social_provider TEXT REFERENCES platform.social_providers(provider),
    ADD COLUMN social_provider_revision BIGINT CHECK (social_provider_revision > 0),
    ADD COLUMN social_purpose TEXT CHECK (social_purpose IN ('login', 'invitation', 'link')),
    ALTER COLUMN connection_id DROP NOT NULL,
    ALTER COLUMN connection_revision DROP NOT NULL;

ALTER TABLE wyrd.auth_login_state DROP CONSTRAINT auth_login_state_flow_check;
ALTER TABLE wyrd.auth_login_state
    ADD CONSTRAINT auth_login_state_flow_check CHECK (flow IN ('oidc', 'github', 'social'));

ALTER TABLE wyrd.auth_login_state DROP CONSTRAINT auth_login_state_initiation;

ALTER TABLE wyrd.auth_login_state
    ADD CONSTRAINT auth_login_state_initiation CHECK (
        (flow = 'oidc' AND nonce IS NOT NULL AND github_installation_id IS NULL
         AND connection_id IS NOT NULL AND connection_revision IS NOT NULL
         AND num_nonnulls(social_provider, social_provider_revision, social_purpose) = 0
         AND num_nonnulls(oauth_client_id, device_id, tester_principal_id,
                          browser_binding_hash) = 1)
        OR
        (flow = 'github' AND nonce IS NULL AND tester_principal_id IS NOT NULL
         AND oauth_client_id IS NULL AND device_id IS NULL AND code_hash IS NULL
         AND connection_id IS NOT NULL AND connection_revision >= 0
         AND num_nonnulls(social_provider, social_provider_revision, social_purpose) = 0)
        OR
        (flow = 'social' AND browser_binding_hash IS NOT NULL
         AND num_nonnulls(social_provider, social_provider_revision, social_purpose) = 3
         AND num_nonnulls(connection_id, connection_revision, oauth_client_id, device_id,
                          tester_principal_id, code_hash, github_installation_id) = 0)
    );
