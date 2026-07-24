-- Retire the legacy storage access ledger after its replacement audit path.

DROP POLICY IF EXISTS admin_cross_tenant ON wyrd.storage_access_ledger;
DROP POLICY IF EXISTS tenant_isolation ON wyrd.storage_access_ledger;

DROP INDEX IF EXISTS wyrd.storage_access_ledger_tenant_created;
DROP INDEX IF EXISTS wyrd.storage_access_ledger_upload;

DROP TABLE IF EXISTS wyrd.storage_access_ledger;
DROP SEQUENCE IF EXISTS wyrd.storage_access_ledger_id_seq;
