-- Replace materialized per-tenant ceilings with one canonical tenant default.

DELETE FROM vala.oracle_admission_policies
WHERE scope_kind = 'tenant';

ALTER TABLE vala.oracle_admission_policies
    DROP CONSTRAINT oracle_admission_policies_scope_kind_check,
    DROP CONSTRAINT oracle_admission_policies_check;

ALTER TABLE vala.oracle_admission_policies
    ADD CONSTRAINT oracle_admission_policies_scope_kind_check
        CHECK (scope_kind IN ('global', 'tenant_default')),
    ADD CONSTRAINT oracle_admission_policies_canonical_scope_check
        CHECK (data_tenant_id IS NULL);
