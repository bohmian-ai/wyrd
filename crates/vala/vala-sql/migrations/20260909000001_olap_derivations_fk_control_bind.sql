-- Fix olap_derivations FK anchor for SystemShared source tables.
--
-- The original FK was:
--   FOREIGN KEY (data_tenant_id, source_table_uid)
--       REFERENCES vala.bifrost_tables(data_tenant_id, table_uid)
--
-- That works for TenantOwned sources (bifrost_tables carries a per-tenant row
-- for each tenant that has data), but every SystemShared source (traces.spans,
-- traces.events, genai.*, metrics.points, logs.records) has ONE bifrost_tables
-- row anchored on SYSTEM_OWNER (the nil UUID). A tenant-scoped derivation
-- registration for such a source with `data_tenant_id = <real tenant>` violates
-- the FK because no `(real_tenant, spans_uid)` row exists in bifrost_tables.
--
-- Correct anchor is `control_bind`:
--   * SystemShared → control_bind = SYSTEM_OWNER (nil UUID) → matches SystemShared
--     bifrost_tables row.
--   * TenantOwned  → control_bind = data_tenant_id → matches TenantOwned
--     bifrost_tables row.
--
-- No data preservation concerns: the table has been in-flight only; any stale
-- rows registered before this fix are unreachable through the runtime (every
-- worker tick reregisters with control_bind = nil for the genai derivation).

ALTER TABLE vala.olap_derivations
    DROP CONSTRAINT olap_derivations_data_tenant_id_source_table_uid_fkey;

ALTER TABLE vala.olap_derivations
    ADD CONSTRAINT olap_derivations_control_bind_source_table_uid_fkey
    FOREIGN KEY (control_bind, source_table_uid)
    REFERENCES vala.bifrost_tables(data_tenant_id, table_uid);
