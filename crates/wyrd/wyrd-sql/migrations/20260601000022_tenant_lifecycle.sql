-- Tenant provisioning lifecycle.
--
-- A tenant is not usable the moment its directory row exists: it needs an
-- administrative principal, that principal's grant, and its first credential
-- before anyone can configure it. The directory therefore distinguishes a
-- tenant being built from one that is ready, so no consumer can pick up a
-- half-provisioned tenant and treat it as live.
--
--   provisioning ──► active      (every required piece exists)
--                └─► failed      (provisioning did not complete)
--   active       ──► suspended   (frozen, state and grants intact)
--
-- `active` is retained rather than renamed to `ready`: every existing consumer
-- already filters on it, and renaming would be churn with no behavioural gain.

-- The original status check was auto-named by Postgres, so it is discovered by
-- the column it constrains rather than by a name this migration would guess.
DO $$
DECLARE
    doomed TEXT;
BEGIN
    FOR doomed IN
        SELECT conname
          FROM pg_constraint
         WHERE conrelid = 'platform.tenants'::regclass
           AND contype = 'c'
           AND pg_get_constraintdef(oid) LIKE '%status%'
    LOOP
        EXECUTE format('ALTER TABLE platform.tenants DROP CONSTRAINT %I', doomed);
    END LOOP;
END $$;

ALTER TABLE platform.tenants
    ADD CONSTRAINT tenants_status_check
    CHECK (status IN ('provisioning','active','suspended','failed','deleted'));

-- Provisioning progress is observable so a resumed attempt knows what is left
-- and an operator can see why a tenant never became usable.
ALTER TABLE platform.tenants
    ADD COLUMN provisioning_failed_reason TEXT;

ALTER TABLE platform.tenants
    ADD CONSTRAINT tenants_failure_reason_consistency
    CHECK (
        (status = 'failed' AND provisioning_failed_reason IS NOT NULL)
     OR (status <> 'failed' AND provisioning_failed_reason IS NULL)
    );

-- The live-tenant index already excludes everything that is not active, so a
-- provisioning or failed tenant is invisible to sweepers and directory reads
-- without any consumer change.

-- ---------------------------------------------------------------------------
-- Refresh tokens follow the tenant-scope principal kinds
-- ---------------------------------------------------------------------------
-- The refresh table is already principal-generic, but its kind check predates
-- the tenant administrative principal. Without this a tenant administrator can
-- hold a credential it can never exchange, which makes a freshly provisioned
-- tenant unusable.
DO $$
DECLARE
    doomed TEXT;
BEGIN
    FOR doomed IN
        SELECT conname
          FROM pg_constraint
         WHERE conrelid = 'wyrd.auth_refresh_tokens'::regclass
           AND contype = 'c'
           AND pg_get_constraintdef(oid) LIKE '%principal_kind%'
    LOOP
        EXECUTE format(
            'ALTER TABLE wyrd.auth_refresh_tokens DROP CONSTRAINT %I', doomed
        );
    END LOOP;
END $$;

ALTER TABLE wyrd.auth_refresh_tokens
    ADD CONSTRAINT auth_refresh_tokens_principal_kind_check
    CHECK (principal_kind IN ('tenant_admin','user','service','agent'));
