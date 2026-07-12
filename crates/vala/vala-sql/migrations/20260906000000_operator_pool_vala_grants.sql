-- T1a: OperatorPool cross-tenant access grants.
--
-- OperatorPool wraps the wyrd_platform_admin BYPASSRLS pool (see plan §4 / §9-T1a).
-- The audit reconciler enumerates shipped outbox rows across all tenants via
-- vala_sql::queries::relay::list_audit_tenant_ids, which runs a direct
-- `SELECT DISTINCT data_tenant_id FROM vala.audit_outbox WHERE shipped = true`
-- as wyrd_platform_admin. That role already has BYPASSRLS (asserted in
-- 20260601000000_platform.sql) but no vala-schema access, so the direct query
-- fails with 42501 without these grants. BYPASSRLS skips RLS policies but NOT
-- privilege checks — USAGE on the schema and SELECT on the table are still
-- required.
--
-- NEVER creates roles here — only asserts existence and GRANTs to existing roles.

DO $$ BEGIN
  IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'wyrd_platform_admin') THEN
    RAISE EXCEPTION 'wyrd_platform_admin missing — run bootstrap/roles.sql / db:setup-roles';
  END IF;
END $$;

GRANT USAGE ON SCHEMA vala TO wyrd_platform_admin;
GRANT SELECT ON vala.audit_outbox TO wyrd_platform_admin;
