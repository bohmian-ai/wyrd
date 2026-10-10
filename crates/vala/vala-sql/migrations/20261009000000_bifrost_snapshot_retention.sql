-- Readers are protected by a snapshot retention window, not tracked.
--
-- An Oracle query no longer records active table reads. Forge instead keeps
-- every replaced snapshot for a retention window measured from when it first
-- observed the snapshot as no longer current, and the window is never shorter
-- than the maximum query deadline. A query pins a snapshot only while it is
-- current, so its snapshot cannot expire before the query's deadline.

DROP FUNCTION vala.oracle_acquire_table_cut(uuid, uuid, bigint, bigint, jsonb);
DROP TABLE vala.oracle_active_table_reads;

-- ---------------------------------------------------------------------------
-- Read-only cut
-- ---------------------------------------------------------------------------

-- Returns one query's complete cut in one read-only call: every requested
-- table's registration, its catalog pointer, and its unresolved file_list
-- candidates. It runs with the caller's privileges, so RLS bounds every row;
-- only the pointer comes from the narrow definer
-- oracle_catalog_metadata_location. It takes no lock and writes nothing.
--
-- The function binds p_data_tenant_id as the transaction-local RLS tenant
-- itself, so a caller runs it as one autocommit statement — one round trip,
-- no BEGIN or COMMIT. The binding ends with that statement's implicit
-- transaction. Inside a transaction that already bound a different tenant it
-- refuses rather than rebinding. The setting is not database state, so the
-- function stays STABLE.
--
-- p_tables is one ordered JSON array of {"namespace": ..., "table": ...}. A
-- repeated table keeps its first position and yields one result. A missing
-- registration or pointer raises no_data_found. STABLE gives every statement
-- the one snapshot of the calling statement.
--
-- Result rows: one per (table, hot candidate), or one row with every hot
-- column NULL for a table with no candidates.
CREATE FUNCTION vala.oracle_read_table_cut(
    p_data_tenant_id uuid,
    p_tables jsonb
) RETURNS TABLE (
    ordinal integer,
    table_uid bytea,
    namespace_name text,
    table_name text,
    metadata_location text,
    id uuid,
    data_tenant_id uuid,
    file_path text,
    file_ordinal smallint,
    file_checksum text,
    file_size bigint,
    row_count bigint,
    min_event_time timestamptz,
    max_event_time timestamptz,
    partition_granularity text,
    partition_start timestamptz,
    compacted boolean,
    committed_snapshot_id bigint,
    forge_publication_operation_id uuid,
    node_id uuid,
    writer_epoch bigint,
    wal_lsn_min bigint,
    wal_lsn_max bigint,
    created_at timestamptz
)
LANGUAGE plpgsql
STABLE
SECURITY INVOKER
SET search_path = ''
AS $$
DECLARE
    v_requested integer;
    v_ordinals integer[];
    v_uids bytea[];
    v_namespaces text[];
    v_tables text[];
    v_locations text[];
BEGIN
    IF pg_catalog.current_setting('app.current_tenant', true)
           NOT IN ('', p_data_tenant_id::text) THEN
        RAISE EXCEPTION 'Oracle cut tenant differs from the bound tenant'
            USING ERRCODE = 'invalid_parameter_value';
    END IF;
    PERFORM pg_catalog.set_config('app.current_tenant', p_data_tenant_id::text, true);

    IF pg_catalog.jsonb_typeof(p_tables) IS DISTINCT FROM 'array'
       OR pg_catalog.jsonb_array_length(p_tables) = 0 THEN
        RAISE EXCEPTION 'invalid Oracle cut request'
            USING ERRCODE = 'invalid_parameter_value';
    END IF;

    SELECT pg_catalog.count(DISTINCT (r.entry->>'namespace', r.entry->>'table'))::integer
      INTO v_requested
      FROM pg_catalog.jsonb_array_elements(p_tables) AS r(entry);

    SELECT pg_catalog.array_agg(q.ordinal ORDER BY q.ordinal),
           pg_catalog.array_agg(a.table_uid ORDER BY q.ordinal),
           pg_catalog.array_agg(a.namespace_name ORDER BY q.ordinal),
           pg_catalog.array_agg(a.table_name ORDER BY q.ordinal),
           pg_catalog.array_agg(
               vala.oracle_catalog_metadata_location(a.namespace_name, a.table_name)
               ORDER BY q.ordinal)
      INTO v_ordinals, v_uids, v_namespaces, v_tables, v_locations
      FROM (
        SELECT pg_catalog.min(r.position)::integer AS ordinal,
               r.entry->>'namespace' AS namespace_name,
               r.entry->>'table' AS table_name
          FROM pg_catalog.jsonb_array_elements(p_tables) WITH ORDINALITY AS r(entry, position)
         GROUP BY r.entry->>'namespace', r.entry->>'table'
      ) q
      JOIN vala.bifrost_table_maintenance_authority a
        ON a.catalog_name = 'wyrd-redux'
       AND a.namespace_name = q.namespace_name
       AND a.table_name = q.table_name;
    IF COALESCE(pg_catalog.cardinality(v_uids), 0) <> v_requested THEN
        RAISE EXCEPTION 'Bifrost table is not registered for this tenant'
            USING ERRCODE = 'no_data_found';
    END IF;
    IF pg_catalog.array_position(v_locations, NULL) IS NOT NULL THEN
        RAISE EXCEPTION 'Bifrost table has no catalog pointer'
            USING ERRCODE = 'no_data_found';
    END IF;

    RETURN QUERY
    SELECT u.ordinal, u.table_uid, u.namespace_name, u.table_name, u.metadata_location,
           f.id, f.data_tenant_id, f.file_path, f.file_ordinal, f.file_checksum, f.file_size,
           f.row_count, f.min_event_time, f.max_event_time, f.partition_granularity,
           f.partition_start, f.compacted, f.committed_snapshot_id,
           f.forge_publication_operation_id, f.node_id, f.writer_epoch, f.wal_lsn_min,
           f.wal_lsn_max, f.created_at
      FROM unnest(v_ordinals, v_uids, v_namespaces, v_tables, v_locations)
           AS u(ordinal, table_uid, namespace_name, table_name, metadata_location)
      LEFT JOIN vala.file_list f
        ON f.namespace = u.namespace_name
       AND f.table_name = u.table_name
       AND f.committed_snapshot_id IS NULL
     ORDER BY u.ordinal, f.partition_granularity, f.partition_start, f.created_at,
              f.file_ordinal, f.id;
END
$$;

REVOKE ALL ON FUNCTION vala.oracle_read_table_cut(uuid, jsonb) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION vala.oracle_read_table_cut(uuid, jsonb) TO wyrd_app;

-- ---------------------------------------------------------------------------
-- Snapshot supersession observations
-- ---------------------------------------------------------------------------

-- One row is one retained snapshot Forge has observed as no longer current,
-- with the PostgreSQL time of the first observation. Snapshot expiry keeps a
-- snapshot until observed_at is at least the retention window old. The first
-- observation is never earlier than the real replacement, so the window only
-- errs toward retaining longer. Rows of snapshots the table no longer retains
-- are forgotten by the same statement that records new observations.
CREATE TABLE vala.forge_snapshot_supersessions (
    data_tenant_id uuid        NOT NULL REFERENCES platform.tenants(data_tenant_id),
    table_uid      bytea       NOT NULL CHECK (octet_length(table_uid) = 16),
    snapshot_id    bigint      NOT NULL,
    observed_at    timestamptz NOT NULL,
    PRIMARY KEY (data_tenant_id, table_uid, snapshot_id),
    FOREIGN KEY (data_tenant_id, table_uid)
        REFERENCES vala.bifrost_table_maintenance_authority(data_tenant_id, table_uid)
);

ALTER TABLE vala.forge_snapshot_supersessions ENABLE ROW LEVEL SECURITY;
ALTER TABLE vala.forge_snapshot_supersessions FORCE  ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON vala.forge_snapshot_supersessions
    USING      (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());

REVOKE ALL ON vala.forge_snapshot_supersessions FROM PUBLIC;
-- Forge gathers expiry protection roots on a tenant connection.
GRANT SELECT, INSERT, DELETE ON vala.forge_snapshot_supersessions TO wyrd_app;
