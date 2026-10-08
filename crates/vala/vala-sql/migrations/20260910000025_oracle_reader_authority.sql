-- Durable Oracle reader authority: table serialization and active table reads.
--
-- Forge maintenance runs in a different process from every reader, so an
-- admitted query has to leave a durable trace or destructive maintenance would
-- be deciding from age alone. Two owners carry that trace:
--
--   * bifrost_table_maintenance_authority is the single SQL serialization
--     boundary for one tenant-qualified table. Cut acquisition share-locks it
--     and destructive Forge preparation locks it FOR UPDATE, so the
--     admission-versus-destruction race has exactly one durable winner per
--     table without a global physical lock.
--   * oracle_active_table_reads holds one row per active query per table,
--     written by the same statement that returns the query's cut. Destructive
--     Forge work refuses a table while any such row exists.

-- ---------------------------------------------------------------------------
-- Per-table serialization boundary
-- ---------------------------------------------------------------------------

CREATE TABLE vala.bifrost_table_maintenance_authority (
    data_tenant_id   uuid  NOT NULL REFERENCES platform.tenants(data_tenant_id),
    catalog_name     text  NOT NULL CHECK (catalog_name = 'wyrd-redux'),
    namespace_name   text  NOT NULL CHECK (btrim(namespace_name) <> ''),
    table_name       text  NOT NULL CHECK (btrim(table_name) <> ''),
    table_uid        bytea NOT NULL CHECK (octet_length(table_uid) = 16),
    PRIMARY KEY (data_tenant_id, catalog_name, namespace_name, table_name),
    UNIQUE (data_tenant_id, table_uid),
    FOREIGN KEY (data_tenant_id, table_uid)
        REFERENCES vala.bifrost_tables(data_tenant_id, table_uid)
);

-- Exactly one authority row per already-registered Bifrost table. The fqn is
-- `<namespace>.<table>` with the namespace itself dotted, so the table name is
-- the final segment and the namespace is everything before it. Table
-- registration inserts its own row from now on, which is why no caller ever
-- needs an advisory-lock fallback for a missing row.
INSERT INTO vala.bifrost_table_maintenance_authority
    (data_tenant_id, catalog_name, namespace_name, table_name, table_uid)
SELECT data_tenant_id,
       'wyrd-redux',
       left(fqn, length(fqn) - strpos(reverse(fqn), '.')),
       right(fqn, strpos(reverse(fqn), '.') - 1),
       table_uid
  FROM vala.bifrost_tables
 WHERE strpos(reverse(fqn), '.') > 1;

ALTER TABLE vala.bifrost_table_maintenance_authority ENABLE ROW LEVEL SECURITY;
ALTER TABLE vala.bifrost_table_maintenance_authority FORCE  ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON vala.bifrost_table_maintenance_authority
    USING      (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());

REVOKE ALL ON vala.bifrost_table_maintenance_authority FROM PUBLIC;
GRANT SELECT, INSERT, UPDATE, DELETE
    ON vala.bifrost_table_maintenance_authority TO wyrd_app;

-- ---------------------------------------------------------------------------
-- Active Oracle table reads
-- ---------------------------------------------------------------------------

-- One row is one active query reading one registered table. It is written by
-- the same statement that selects the query's catalog pointers and hot-file
-- candidates, under the table's maintenance-authority row, so a cut cannot be
-- returned without the row and destructive maintenance cannot run while it
-- exists. The row names the exact Oracle node fence that owns it; it carries
-- no snapshot, ancestry, epoch, or capacity state.
--
-- abandon_after is PostgreSQL time: statement_timestamp() plus the query's
-- remaining deadline, bound by Oracle at acquisition. A row is discarded early
-- only by its owner's release, which runs at terminal settlement or when the
-- owner drops the query. A row whose owner crashed before releasing stays
-- protective until abandon_after has passed, after which Forge discards it.
CREATE TABLE vala.oracle_active_table_reads (
    data_tenant_id uuid   NOT NULL REFERENCES platform.tenants(data_tenant_id),
    query_id       uuid   NOT NULL,
    table_uid      bytea  NOT NULL CHECK (octet_length(table_uid) = 16),
    catalog_name   text   NOT NULL CHECK (catalog_name = 'wyrd-redux'),
    namespace_name text   NOT NULL CHECK (btrim(namespace_name) <> ''),
    table_name     text   NOT NULL CHECK (btrim(table_name) <> ''),
    node_id        uuid   NOT NULL,
    fencing_token  bigint NOT NULL CHECK (fencing_token > 0),
    acquired_at    timestamptz NOT NULL,
    abandon_after  timestamptz NOT NULL,
    CHECK (abandon_after > acquired_at),
    PRIMARY KEY (data_tenant_id, query_id, table_uid),
    FOREIGN KEY (data_tenant_id, catalog_name, namespace_name, table_name)
        REFERENCES vala.bifrost_table_maintenance_authority
                   (data_tenant_id, catalog_name, namespace_name, table_name),
    FOREIGN KEY (data_tenant_id, table_uid)
        REFERENCES vala.bifrost_table_maintenance_authority(data_tenant_id, table_uid)
);

-- Destructive maintenance asks one question per table: does any read exist?
CREATE INDEX oracle_active_table_reads_table
    ON vala.oracle_active_table_reads (data_tenant_id, table_uid);

ALTER TABLE vala.oracle_active_table_reads ENABLE ROW LEVEL SECURITY;
ALTER TABLE vala.oracle_active_table_reads FORCE  ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON vala.oracle_active_table_reads
    USING      (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());

REVOKE ALL ON vala.oracle_active_table_reads FROM PUBLIC;
GRANT SELECT, INSERT, UPDATE, DELETE ON vala.oracle_active_table_reads TO wyrd_app;
-- Forge's fenced maintenance transactions run on the operator pool: they read
-- the table's active reads and discard only provably abandoned ones.
GRANT SELECT, DELETE ON vala.oracle_active_table_reads TO wyrd_platform_admin;
-- The fenced Forge expiration lifecycle runs on the operator pool and takes the
-- maintenance-authority row as its serialization lock against cut acquisition.
-- PostgreSQL requires UPDATE privilege to take a `FOR UPDATE` row lock, so the
-- grant is wider than the behavior: Forge only ever reads and locks this row,
-- and registration remains the sole writer.
GRANT SELECT, UPDATE ON vala.bifrost_table_maintenance_authority TO wyrd_platform_admin;
-- Expired-object cleanup retires a promoted object's terminal file_list row in
-- the same operator transaction that records the object's proven deletion.
GRANT DELETE ON vala.file_list TO wyrd_platform_admin;

-- ---------------------------------------------------------------------------
-- Tenant-derived catalog pointer
-- ---------------------------------------------------------------------------

-- The request role has no access to iceberg_catalog. This definer is its only
-- route to a pointer: it takes a canonical logical namespace (`vala.<segment>`)
-- and table name, derives the physical namespace from wyrd.current_tenant()
-- alone, and returns one metadata_location or NULL. No tenant, catalog, or
-- physical namespace is accepted from the caller and no catalog row escapes.
--
-- iceberg_tables is created at runtime by iceberg-catalog-sql, possibly after
-- migrations run. PL/pgSQL resolves the constant statement at first execution,
-- so creating this function does not require the relation to exist yet; an
-- absent relation is an absent pointer.
CREATE FUNCTION vala.oracle_catalog_metadata_location(
    p_namespace_name text,
    p_table_name text
) RETURNS text
LANGUAGE plpgsql
STABLE
SECURITY DEFINER
SET search_path = ''
AS $$
DECLARE
    v_tenant uuid := wyrd.current_tenant();
    v_segment text;
    v_location text;
    v_matches bigint;
BEGIN
    IF v_tenant IS NULL
       OR p_namespace_name IS NULL
       OR p_table_name IS NULL
       OR p_table_name = ''
       OR p_namespace_name !~ '^vala\.[^.]+$' THEN
        RETURN NULL;
    END IF;
    v_segment := pg_catalog.substr(p_namespace_name, 6);
    SELECT pg_catalog.max(metadata_location), pg_catalog.count(*)
      INTO v_location, v_matches
      FROM iceberg_catalog.iceberg_tables
     WHERE catalog_name = 'wyrd-redux'
       AND table_namespace = 'vala.tenants.' || v_tenant::text || '.' || v_segment
       AND table_name = p_table_name
       AND (iceberg_type = 'TABLE' OR iceberg_type IS NULL);
    IF v_matches <> 1 THEN
        RETURN NULL;
    END IF;
    RETURN v_location;
EXCEPTION
    WHEN undefined_table THEN
        RETURN NULL;
END
$$;

ALTER FUNCTION vala.oracle_catalog_metadata_location(text, text) OWNER TO wyrd_platform_admin;
REVOKE ALL ON FUNCTION vala.oracle_catalog_metadata_location(text, text) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION vala.oracle_catalog_metadata_location(text, text) TO wyrd_app;

-- ---------------------------------------------------------------------------
-- One-statement cut acquisition
-- ---------------------------------------------------------------------------

-- Acquires one query's complete cut and its active reads in one call. It runs
-- with the caller's privileges, so TenantConn RLS bounds every registration,
-- authority, hot-file, and active-read row it touches; only the pointer comes
-- from the narrow definer above.
--
-- p_tables is one ordered JSON array of {"namespace": ..., "table": ...}. A
-- repeated table keeps its first position and yields one row and one claim.
-- The durable order is: resolve every authority row, share-lock them in
-- primary-key order (Forge's destructive preparation takes the same rows FOR
-- UPDATE), refuse a table whose snapshot expiration is still unresolved, read
-- each pointer, record every active read, then return the tables with their
-- unresolved file_list candidates. A missing registration or pointer raises
-- no_data_found, so no partial set of reads can commit.
--
-- Forge resolves an expiration's claims before it surrenders the authority
-- row, except when the catalog commit's acceptance is still unknown at the
-- Forge lease bound. Claims seen under the share lock therefore name an
-- expiration whose pointer may still change, and the acquisition raises
-- object_not_in_prerequisite_state (55000) until reconciliation establishes
-- the stable pointer and deletes them. It never waits or polls. Each
-- statement in this VOLATILE function takes a fresh snapshot, so the pointer
-- and hot rows are read after the share lock is granted.
--
-- Result rows: one per (table, hot candidate), or one row with every hot
-- column NULL for a table with no candidates.
CREATE FUNCTION vala.oracle_acquire_table_cut(
    p_query_id uuid,
    p_node_id uuid,
    p_fencing_token bigint,
    p_deadline_ms bigint,
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
VOLATILE
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
    IF p_query_id IS NULL OR p_node_id IS NULL OR p_fencing_token IS NULL
       OR p_fencing_token <= 0 OR p_deadline_ms IS NULL OR p_deadline_ms <= 0
       OR pg_catalog.jsonb_typeof(p_tables) IS DISTINCT FROM 'array'
       OR pg_catalog.jsonb_array_length(p_tables) = 0 THEN
        RAISE EXCEPTION 'invalid Oracle cut acquisition request'
            USING ERRCODE = 'invalid_parameter_value';
    END IF;

    SELECT pg_catalog.count(DISTINCT (r.entry->>'namespace', r.entry->>'table'))::integer
      INTO v_requested
      FROM pg_catalog.jsonb_array_elements(p_tables) AS r(entry);

    SELECT pg_catalog.array_agg(q.ordinal ORDER BY q.ordinal),
           pg_catalog.array_agg(a.table_uid ORDER BY q.ordinal),
           pg_catalog.array_agg(a.namespace_name ORDER BY q.ordinal),
           pg_catalog.array_agg(a.table_name ORDER BY q.ordinal)
      INTO v_ordinals, v_uids, v_namespaces, v_tables
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

    PERFORM 1
       FROM vala.bifrost_table_maintenance_authority a
      WHERE a.catalog_name = 'wyrd-redux'
        AND a.table_uid = ANY (v_uids)
      ORDER BY a.data_tenant_id, a.catalog_name, a.namespace_name, a.table_name
        FOR SHARE OF a;

    IF EXISTS (
        SELECT 1
          FROM vala.forge_snapshot_expiration_claims c
         WHERE c.data_tenant_id = wyrd.current_tenant()
           AND c.table_uid = ANY (v_uids)
    ) THEN
        RAISE EXCEPTION 'a Bifrost table has an unresolved snapshot expiration'
            USING ERRCODE = 'object_not_in_prerequisite_state';
    END IF;

    SELECT pg_catalog.array_agg(
               vala.oracle_catalog_metadata_location(u.namespace_name, u.table_name)
               ORDER BY u.ordinal)
      INTO v_locations
      FROM unnest(v_ordinals, v_namespaces, v_tables)
           AS u(ordinal, namespace_name, table_name);
    IF pg_catalog.array_position(v_locations, NULL) IS NOT NULL THEN
        RAISE EXCEPTION 'Bifrost table has no catalog pointer'
            USING ERRCODE = 'no_data_found';
    END IF;

    INSERT INTO vala.oracle_active_table_reads
        (data_tenant_id, query_id, table_uid, catalog_name, namespace_name, table_name,
         node_id, fencing_token, acquired_at, abandon_after)
    SELECT wyrd.current_tenant(), p_query_id, u.table_uid, 'wyrd-redux', u.namespace_name,
           u.table_name, p_node_id, p_fencing_token, pg_catalog.statement_timestamp(),
           pg_catalog.statement_timestamp() + p_deadline_ms * interval '1 millisecond'
      FROM unnest(v_uids, v_namespaces, v_tables)
           AS u(table_uid, namespace_name, table_name)
    ON CONFLICT ON CONSTRAINT oracle_active_table_reads_pkey DO UPDATE
       SET node_id = EXCLUDED.node_id,
           fencing_token = EXCLUDED.fencing_token,
           acquired_at = EXCLUDED.acquired_at,
           abandon_after = EXCLUDED.abandon_after;

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

REVOKE ALL ON FUNCTION vala.oracle_acquire_table_cut(uuid, uuid, bigint, bigint, jsonb) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION vala.oracle_acquire_table_cut(uuid, uuid, bigint, bigint, jsonb) TO wyrd_app;

-- ---------------------------------------------------------------------------
-- Forge snapshot-expiration claims
-- ---------------------------------------------------------------------------

-- A claim row says "an expiration operation has already selected this
-- snapshot and has not resolved". It is written while the table's
-- bifrost_table_maintenance_authority row is held FOR UPDATE and no active
-- table read exists, so the read-versus-destroy race has exactly one durable
-- winner per tenant-qualified table.
--
-- Only unresolved claims live here: settlement and reset delete the operation's
-- rows in the same transaction that closes it, and the cascade from
-- forge_operation_state means an operation can never outlive its claims in the
-- other direction either.
--
-- Claim rows are immutable. They preserve the identity the selection was
-- prepared under — task, attempt, preparing worker, and that worker's table
-- lease key and fencing token — as historical evidence. Correcting a
-- preparation means deleting these rows and preparing a new operation, never
-- updating one, which is why wyrd_platform_admin holds INSERT and DELETE but
-- no UPDATE. A successor that takes the task over settles under its own current
-- ownership and its own newly acquired fence; it does not rewrite these rows.
CREATE TABLE vala.forge_snapshot_expiration_claims (
    data_tenant_id      uuid   NOT NULL REFERENCES platform.tenants(data_tenant_id),
    resource            text   NOT NULL,
    family              text   NOT NULL CHECK (family = 'snapshot_expire'),
    operation_id        uuid   NOT NULL,
    snapshot_id         bigint NOT NULL,
    task_id             uuid   NOT NULL REFERENCES vala.forge_tasks(task_id),
    attempt_id          uuid   NOT NULL,
    worker_id           uuid   NOT NULL,
    lease_key           text   NOT NULL CHECK (btrim(lease_key) <> ''),
    lease_fencing_token bigint NOT NULL CHECK (lease_fencing_token > 0),
    table_uid           bytea  NOT NULL CHECK (octet_length(table_uid) = 16),
    catalog_name        text   NOT NULL CHECK (catalog_name = 'wyrd-redux'),
    namespace_name      text   NOT NULL CHECK (btrim(namespace_name) <> ''),
    table_name          text   NOT NULL CHECK (btrim(table_name) <> ''),
    table_uuid          uuid   NOT NULL,
    PRIMARY KEY (data_tenant_id, resource, family, operation_id, snapshot_id),
    FOREIGN KEY (data_tenant_id, resource, family, operation_id)
        REFERENCES vala.forge_operation_state
                   (data_tenant_id, resource, family, operation_id)
        ON DELETE CASCADE,
    FOREIGN KEY (data_tenant_id, table_uid)
        REFERENCES vala.bifrost_table_maintenance_authority(data_tenant_id, table_uid)
);

-- Admission asks one question: is this exact snapshot on this exact table
-- already claimed? Reconciliation asks the other: which operation do this
-- task's claims belong to? Neither may become a scan.
CREATE INDEX forge_snapshot_expiration_claims_admission
    ON vala.forge_snapshot_expiration_claims (data_tenant_id, table_uid, snapshot_id);
CREATE INDEX forge_snapshot_expiration_claims_task
    ON vala.forge_snapshot_expiration_claims (data_tenant_id, task_id);

ALTER TABLE vala.forge_snapshot_expiration_claims ENABLE ROW LEVEL SECURITY;
ALTER TABLE vala.forge_snapshot_expiration_claims FORCE  ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON vala.forge_snapshot_expiration_claims
    USING      (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());

REVOKE ALL ON vala.forge_snapshot_expiration_claims FROM PUBLIC;
-- Oracle admission reads the claim index from a tenant connection; only the
-- fenced Forge lifecycle owner, which runs on the operator pool, may write one.
GRANT SELECT ON vala.forge_snapshot_expiration_claims TO wyrd_app;
GRANT SELECT, INSERT, DELETE
    ON vala.forge_snapshot_expiration_claims TO wyrd_platform_admin;

-- ---------------------------------------------------------------------------
-- Expired-cleanup handoff identity
-- ---------------------------------------------------------------------------

-- One cleanup task per succeeded snapshot-expiration source, as a database
-- invariant rather than a scheduler convention. The source task id is the
-- globally unique replay identity of the handoff, so a second enqueue for the
-- same source cannot create a second physical deletion owner even if two
-- schedulers race or a demand is replanned. Retention also reads this index:
-- a succeeded expiration whose candidates no cleanup plan references yet is
-- held back from terminal pruning by a NOT EXISTS against exactly this key.
CREATE UNIQUE INDEX forge_tasks_expired_cleanup_source ON vala.forge_tasks
 ((plan #>> '{parameters,source_task_id}'))
 WHERE strategy = 'expired_cleanup';
