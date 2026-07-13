-- Stage-5 (Task E): admit the cross-table-derivation maintenance lease
-- namespace on vala.maintenance_leases.
--
-- The cross-table-derivation worker (genai-from-spans) holds a maintenance
-- lease per derivation, named `cross_table_derivation:{uid-hex}` and fenced by
-- the shared vala.maintenance_fencing_seq like every other maintenance lease.
--
-- The maintenance_leases table shipped without a lease-key format CHECK (slice
-- 01 deliberately deferred it while namespaces were still being introduced).
-- Now that the live namespaces are stable, this migration adds a namespace
-- whitelist CHECK and WIDENS it to admit `cross_table_derivation`. The whitelist
-- covers every namespace already in use — `table_writer`, `compaction`,
-- `snapshot_expiry`, `orphan_files` — so no existing lease key is rejected; it
-- is a strict widening. New namespaces widen this CHECK in their own migration.
--
-- The namespace is the substring before the first ':'. Keys are `namespace:{...}`.

ALTER TABLE vala.maintenance_leases
    ADD CONSTRAINT maintenance_leases_lease_key_namespace_check
        CHECK (
            split_part(lease_key, ':', 1) IN (
                'table_writer',
                'compaction',
                'snapshot_expiry',
                'orphan_files',
                'cross_table_derivation'
            )
        );
