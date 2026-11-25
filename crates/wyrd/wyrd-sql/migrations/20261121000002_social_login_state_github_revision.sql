-- A GitHub App connection transaction always names its slot revision
-- (authorization-model spec revision 3, "Persistence and authority changes":
-- wyrd.auth_login_state). 20261121000001 dropped the column's NOT NULL, and a
-- CHECK accepts NULL, so the GitHub branch states it explicitly.
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
         AND connection_id IS NOT NULL AND connection_revision IS NOT NULL
         AND connection_revision >= 0
         AND num_nonnulls(social_provider, social_provider_revision, social_purpose) = 0)
        OR
        (flow = 'social' AND browser_binding_hash IS NOT NULL
         AND num_nonnulls(social_provider, social_provider_revision, social_purpose) = 3
         AND num_nonnulls(connection_id, connection_revision, oauth_client_id, device_id,
                          tester_principal_id, code_hash, github_installation_id) = 0)
    );
