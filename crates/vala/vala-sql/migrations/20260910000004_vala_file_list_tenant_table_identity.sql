-- Normalize vala.file_list to one organization-qualified physical table per
-- logical TableRef. The Redux Bifrost migration is greenfield-safe: old rows
-- must not be silently repartitioned or reinterpreted.

DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM vala.file_list) THEN
        RAISE EXCEPTION
            'vala.file_list must be empty before tenant-table identity migration';
    END IF;
END
$$;

DROP INDEX vala.file_list_group_idx;
DROP INDEX vala.file_list_live_tail_watermark_idx;

ALTER TABLE vala.file_list
    DROP COLUMN tenant_bucket;

CREATE INDEX file_list_group_idx
    ON vala.file_list (data_tenant_id, namespace, table_name, partition_day)
    WHERE NOT compacted;

CREATE INDEX file_list_live_tail_watermark_idx
    ON vala.file_list (data_tenant_id, namespace, table_name,
                       node_id, writer_epoch, wal_lsn_max);
