-- Tenant GitHub App installation associations.
--
-- The deployment operator configures one GitHub App; a tenant associates at
-- most one of its installations through Wyrd's verified connect callback. A
-- tenant has at most one row, its connection slot, created by its first
-- association and kept after a disconnect so its revision keeps counting:
-- every change to the association or its access advances `revision`, and a
-- connection transaction completes only against the exact slot and revision
-- it began from, so a callback begun before a disconnect, a reconnect or a
-- replacement can never restore the old association.
--
-- An associated slot holds the configured host, the verified installation,
-- the tenant-selected repositories that installation can currently reach as
-- `[{"repository_id": "<decimal>", "full_name": "owner/name"}]`, and the
-- instant GitHub suspended it, if it did. A suspended installation stays
-- associated, so it still blocks every other tenant. A released slot holds
-- no installation. No App key, user token or installation token is stored.
--
-- The primary key allows one associated installation per tenant, and the
-- partial unique index allows one tenant per (host, installation). Both are
-- enforced by Postgres, so concurrent claims from any replica resolve to one
-- winner. A unique index is not subject to RLS, so it refuses another
-- tenant's claim that the claimant cannot see.
CREATE TABLE wyrd.github_installations (
    data_tenant_id  UUID        PRIMARY KEY REFERENCES platform.tenants(data_tenant_id),
    connection_id   UUID        NOT NULL UNIQUE,
    revision        BIGINT      NOT NULL CHECK (revision > 0),
    host            TEXT,
    installation_id BIGINT      CHECK (installation_id > 0),
    repositories    JSONB       NOT NULL DEFAULT '[]'::jsonb
        CHECK (jsonb_typeof(repositories) = 'array'),
    suspended_at    TIMESTAMPTZ,
    connected_at    TIMESTAMPTZ,
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT statement_timestamp(),
    CONSTRAINT github_installations_association CHECK (
        (installation_id IS NOT NULL AND host IS NOT NULL AND connected_at IS NOT NULL)
        OR (installation_id IS NULL AND host IS NULL AND connected_at IS NULL
            AND suspended_at IS NULL AND repositories = '[]'::jsonb)
    )
);

CREATE UNIQUE INDEX github_installations_one_tenant
    ON wyrd.github_installations (host, installation_id)
    WHERE installation_id IS NOT NULL;

ALTER TABLE wyrd.github_installations ENABLE ROW LEVEL SECURITY;
ALTER TABLE wyrd.github_installations FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON wyrd.github_installations
    USING (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());
CREATE POLICY operator_access ON wyrd.github_installations TO CURRENT_USER
    USING (wyrd.operator_session()) WITH CHECK (wyrd.operator_session());

-- A signed installation webhook names an installation, never a tenant. It
-- learns which tenant to open a row-level-secured transaction for through
-- this one answer and nothing else: the tenant whose slot is associated with
-- exactly this host and installation, or NULL. Routing grants no permission.
CREATE FUNCTION wyrd.github_installation_tenant(p_host text, p_installation_id bigint)
RETURNS uuid
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog, wyrd
AS $$
DECLARE
    prior text := coalesce(current_setting('app.operator', true), '');
    tenant uuid;
BEGIN
    PERFORM set_config('app.operator', 'on', true);
    SELECT data_tenant_id INTO tenant FROM wyrd.github_installations
     WHERE host = p_host AND installation_id = p_installation_id;
    PERFORM set_config('app.operator', prior, true);
    RETURN tenant;
END;
$$;

-- A GitHub connection transaction is ordinary one-use login state with the
-- same tenant RLS, state hash and PostgreSQL-owned expiry, marked by `flow`
-- so neither the OIDC callback nor the GitHub callback consumes the other's
-- rows. It binds the principal that began it (the tester columns), names the
-- slot and revision it began from as `connection_id`/`connection_revision`
-- (revision 0 when the tenant has no slot yet), and once GitHub's setup
-- redirect reaches a browser signed in as that principal, that browser's
-- transaction secret and the installation id GitHub hinted. The hint is not
-- proof. GitHub's OAuth has no nonce, and its transaction issues no code.
ALTER TABLE wyrd.auth_login_state
    ADD COLUMN flow TEXT NOT NULL DEFAULT 'oidc' CHECK (flow IN ('oidc', 'github')),
    ADD COLUMN github_installation_id BIGINT CHECK (github_installation_id > 0),
    ALTER COLUMN nonce DROP NOT NULL;

ALTER TABLE wyrd.auth_login_state DROP CONSTRAINT auth_login_state_initiation;

ALTER TABLE wyrd.auth_login_state
    ADD CONSTRAINT auth_login_state_initiation CHECK (
        (flow = 'oidc' AND nonce IS NOT NULL AND github_installation_id IS NULL
         AND num_nonnulls(oauth_client_id, device_id, tester_principal_id,
                          browser_binding_hash) = 1)
        OR
        (flow = 'github' AND nonce IS NULL AND tester_principal_id IS NOT NULL
         AND oauth_client_id IS NULL AND device_id IS NULL AND code_hash IS NULL
         AND connection_revision >= 0)
    );
