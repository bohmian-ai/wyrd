-- Wyrd platform schema bootstrap.
--
-- Cluster role creation is owned by infra bootstrap or embedded Postgres boot.
-- This migration validates those roles and grants object privileges only.

CREATE SCHEMA IF NOT EXISTS platform;
CREATE SCHEMA IF NOT EXISTS wyrd;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_roles
        WHERE rolname = 'wyrd_migrator' AND rolbypassrls = true
    ) THEN
        RAISE EXCEPTION
            'role wyrd_migrator missing or lacks BYPASSRLS - infra bootstrap incomplete';
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_roles
        WHERE rolname = 'wyrd_app' AND rolbypassrls = false
    ) THEN
        RAISE EXCEPTION
            'role wyrd_app missing or has BYPASSRLS (must NOT bypass) - infra bootstrap broken';
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_roles
        WHERE rolname = 'wyrd_platform_admin' AND rolbypassrls = true
    ) THEN
        RAISE EXCEPTION
            'role wyrd_platform_admin missing or lacks BYPASSRLS - infra bootstrap incomplete';
    END IF;
END $$;

GRANT USAGE ON SCHEMA platform, wyrd TO wyrd_app, wyrd_platform_admin;

ALTER DEFAULT PRIVILEGES FOR ROLE wyrd_migrator IN SCHEMA wyrd
    GRANT SELECT, INSERT, UPDATE, DELETE ON TABLES TO wyrd_app;
ALTER DEFAULT PRIVILEGES FOR ROLE wyrd_migrator IN SCHEMA wyrd
    GRANT USAGE, SELECT ON SEQUENCES TO wyrd_app;
ALTER DEFAULT PRIVILEGES FOR ROLE wyrd_migrator IN SCHEMA platform, wyrd
    GRANT SELECT, INSERT, UPDATE, DELETE ON TABLES TO wyrd_platform_admin;
ALTER DEFAULT PRIVILEGES FOR ROLE wyrd_migrator IN SCHEMA platform, wyrd
    GRANT USAGE, SELECT ON SEQUENCES TO wyrd_platform_admin;

CREATE FUNCTION wyrd.current_tenant() RETURNS uuid
LANGUAGE sql STABLE PARALLEL SAFE AS $$
    SELECT current_setting('app.current_tenant')::uuid
$$;

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

CREATE FUNCTION platform.resolve_tenant_by_slug(p_slug TEXT)
RETURNS uuid
LANGUAGE sql
SECURITY DEFINER
STABLE
SET search_path = pg_catalog, platform
AS $$
    SELECT data_tenant_id FROM platform.tenants
    WHERE slug = p_slug
      AND status = 'active'
      AND deleted_at IS NULL
$$;

REVOKE EXECUTE ON FUNCTION platform.resolve_tenant_by_slug(TEXT) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION platform.resolve_tenant_by_slug(TEXT) TO wyrd_app;
GRANT EXECUTE ON FUNCTION platform.resolve_tenant_by_slug(TEXT) TO wyrd_platform_admin;

CREATE TABLE platform.users (
    id              TEXT PRIMARY KEY,
    email           TEXT NOT NULL UNIQUE,
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

CREATE TABLE platform.audit_log (
    id              BIGSERIAL PRIMARY KEY,
    actor_user_id   TEXT REFERENCES platform.users(id),
    operation       TEXT NOT NULL,
    target          TEXT,
    before          JSONB,
    after           JSONB,
    request_id      TEXT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX platform_audit_log_by_actor
    ON platform.audit_log (actor_user_id, created_at DESC);
CREATE INDEX platform_audit_log_by_target
    ON platform.audit_log (target, created_at DESC);

GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA platform, wyrd TO wyrd_platform_admin;
GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA wyrd TO wyrd_app;
