-- Widen vala.olap_commits identity to (table_uid, control_bind, batch_id).
--
-- The prior PK (data_tenant_id, table_uid, batch_id) conflated the RLS bind
-- with CommitKey uniqueness.  The group-commit coordinator (stage-5/02) needs
-- cross-tenant batch dedup keyed on (table_uid, control_bind, batch_id):
--   - control_bind = data_tenant_id for TenantOwned tables
--   - control_bind = '00000000-0000-0000-0000-000000000000' for SystemShared tables
-- This is a logical identity that spans the two staging tenants and is stable
-- across recovery / relay re-flush.
--
-- Migration steps:
--   1. Add control_bind column (not-null, default = data_tenant_id so existing
--      rows are backfilled with the correct value).
--   2. Drop the old PK constraint.
--   3. Add the new PK.
--   4. Drop the old state index and recreate it over the new key.
--   5. Grant the column to wyrd_app (inherited from table-level grant in init).

-- Step 1 – Add control_bind, backfill from data_tenant_id.
ALTER TABLE vala.olap_commits
    ADD COLUMN IF NOT EXISTS control_bind UUID
        NOT NULL DEFAULT '00000000-0000-0000-0000-000000000000';

-- Backfill: TenantOwned rows already have control_bind == data_tenant_id.
-- SystemShared rows (data_tenant_id == SYSTEM_OWNER nil UUID) stay as-is.
-- For TenantOwned rows that pre-date this migration, set control_bind from data_tenant_id.
UPDATE vala.olap_commits
   SET control_bind = data_tenant_id
 WHERE data_tenant_id != '00000000-0000-0000-0000-000000000000';

-- Step 2 – Drop old PK.
ALTER TABLE vala.olap_commits DROP CONSTRAINT IF EXISTS olap_commits_pkey;

-- Step 3 – New PK on (data_tenant_id, table_uid, control_bind, batch_id).
-- data_tenant_id is kept as the first column so RLS policies (which filter on
-- wyrd.current_tenant() = data_tenant_id) remain efficiently anchored.
ALTER TABLE vala.olap_commits
    ADD CONSTRAINT olap_commits_pkey
        PRIMARY KEY (data_tenant_id, table_uid, control_bind, batch_id);

-- Step 4 – Recreate the state index over the new identity columns.
DROP INDEX IF EXISTS vala.olap_commits_state_idx;
CREATE INDEX olap_commits_state_idx
    ON vala.olap_commits (data_tenant_id, table_uid, control_bind, state)
    WHERE state IN ('precommit', 'failed');

-- Step 5 – No explicit column grant needed; table-level grants in the init
-- migration already cover SELECT/INSERT/UPDATE/DELETE to wyrd_app and the
-- recovery role.  The column is covered by those grants automatically.
