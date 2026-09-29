-- Internal verification-result writer: one SYSTEM principal per tenant.
--
-- Verification results are written through Bifrost by the server itself, not
-- by any registered workload. That writer is a tenant-plane principal with its
-- own stable identity so Gate, Scribe, and audit attribute every result row to
-- it rather than to a platform identity or a borrowed tenant principal. It
-- lives in the existing tenant machine-principal store and is:
--
--   * exactly one per tenant, created at provisioning and backfilled here;
--   * credentialless and role-free: the server mints its short-lived token
--     directly, scoped to one Verifier Card, so there is nothing to exchange;
--   * lifecycle-free: always `active`, never suspended or deleted through a
--     public surface, and bound to no Card.
--
-- Every statement is idempotent so the upgrade can be re-applied safely.

-- ---------------------------------------------------------------------------
-- Admit the `system` kind with no Card and no lifecycle
-- ---------------------------------------------------------------------------
ALTER TABLE wyrd.auth_service_accounts
    DROP CONSTRAINT IF EXISTS auth_service_accounts_principal_kind_check,
    ADD CONSTRAINT auth_service_accounts_principal_kind_check
    CHECK (principal_kind IN ('tenant_admin','service','agent','system'));

ALTER TABLE wyrd.auth_service_accounts
    DROP CONSTRAINT IF EXISTS auth_service_accounts_kind_card_check,
    ADD CONSTRAINT auth_service_accounts_kind_card_check
    CHECK (
        (principal_kind = 'agent'        AND card_kind = 'Agent')
        OR (principal_kind = 'service'   AND (card_kind IS NULL OR card_kind = 'Service'))
        OR (principal_kind = 'tenant_admin' AND card_kind IS NULL)
        OR (principal_kind = 'system'    AND card_kind IS NULL)
    );

ALTER TABLE wyrd.auth_service_accounts
    DROP CONSTRAINT IF EXISTS auth_service_accounts_system_active_check,
    ADD CONSTRAINT auth_service_accounts_system_active_check
    CHECK (principal_kind <> 'system' OR status = 'active');

CREATE UNIQUE INDEX IF NOT EXISTS auth_service_accounts_one_system
    ON wyrd.auth_service_accounts (data_tenant_id)
    WHERE principal_kind = 'system';

-- The SYSTEM writer's name is an internal label, not a tenant-chosen identity,
-- so it neither reserves the name nor collides with a tenant principal that
-- already uses it.
DROP INDEX IF EXISTS wyrd.auth_service_accounts_cardless_name;
CREATE UNIQUE INDEX auth_service_accounts_cardless_name
    ON wyrd.auth_service_accounts (data_tenant_id, name)
    WHERE card_uid IS NULL AND principal_kind <> 'system';

-- ---------------------------------------------------------------------------
-- The one provisioning owner
-- ---------------------------------------------------------------------------
-- Creates the current tenant's SYSTEM principal when absent and returns its
-- stable id either way. Tenant scope is the forced RLS setting
-- `app.current_tenant`; the partial unique index makes concurrent callers
-- converge on one row. The id is a UUIDv7: a millisecond Unix timestamp over a
-- random v4 body, with the version nibble raised from 4 to 7.
CREATE OR REPLACE FUNCTION wyrd.provision_system_principal() RETURNS uuid
LANGUAGE plpgsql VOLATILE AS $$
DECLARE
    candidate uuid := encode(
        set_bit(set_bit(
            overlay(uuid_send(gen_random_uuid())
                placing substring(
                    int8send(floor(extract(epoch FROM clock_timestamp()) * 1000)::bigint)
                    FROM 3)
                FROM 1 FOR 6),
            52, 1), 53, 1),
        'hex')::uuid;
    provisioned uuid;
BEGIN
    INSERT INTO wyrd.auth_service_accounts (
        id, data_tenant_id, principal_kind, name, description, status, created_by
    ) VALUES (
        candidate, wyrd.current_tenant(), 'system', 'verification-results-writer',
        'Internal verification-result writer', 'active', candidate
    )
    ON CONFLICT (data_tenant_id) WHERE principal_kind = 'system' DO NOTHING;

    SELECT id INTO STRICT provisioned
      FROM wyrd.auth_service_accounts
     WHERE data_tenant_id = wyrd.current_tenant()
       AND principal_kind = 'system';
    RETURN provisioned;
END $$;

REVOKE ALL ON FUNCTION wyrd.provision_system_principal() FROM PUBLIC;
GRANT EXECUTE ON FUNCTION wyrd.provision_system_principal() TO wyrd_app, wyrd_platform_admin;

-- ---------------------------------------------------------------------------
-- Backfill every existing data tenant
--
-- The `wyrd-system` sentinel seeded by 20260601000015 is the platform plane's
-- audit owner, not a data tenant: it writes no verification results and is
-- never provisioned through tenant administration, so it gets no writer.
-- ---------------------------------------------------------------------------
DO $$
DECLARE
    tenant uuid;
BEGIN
    FOR tenant IN
        SELECT data_tenant_id FROM platform.tenants
         WHERE data_tenant_id <> '00000000-0000-7000-8000-000000000000'::uuid
    LOOP
        PERFORM set_config('app.current_tenant', tenant::text, true);
        PERFORM wyrd.provision_system_principal();
    END LOOP;
END $$;
