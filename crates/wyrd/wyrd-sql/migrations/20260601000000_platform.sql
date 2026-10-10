-- Wyrd platform schema bootstrap.
--
-- Operators create every Postgres login; Wyrd creates no roles and grants
-- nothing to a named role. This migration runs through the one-off
-- `wyrd-server migrate` command as the platform login, which owns every object
-- these migrations create.
--
-- Every table forces row-level security, so the owner is policy-bound too.
-- Tenant-keyed tables admit the tenant bound by `TenantConn`; every table also
-- admits an operator session through its `operator_access` policy, which
-- targets the migrating owner (`TO CURRENT_USER`) and requires the
-- `app.operator` flag that only `OperatorPool` connections set. A separate
-- tenant login is not the owner, so setting the flag gives it nothing.

CREATE SCHEMA IF NOT EXISTS platform;
CREATE SCHEMA IF NOT EXISTS wyrd;

-- Whether this session is an operator session. Only `operator_access`
-- policies consult it, and they apply only to the owner role.
CREATE FUNCTION wyrd.operator_session() RETURNS boolean
LANGUAGE sql STABLE PARALLEL RESTRICTED AS $$
    SELECT coalesce(current_setting('app.operator', true), '') = 'on'
$$;

-- The tenant `TenantConn` bound to this transaction. A tenant session with no
-- binding fails loudly rather than reading nothing; an operator session, which
-- evaluates `tenant_isolation` alongside `operator_access`, reads NULL.
CREATE FUNCTION wyrd.current_tenant() RETURNS uuid
LANGUAGE sql STABLE PARALLEL RESTRICTED AS $$
    SELECT CASE WHEN wyrd.operator_session()
        THEN nullif(current_setting('app.current_tenant', true), '')::uuid
        ELSE current_setting('app.current_tenant')::uuid
    END
$$;

-- The ledger is a table like any other: only operator sessions, which the
-- migration runs as, may read or write it.
ALTER TABLE wyrd._sqlx_migrations ENABLE ROW LEVEL SECURITY;
ALTER TABLE wyrd._sqlx_migrations FORCE ROW LEVEL SECURITY;
CREATE POLICY operator_access ON wyrd._sqlx_migrations TO CURRENT_USER
    USING (wyrd.operator_session()) WITH CHECK (wyrd.operator_session());

CREATE TABLE platform.tenants (
    data_tenant_id  UUID PRIMARY KEY,
    slug            TEXT UNIQUE NOT NULL
        CHECK (slug ~ '^[a-z0-9][a-z0-9_-]{0,62}$' AND length(slug) BETWEEN 1 AND 63),
    display_name    TEXT NOT NULL,
    status          TEXT NOT NULL CHECK (status IN ('active','suspended','deleted')),
    plan            TEXT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    deleted_at      TIMESTAMPTZ
);

CREATE INDEX tenants_by_slug
    ON platform.tenants (slug) WHERE deleted_at IS NULL;
CREATE INDEX tenants_by_status
    ON platform.tenants (status) WHERE status = 'active';

ALTER TABLE platform.tenants ENABLE ROW LEVEL SECURITY;
ALTER TABLE platform.tenants FORCE ROW LEVEL SECURITY;
CREATE POLICY operator_access ON platform.tenants TO CURRENT_USER
    USING (wyrd.operator_session()) WITH CHECK (wyrd.operator_session());

-- Postgres refuses an ordinary owner a custom parameter in a function's SET
-- clause, so the body raises the operator flag transaction-locally and restores
-- the caller's value before returning; an error aborts the transaction, or the
-- enclosing savepoint, which reverts it too.
CREATE FUNCTION platform.resolve_tenant_by_slug(p_slug TEXT)
RETURNS uuid
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog, platform
AS $$
DECLARE
    prior text := coalesce(current_setting('app.operator', true), '');
    tenant uuid;
BEGIN
    PERFORM set_config('app.operator', 'on', true);
    SELECT data_tenant_id INTO tenant FROM platform.tenants
    WHERE slug = p_slug
      AND status = 'active'
      AND deleted_at IS NULL;
    PERFORM set_config('app.operator', prior, true);
    RETURN tenant;
END;
$$;

CREATE TABLE platform.users (
    id              TEXT PRIMARY KEY,
    email           TEXT NOT NULL UNIQUE,
    auth_type       TEXT NOT NULL CHECK (auth_type IN ('password','sso','service_account')),
    password_hash   TEXT,
    status          TEXT NOT NULL CHECK (status IN ('active','suspended','deleted')),
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX platform_users_by_email ON platform.users (email);

CREATE TABLE platform.roles (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL UNIQUE,
    permissions JSONB NOT NULL,
    builtin     BOOLEAN NOT NULL DEFAULT FALSE,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE platform.user_roles (
    user_id     TEXT NOT NULL REFERENCES platform.users(id) ON DELETE CASCADE,
    role_id     TEXT NOT NULL REFERENCES platform.roles(id) ON DELETE CASCADE,
    granted_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    granted_by  TEXT REFERENCES platform.users(id),
    PRIMARY KEY (user_id, role_id)
);

CREATE TABLE platform.api_keys (
    id           TEXT PRIMARY KEY,
    user_id      TEXT NOT NULL REFERENCES platform.users(id) ON DELETE CASCADE,
    prefix       TEXT NOT NULL UNIQUE,
    key_hash     TEXT NOT NULL,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_used_at TIMESTAMPTZ,
    expires_at   TIMESTAMPTZ NOT NULL,
    revoked_at   TIMESTAMPTZ
);

CREATE INDEX platform_api_keys_by_prefix ON platform.api_keys (prefix);
CREATE INDEX platform_api_keys_active
    ON platform.api_keys (user_id) WHERE revoked_at IS NULL;
