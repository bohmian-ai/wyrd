-- Re-point the vala.olap_commits composite FK from data_tenant_id to control_bind.
--
-- M02 (stage-5/02): a SystemShared physical table takes writes from many tenants
-- under a client-supplied batch_id, so two tenants can present the SAME batch_id.
-- The dedup discriminator must be the real DATA tenant (data_tenant_id, the RLS
-- bind), so identical client batch_ids never collide on a shared table. But a
-- SystemShared table registers exactly ONE vala.bifrost_tables row, keyed by the
-- registration anchor SYSTEM_OWNER; the writing data tenant has no bifrost_tables
-- row of its own. The prior FK (data_tenant_id, table_uid) -> bifrost_tables
-- therefore rejected any per-data-tenant olap_commits precommit for a SystemShared
-- table -- forcing every shared write onto SYSTEM_OWNER and reintroducing the exact
-- cross-tenant batch_id collision M02 exists to prevent.
--
-- Fix: anchor the composite FK on control_bind instead of data_tenant_id.
-- control_bind = scope.control_bind(tenant): SYSTEM_OWNER for SystemShared, the data
-- tenant for TenantOwned -- which is exactly the bifrost_tables registration key.
-- Postgres FK-integrity checks bypass RLS, so a real-data-tenant-bound insert whose
-- control_bind = SYSTEM_OWNER validates against the SYSTEM_OWNER registry row.
-- data_tenant_id remains the RLS anchor + dedup discriminator (still FK'd to
-- platform.tenants at the column level); control_bind becomes the registration
-- anchor / bifrost_tables FK.
--
-- Backward-compatible with existing rows: TenantOwned rows already have
-- control_bind == data_tenant_id, and legacy SystemShared rows have
-- control_bind == SYSTEM_OWNER; both reference a valid bifrost_tables row under the
-- new anchor, so the ADD CONSTRAINT validates without data migration.

ALTER TABLE vala.olap_commits
    DROP CONSTRAINT IF EXISTS olap_commits_data_tenant_id_table_uid_fkey;

ALTER TABLE vala.olap_commits
    ADD CONSTRAINT olap_commits_control_bind_table_uid_fkey
        FOREIGN KEY (control_bind, table_uid)
        REFERENCES vala.bifrost_tables (data_tenant_id, table_uid);
