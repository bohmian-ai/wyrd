-- Durable per-table publication revisions for Oracle source acquisition.
--
-- Oracle reads a fresh cut with each table's publication revision, captures
-- the live Scribe sources, then reads the revisions again. An unchanged
-- revision proves no file was published between the two reads, so the cut
-- and the captures neither lose nor repeat a row. The revision advances in the
-- same transaction that inserts a vala.file_list row; an identical replay
-- inserts nothing (ON CONFLICT DO NOTHING fires no row trigger) and so invents
-- no publication. A table with no row has revision 0.
--
-- The revisions live beside, not on, vala.bifrost_table_maintenance_authority:
-- destructive Forge preparation holds that row FOR NO KEY UPDATE, and a
-- publication must not wait behind it.
--
-- The notification cut cache this replaces could not validate a read, so its
-- trigger and channel are dropped.

DROP TRIGGER file_list_notify_bifrost_cut ON vala.file_list;
DROP FUNCTION vala.notify_bifrost_cut();

CREATE TABLE vala.bifrost_table_publication_revisions (
    data_tenant_id       uuid   NOT NULL REFERENCES platform.tenants(data_tenant_id),
    namespace_name       text   NOT NULL CHECK (btrim(namespace_name) <> ''),
    table_name           text   NOT NULL CHECK (btrim(table_name) <> ''),
    publication_revision bigint NOT NULL CHECK (publication_revision > 0),
    PRIMARY KEY (data_tenant_id, namespace_name, table_name)
);

ALTER TABLE vala.bifrost_table_publication_revisions ENABLE ROW LEVEL SECURITY;
ALTER TABLE vala.bifrost_table_publication_revisions FORCE  ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON vala.bifrost_table_publication_revisions
    USING      (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());
CREATE POLICY operator_access ON vala.bifrost_table_publication_revisions TO CURRENT_USER
    USING (wyrd.operator_session()) WITH CHECK (wyrd.operator_session());

-- Advances the inserted row's table revision inside the inserting transaction.
-- It runs with the inserter's privileges, so RLS bounds it like the insert.
CREATE FUNCTION vala.advance_bifrost_publication_revision() RETURNS trigger
LANGUAGE plpgsql
SET search_path = ''
AS $$
BEGIN
    INSERT INTO vala.bifrost_table_publication_revisions AS r
        (data_tenant_id, namespace_name, table_name, publication_revision)
    VALUES (NEW.data_tenant_id, NEW.namespace, NEW.table_name, 1)
    ON CONFLICT (data_tenant_id, namespace_name, table_name)
    DO UPDATE SET publication_revision = r.publication_revision + 1;
    RETURN NULL;
END
$$;

CREATE TRIGGER file_list_advance_publication_revision
    AFTER INSERT ON vala.file_list
    FOR EACH ROW EXECUTE FUNCTION vala.advance_bifrost_publication_revision();

-- The cut now also returns each table's publication revision, read in the
-- same statement snapshot as its pointer and hot candidates. Everything else
-- is unchanged from 20261009000000_bifrost_snapshot_retention.sql.
DROP FUNCTION vala.oracle_read_table_cut(uuid, jsonb);

CREATE FUNCTION vala.oracle_read_table_cut(
    p_data_tenant_id uuid,
    p_tables jsonb
) RETURNS TABLE (
    ordinal integer,
    table_uid bytea,
    namespace_name text,
    table_name text,
    metadata_location text,
    publication_revision bigint,
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
           COALESCE(p.publication_revision, 0),
           f.id, f.data_tenant_id, f.file_path, f.file_ordinal, f.file_checksum, f.file_size,
           f.row_count, f.min_event_time, f.max_event_time, f.partition_granularity,
           f.partition_start, f.compacted, f.committed_snapshot_id,
           f.forge_publication_operation_id, f.node_id, f.writer_epoch, f.wal_lsn_min,
           f.wal_lsn_max, f.created_at
      FROM unnest(v_ordinals, v_uids, v_namespaces, v_tables, v_locations)
           AS u(ordinal, table_uid, namespace_name, table_name, metadata_location)
      LEFT JOIN vala.bifrost_table_publication_revisions p
        ON p.data_tenant_id = p_data_tenant_id
       AND p.namespace_name = u.namespace_name
       AND p.table_name = u.table_name
      LEFT JOIN vala.file_list f
        ON f.namespace = u.namespace_name
       AND f.table_name = u.table_name
       AND f.committed_snapshot_id IS NULL
     ORDER BY u.ordinal, f.partition_granularity, f.partition_start, f.created_at,
              f.file_ordinal, f.id;
END
$$;

-- Returns each requested table's current publication revision, in request
-- order, for Oracle's validation of an acquired selection. It binds its
-- tenant exactly as oracle_read_table_cut does and runs as one autocommit
-- statement. A table with no published file has revision 0; registration is
-- not rechecked here because the cut already proved it.
CREATE FUNCTION vala.oracle_read_publication_revisions(
    p_data_tenant_id uuid,
    p_tables jsonb
) RETURNS TABLE (
    ordinal integer,
    publication_revision bigint
)
LANGUAGE plpgsql
STABLE
SECURITY INVOKER
SET search_path = ''
AS $$
BEGIN
    IF pg_catalog.current_setting('app.current_tenant', true)
           NOT IN ('', p_data_tenant_id::text) THEN
        RAISE EXCEPTION 'Oracle revision tenant differs from the bound tenant'
            USING ERRCODE = 'invalid_parameter_value';
    END IF;
    PERFORM pg_catalog.set_config('app.current_tenant', p_data_tenant_id::text, true);

    IF pg_catalog.jsonb_typeof(p_tables) IS DISTINCT FROM 'array'
       OR pg_catalog.jsonb_array_length(p_tables) = 0 THEN
        RAISE EXCEPTION 'invalid Oracle revision request'
            USING ERRCODE = 'invalid_parameter_value';
    END IF;

    RETURN QUERY
    SELECT q.ordinal, COALESCE(p.publication_revision, 0)
      FROM (
        SELECT pg_catalog.min(r.position)::integer AS ordinal,
               r.entry->>'namespace' AS namespace_name,
               r.entry->>'table' AS table_name
          FROM pg_catalog.jsonb_array_elements(p_tables) WITH ORDINALITY AS r(entry, position)
         GROUP BY r.entry->>'namespace', r.entry->>'table'
      ) q
      LEFT JOIN vala.bifrost_table_publication_revisions p
        ON p.data_tenant_id = p_data_tenant_id
       AND p.namespace_name = q.namespace_name
       AND p.table_name = q.table_name
     ORDER BY q.ordinal;
END
$$;
