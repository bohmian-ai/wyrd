-- Let the internal verification-result writer appear in the canonical audit
-- chain.
--
-- Each tenant owns one credentialless `system` principal whose only purpose is
-- writing the verification result tables through Bifrost. Gate records exactly
-- one `bifrost_record:write` decision for every admission, and audit is
-- fail-closed: a kind this CHECK refuses would turn every SYSTEM write into an
-- audit-unavailable refusal.
--
-- Widening the accepted set is the whole fix. The chain's hash covers the kind
-- as text, so existing entries are unaffected and no rewrite is needed.
ALTER TABLE vala.audit_staging DROP CONSTRAINT audit_staging_principal_kind_check;
ALTER TABLE vala.audit_staging ADD CONSTRAINT audit_staging_principal_kind_check
    CHECK (principal_kind IN ('global_admin', 'tenant_admin', 'user', 'service', 'agent', 'system'));
