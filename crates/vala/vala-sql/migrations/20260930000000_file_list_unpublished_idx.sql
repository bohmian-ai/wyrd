-- Oracle's hot cut reads only files not yet committed to an Iceberg snapshot,
-- so its cost follows unpublished work rather than the table's whole history.
CREATE INDEX file_list_unpublished_idx
    ON vala.file_list (data_tenant_id, namespace, table_name)
    WHERE committed_snapshot_id IS NULL;
