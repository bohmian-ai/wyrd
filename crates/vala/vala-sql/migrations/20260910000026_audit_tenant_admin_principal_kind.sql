-- Let administrative principals appear in the canonical audit chain, and let
-- the platform boundary write it.
--
-- `vala.audit_staging` was written when every principal was a user, a service,
-- or an agent. The administrative principals change adds `tenant_admin` on the
-- tenant plane and `global_admin` on the platform plane, and their absence here
-- is not a cosmetic gap: audit is fail-closed, so an operation whose row cannot
-- be appended is refused. Either administrator could therefore perform no
-- audited operation at all — the CHECK rejected the insert and the caller saw
-- an audit-unavailable 500.
--
-- Widening the accepted set is the whole fix. The chain's hash covers the kind
-- as text, so existing entries are unaffected and no rewrite is needed.
ALTER TABLE vala.audit_staging DROP CONSTRAINT audit_staging_principal_kind_check;
ALTER TABLE vala.audit_staging ADD CONSTRAINT audit_staging_principal_kind_check
    CHECK (principal_kind IN ('global_admin', 'tenant_admin', 'user', 'service', 'agent'));

-- A platform-plane decision has no owning tenant, so it stages under the
-- `wyrd-system` sentinel through the operator role rather than through a
-- tenant's application connection. That is what keeps one canonical append and
-- one publisher: the platform's decision and its `platform.*` effect commit in
-- the same operator transaction, and the ordinary publisher drains the sentinel
-- tenant's staged rows into retained history like any other tenant's.
--
-- `append_audit` names `data_tenant_id` in every statement, so the row-level
-- security this role bypasses is not what bounds the write.
--
-- The grant is exactly what appending costs and nothing more. The chain head is
-- read and advanced; staged rows are only inserted. The operator therefore
-- still cannot read, amend, or remove any tenant's audit — a role that bypasses
-- row-level security must not be able to see across tenants, and appending a
-- decision never requires it.
GRANT SELECT, INSERT, UPDATE ON vala.audit_chain_head TO wyrd_platform_admin;
GRANT         INSERT          ON vala.audit_staging   TO wyrd_platform_admin;
