-- Scoped authorization: roles are operation bundles, bindings assign one role
-- to one account or local group at one explicit scope, and one per-tenant
-- revision advances with every access-affecting change.
--
-- wyrd.auth_roles keeps its table; its JSONB `permissions` column now holds
-- the role's operations ([{"resource","action"}]) with no object scope. The
-- binding's scope supplies where those operations apply.

-- Optional tenant-local rosters of Users and Service accounts. A group is
-- never a principal, owner, or namespace and never logs in.
CREATE TABLE wyrd.auth_groups (
    data_tenant_id  UUID        NOT NULL REFERENCES platform.tenants(data_tenant_id),
    id              UUID        NOT NULL,
    name            TEXT        NOT NULL
        CONSTRAINT auth_groups_name_canonical
        CHECK (name ~ '^[a-z0-9][a-z0-9._-]{0,63}$'),
    created_at      TIMESTAMPTZ NOT NULL DEFAULT statement_timestamp(),
    PRIMARY KEY (data_tenant_id, id),
    UNIQUE (data_tenant_id, name)
);

ALTER TABLE wyrd.auth_groups ENABLE ROW LEVEL SECURITY;
ALTER TABLE wyrd.auth_groups FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON wyrd.auth_groups
    USING (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());
CREATE POLICY operator_access ON wyrd.auth_groups TO CURRENT_USER
    USING (wyrd.operator_session()) WITH CHECK (wyrd.operator_session());

-- One account in one group, referencing exactly one existing User or Service
-- account of the same tenant.
CREATE TABLE wyrd.auth_group_memberships (
    data_tenant_id      UUID        NOT NULL REFERENCES platform.tenants(data_tenant_id),
    group_id            UUID        NOT NULL,
    user_id             UUID,
    service_account_id  UUID,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT statement_timestamp(),
    CONSTRAINT auth_group_memberships_one_account
        CHECK (num_nonnulls(user_id, service_account_id) = 1),
    UNIQUE NULLS NOT DISTINCT (data_tenant_id, group_id, user_id, service_account_id),
    FOREIGN KEY (data_tenant_id, group_id)
        REFERENCES wyrd.auth_groups(data_tenant_id, id) ON DELETE CASCADE,
    FOREIGN KEY (data_tenant_id, user_id)
        REFERENCES wyrd.auth_users(data_tenant_id, id) ON DELETE CASCADE,
    FOREIGN KEY (data_tenant_id, service_account_id)
        REFERENCES wyrd.auth_service_accounts(data_tenant_id, id) ON DELETE CASCADE
);

CREATE INDEX auth_group_memberships_by_user
    ON wyrd.auth_group_memberships (data_tenant_id, user_id) WHERE user_id IS NOT NULL;
CREATE INDEX auth_group_memberships_by_service_account
    ON wyrd.auth_group_memberships (data_tenant_id, service_account_id)
    WHERE service_account_id IS NOT NULL;

ALTER TABLE wyrd.auth_group_memberships ENABLE ROW LEVEL SECURITY;
ALTER TABLE wyrd.auth_group_memberships FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON wyrd.auth_group_memberships
    USING (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());
CREATE POLICY operator_access ON wyrd.auth_group_memberships TO CURRENT_USER
    USING (wyrd.operator_session()) WITH CHECK (wyrd.operator_session());

-- One role assigned to exactly one subject at one explicit typed scope.
-- `source` keeps direct and identity-provider provenance distinct; a
-- provider login replaces only its own User's `idp` bindings. Group bindings
-- are always direct.
CREATE TABLE wyrd.auth_role_bindings (
    data_tenant_id      UUID        NOT NULL REFERENCES platform.tenants(data_tenant_id),
    id                  UUID        NOT NULL,
    role_id             UUID        NOT NULL,
    user_id             UUID,
    service_account_id  UUID,
    group_id            UUID,
    scope               JSONB       NOT NULL
        CONSTRAINT auth_role_bindings_scope_shape
        CHECK (jsonb_typeof(scope) IN ('string', 'object')),
    source              TEXT        NOT NULL CHECK (source IN ('direct', 'idp')),
    created_at          TIMESTAMPTZ NOT NULL DEFAULT statement_timestamp(),
    PRIMARY KEY (data_tenant_id, id),
    CONSTRAINT auth_role_bindings_one_subject
        CHECK (num_nonnulls(user_id, service_account_id, group_id) = 1),
    CONSTRAINT auth_role_bindings_provider_user
        CHECK (source = 'direct' OR user_id IS NOT NULL),
    UNIQUE NULLS NOT DISTINCT
        (data_tenant_id, role_id, user_id, service_account_id, group_id, source, scope),
    FOREIGN KEY (data_tenant_id, role_id)
        REFERENCES wyrd.auth_roles(data_tenant_id, id) ON DELETE CASCADE,
    FOREIGN KEY (data_tenant_id, user_id)
        REFERENCES wyrd.auth_users(data_tenant_id, id) ON DELETE CASCADE,
    FOREIGN KEY (data_tenant_id, service_account_id)
        REFERENCES wyrd.auth_service_accounts(data_tenant_id, id) ON DELETE CASCADE,
    FOREIGN KEY (data_tenant_id, group_id)
        REFERENCES wyrd.auth_groups(data_tenant_id, id) ON DELETE CASCADE
);

CREATE INDEX auth_role_bindings_by_user
    ON wyrd.auth_role_bindings (data_tenant_id, user_id) WHERE user_id IS NOT NULL;
CREATE INDEX auth_role_bindings_by_service_account
    ON wyrd.auth_role_bindings (data_tenant_id, service_account_id)
    WHERE service_account_id IS NOT NULL;
CREATE INDEX auth_role_bindings_by_group
    ON wyrd.auth_role_bindings (data_tenant_id, group_id) WHERE group_id IS NOT NULL;
CREATE INDEX auth_role_bindings_by_role
    ON wyrd.auth_role_bindings (data_tenant_id, role_id);

ALTER TABLE wyrd.auth_role_bindings ENABLE ROW LEVEL SECURITY;
ALTER TABLE wyrd.auth_role_bindings FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON wyrd.auth_role_bindings
    USING (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());
CREATE POLICY operator_access ON wyrd.auth_role_bindings TO CURRENT_USER
    USING (wyrd.operator_session()) WITH CHECK (wyrd.operator_session());

-- Unscoped role assignments are superseded by scoped bindings. Nothing has
-- shipped, so there is no conversion.
DROP TABLE wyrd.auth_user_roles;
DROP TABLE wyrd.auth_service_account_roles;

-- One monotonically advancing authorization revision per tenant. Every
-- access-affecting mutation advances it in its own transaction and notifies
-- `wyrd_authorization` with '<data_tenant_id>:<revision>'; replicas confirm
-- it in the background.
CREATE TABLE wyrd.auth_authorization_revisions (
    data_tenant_id  UUID        PRIMARY KEY REFERENCES platform.tenants(data_tenant_id),
    revision        BIGINT      NOT NULL CHECK (revision > 0),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT statement_timestamp()
);

ALTER TABLE wyrd.auth_authorization_revisions ENABLE ROW LEVEL SECURITY;
ALTER TABLE wyrd.auth_authorization_revisions FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON wyrd.auth_authorization_revisions
    USING (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());
CREATE POLICY operator_access ON wyrd.auth_authorization_revisions TO CURRENT_USER
    USING (wyrd.operator_session()) WITH CHECK (wyrd.operator_session());

-- A CLI device approval records exactly one human authentication anchor: the
-- provider connection and its revision, or the local User's password
-- revision. Unapproved rows carry no principal and no anchor.
ALTER TABLE wyrd.auth_device_authorizations
    ADD COLUMN password_revision BIGINT,
    DROP CONSTRAINT auth_device_authorizations_approval,
    ADD CONSTRAINT auth_device_authorizations_approval
        CHECK (
            ((principal_id IS NULL)
                AND num_nonnulls(connection_id, connection_revision, password_revision) = 0)
            OR ((principal_id IS NOT NULL)
                AND (connection_id IS NULL) = (connection_revision IS NULL)
                AND num_nonnulls(connection_id, password_revision) = 1)
        );

-- A human refresh family carries its client binding and exactly the anchor
-- its login went through; renewal verifies that same anchor.
ALTER TABLE wyrd.auth_refresh_tokens
    ADD COLUMN password_revision BIGINT,
    DROP CONSTRAINT auth_refresh_tokens_human_client,
    DROP CONSTRAINT auth_refresh_tokens_human_connection_user,
    ADD CONSTRAINT auth_refresh_tokens_human_anchor
        CHECK (num_nonnulls(human_connection_id, password_revision) <= 1
               AND (client_id IS NULL)
                   = (human_connection_id IS NULL AND password_revision IS NULL)
               AND (client_id IS NULL OR principal_kind = 'user'));

-- An invitation records explicit role-and-scope assignment intents, never
-- bare role ids, so enrollment cannot widen a narrow invitation. A pending
-- role-only invitation cannot be expressed as a scoped intent; it is removed
-- and must be reissued.
DELETE FROM wyrd.auth_user_invitations;
ALTER TABLE wyrd.auth_user_invitations
    DROP COLUMN role_ids,
    ADD COLUMN assignments JSONB NOT NULL
        CONSTRAINT auth_user_invitations_assignments_shape
        CHECK (jsonb_typeof(assignments) = 'array' AND jsonb_array_length(assignments) > 0);

-- A User key keeps its required name, ceiling, and expiry; a Service key may
-- now carry a restrictive ceiling too.
ALTER TABLE wyrd.auth_api_keys
    DROP CONSTRAINT auth_api_keys_user_key_shape,
    ADD CONSTRAINT auth_api_keys_user_key_shape
        CHECK ((user_id IS NULL) = (name IS NULL)
               AND (user_id IS NULL OR (permission_ceiling IS NOT NULL AND expires_at IS NOT NULL)));
