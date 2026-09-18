-- Administrative principals: durable identity separated from credentials.
--
-- Two stores, one model. Tenant-scope principals stay in `wyrd.*` under row
-- level security, which remains the load-bearing tenant boundary. Platform-scope
-- principals live in `platform.*`, reachable only through the BYPASSRLS operator
-- role. A platform principal's absence of tenancy is structural — the table has
-- no tenant column — rather than a nullable column guarded by a check.
--
-- Principal kind fixes scope; grants fix authority. `global_admin` is the only
-- platform-scope kind. What a principal may do comes from its grants, never from
-- its kind.
--
-- This migration also retires the dormant `platform.users`, `platform.roles`,
-- `platform.user_roles`, and `platform.api_keys` objects. They predate the
-- principal model, are email/password shaped, have no principal/credential
-- separation, and were never reachable from a served surface.

-- ---------------------------------------------------------------------------
-- Retire the unreachable pre-principal platform identity model
-- ---------------------------------------------------------------------------
DROP TABLE IF EXISTS platform.api_keys;
DROP TABLE IF EXISTS platform.user_roles;
DROP TABLE IF EXISTS platform.roles;
DROP TABLE IF EXISTS platform.users;

-- ---------------------------------------------------------------------------
-- Platform-scope principals
-- ---------------------------------------------------------------------------
-- No `data_tenant_id` column exists here by design: a platform principal cannot
-- be given a tenant, so the constraint cannot be violated rather than merely
-- being checked. Only `wyrd_platform_admin` reaches this table.
CREATE TABLE platform.principals (
    id              UUID PRIMARY KEY,
    principal_kind  TEXT NOT NULL CHECK (principal_kind = 'global_admin'),
    name            TEXT NOT NULL UNIQUE,
    status          TEXT NOT NULL CHECK (status IN ('active','suspended','deleted')),
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX platform_principals_active
    ON platform.principals (id) WHERE status = 'active';

-- ---------------------------------------------------------------------------
-- Platform-scope credentials
-- ---------------------------------------------------------------------------
-- A principal may hold several credentials at once so rotation can overlap.
-- `prefix` is the non-secret lookup key; `secret_hash` is the Argon2 verifier.
-- The plaintext is returned once at creation and never persisted.
CREATE TABLE platform.credentials (
    id            UUID PRIMARY KEY,
    principal_id  UUID NOT NULL REFERENCES platform.principals(id) ON DELETE CASCADE,
    prefix        TEXT NOT NULL UNIQUE,
    secret_hash   TEXT NOT NULL,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at    TIMESTAMPTZ,
    revoked_at    TIMESTAMPTZ,
    last_used_at  TIMESTAMPTZ
);

CREATE INDEX platform_credentials_by_principal
    ON platform.credentials (principal_id);
CREATE INDEX platform_credentials_active
    ON platform.credentials (principal_id) WHERE revoked_at IS NULL;

-- ---------------------------------------------------------------------------
-- Platform-scope grants
-- ---------------------------------------------------------------------------
-- Authority lives here, not on the principal row, so revoking or rotating a
-- credential never touches what the principal may do. The permission payload is
-- the same JSONB projection `wyrd.auth_roles.permissions` uses, so one
-- permission vocabulary and one synchronous checker serve both planes.
CREATE TABLE platform.principal_grants (
    principal_id  UUID PRIMARY KEY REFERENCES platform.principals(id) ON DELETE CASCADE,
    permissions   JSONB NOT NULL,
    granted_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

GRANT SELECT, INSERT, UPDATE, DELETE
    ON platform.principals, platform.credentials, platform.principal_grants
    TO wyrd_platform_admin;

-- ---------------------------------------------------------------------------
-- Tenant-scope principals: Card binding becomes a property
-- ---------------------------------------------------------------------------
-- `wyrd.auth_service_accounts` already owns every tenant-scope machine
-- principal. It required a bound Card, which forced administrative identities to
-- invent one. Card columns become nullable and a check fixes which kinds may
-- carry them:
--
--   agent        -> Card required   (unchanged deployable identity)
--   service      -> Card optional   (deployed workload, or tenant automation)
--   tenant_admin -> Card forbidden  (the tenant's headless root of trust)
--
-- The existing UNIQUE (data_tenant_id, principal_kind, card_kind, card_uid) is
-- retained. Postgres treats NULLs as distinct there, so Card-free principals do
-- not collide with each other while `wyrd apply` re-application still resolves a
-- Card-bound principal to the same row.
-- `space` and `version` are projections of the bound Card, so they travel with
-- it rather than being invented for a principal that has none.
ALTER TABLE wyrd.auth_service_accounts
    ALTER COLUMN card_kind DROP NOT NULL,
    ALTER COLUMN card_uid  DROP NOT NULL,
    ALTER COLUMN card_ref  DROP NOT NULL,
    ALTER COLUMN space     DROP NOT NULL,
    ALTER COLUMN version   DROP NOT NULL;

-- The original kind and card-agreement checks were auto-named by Postgres, so
-- they are discovered by the columns they constrain rather than by a name this
-- migration would be guessing. Mirrors the name-agnostic drop in
-- 20260601000011_auth_relax_created_by_fk.sql.
DO $$
DECLARE
    doomed TEXT;
BEGIN
    FOR doomed IN
        SELECT conname
          FROM pg_constraint
         WHERE conrelid = 'wyrd.auth_service_accounts'::regclass
           AND contype = 'c'
           AND pg_get_constraintdef(oid) LIKE '%principal_kind%'
    LOOP
        EXECUTE format(
            'ALTER TABLE wyrd.auth_service_accounts DROP CONSTRAINT %I', doomed
        );
    END LOOP;
END $$;

ALTER TABLE wyrd.auth_service_accounts
    ADD CONSTRAINT auth_service_accounts_principal_kind_check
    CHECK (principal_kind IN ('tenant_admin','service','agent'));

-- Card columns now move together or not at all.
ALTER TABLE wyrd.auth_service_accounts
    ADD CONSTRAINT auth_service_accounts_card_binding_check
    CHECK (
        (card_kind IS NULL AND card_uid IS NULL AND card_ref IS NULL
            AND space IS NULL AND version IS NULL)
        OR (card_kind IS NOT NULL AND card_uid IS NOT NULL AND card_ref IS NOT NULL
            AND space IS NOT NULL AND version IS NOT NULL)
    );

ALTER TABLE wyrd.auth_service_accounts
    ADD CONSTRAINT auth_service_accounts_kind_card_check
    CHECK (
        (principal_kind = 'agent'        AND card_kind = 'Agent')
        OR (principal_kind = 'service'   AND (card_kind IS NULL OR card_kind = 'Service'))
        OR (principal_kind = 'tenant_admin' AND card_kind IS NULL)
    );

-- ---------------------------------------------------------------------------
-- Tenant-scope credentials become principal-generic
-- ---------------------------------------------------------------------------
-- `sa_id` named one kind of owner. The column now names the owning principal of
-- any tenant-scope kind the table above holds, matching the ownership shape
-- `wyrd.auth_refresh_tokens` already uses.
ALTER TABLE wyrd.auth_api_keys RENAME COLUMN sa_id TO principal_id;

ALTER INDEX IF EXISTS auth_api_keys_active RENAME TO auth_api_keys_active_by_principal;

-- A credential's expiry is optional: an administrative credential issued during
-- provisioning or recovery has no natural lifetime, while a workload key keeps
-- the bounded expiry its issuance path supplies.
ALTER TABLE wyrd.auth_api_keys ALTER COLUMN expires_at DROP NOT NULL;
