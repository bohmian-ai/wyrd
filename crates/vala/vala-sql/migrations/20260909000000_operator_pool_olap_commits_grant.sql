-- Grant OperatorPool (wyrd_platform_admin) SELECT on vala.olap_commits.
--
-- The genai derivation worker enumerates tenants with committed source batches
-- via list_source_commit_tenants (crates/vala/vala-sql/src/queries/olap_derivations.rs)
-- using the OperatorPool BYPASSRLS handle. wyrd_platform_admin was previously
-- granted access only to vala.audit_outbox (see 20260906000000), so every
-- NOTIFY-driven derivation tick failed with `permission denied for table
-- olap_commits`. BYPASSRLS skips RLS policies but NOT privilege checks.
--
-- NEVER creates roles here — only asserts existence and GRANTs to existing roles.

DO $$ BEGIN
  IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'wyrd_platform_admin') THEN
    RAISE EXCEPTION 'wyrd_platform_admin missing — run bootstrap/roles.sql / db:setup-roles';
  END IF;
END $$;

GRANT SELECT ON vala.olap_commits TO wyrd_platform_admin;
