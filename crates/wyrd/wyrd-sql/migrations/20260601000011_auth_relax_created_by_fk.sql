-- Relax auth `created_by` FK to a generic issuer principal id.
--
-- Both wyrd.auth_api_keys and wyrd.auth_service_accounts declared an unnamed,
-- table-level composite FOREIGN KEY (data_tenant_id, created_by) referencing
-- wyrd.auth_users(data_tenant_id, id). That forced every issuer to be a human
-- user row. A service-account issuer's principal.id is not an auth_users row,
-- so it could not write `created_by` without violating the FK.
--
-- This forward-only migration drops ONLY those two auth_users-referencing FKs,
-- turning `created_by` into an FK-free issuer principal id modeled like the
-- existing audit_credential_issuance.issuer_principal_id. The columns stay
-- UUID NOT NULL. No backfill is needed: existing `created_by` values already
-- hold a valid auth_users.id, which remains a valid principal id.
--
-- The drop is name-agnostic: the original FKs were auto-named by Postgres, so
-- we discover them via pg_constraint filtered on the referenced table
-- (confrelid = wyrd.auth_users). That predicate is load-bearing — it guarantees
-- the sa_id -> auth_service_accounts and data_tenant_id -> platform.tenants FKs
-- are left intact and is robust to any historical rename.

DO $$
DECLARE
    constraint_name TEXT;
BEGIN
    FOR constraint_name IN
        SELECT conname
          FROM pg_constraint
         WHERE conrelid = 'wyrd.auth_service_accounts'::regclass
           AND confrelid = 'wyrd.auth_users'::regclass
           AND contype = 'f'
    LOOP
        EXECUTE format(
            'ALTER TABLE wyrd.auth_service_accounts DROP CONSTRAINT %I',
            constraint_name
        );
    END LOOP;

    FOR constraint_name IN
        SELECT conname
          FROM pg_constraint
         WHERE conrelid = 'wyrd.auth_api_keys'::regclass
           AND confrelid = 'wyrd.auth_users'::regclass
           AND contype = 'f'
    LOOP
        EXECUTE format(
            'ALTER TABLE wyrd.auth_api_keys DROP CONSTRAINT %I',
            constraint_name
        );
    END LOOP;
END $$;
