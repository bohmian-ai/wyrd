-- Let a tenant administrative principal appear in the audit chain.
--
-- `vala.audit_staging` was written when every tenant-plane principal was a user,
-- a service, or an agent. The administrative principals change adds
-- `tenant_admin` as a fourth, and its absence here is not a cosmetic gap: audit
-- is fail-closed, so an operation whose row cannot be appended is refused. A
-- tenant administrator could therefore perform no audited operation at all —
-- the CHECK rejected the insert and the caller saw an audit-unavailable 500.
--
-- Widening the accepted set is the whole fix. The chain's hash covers the kind
-- as text, so existing entries are unaffected and no rewrite is needed.
ALTER TABLE vala.audit_staging DROP CONSTRAINT audit_staging_principal_kind_check;
ALTER TABLE vala.audit_staging ADD CONSTRAINT audit_staging_principal_kind_check
    CHECK (principal_kind IN ('tenant_admin', 'user', 'service', 'agent'));
