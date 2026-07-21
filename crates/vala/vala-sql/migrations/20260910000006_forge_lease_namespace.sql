-- Admit the Forge table-maintenance lease namespace alongside the existing
-- globally fenced maintenance workers.

ALTER TABLE vala.maintenance_leases
    DROP CONSTRAINT maintenance_leases_lease_key_namespace_check;

ALTER TABLE vala.maintenance_leases
    ADD CONSTRAINT maintenance_leases_lease_key_namespace_check
        CHECK (
            split_part(lease_key, ':', 1) IN (
                'table_writer',
                'compaction',
                'snapshot_expiry',
                'orphan_files',
                'cross_table_derivation',
                'forge'
            )
        );
