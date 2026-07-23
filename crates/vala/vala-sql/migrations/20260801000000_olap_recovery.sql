-- Stage 2 OLAP recovery extension.
-- Extends olap_commits with writer-lease and fencing columns for recovery tracking.
-- Adds olap_recovery_events audit table.
-- Adds SECURITY DEFINER recovery routines owned by vala_recovery_owner.
-- NEVER creates roles here — only asserts existence and GRANTs to existing roles.

-- 1. Widen the state FSM to include 'aborted'.
ALTER TABLE vala.olap_commits DROP CONSTRAINT olap_commits_state_check;
ALTER TABLE vala.olap_commits ADD CONSTRAINT olap_commits_state_check
    CHECK (state IN ('precommit', 'committed', 'failed', 'aborted'));

-- 2. Writer-lease and recovery fencing columns.
ALTER TABLE vala.olap_commits
    ADD COLUMN writer_owner              uuid,
    ADD COLUMN writer_fencing_token      bigint,
    ADD COLUMN writer_lease_expires_at   timestamptz,
    ADD COLUMN writer_heartbeat_at       timestamptz,
    ADD COLUMN recovery_fencing_token    bigint,
    ADD COLUMN recovery_owner            uuid,
    ADD COLUMN recovery_attempts         integer NOT NULL DEFAULT 0,
    ADD COLUMN recovery_last_error       text,
    ADD COLUMN recovery_last_error_at    timestamptz;

-- 3. Monotonic fencing sequences. Epoch 0 is reserved ("never claimed"); any
--    non-zero token is a valid fence from a live writer or recovery sweep.
CREATE SEQUENCE IF NOT EXISTS vala.writer_fencing_seq;
CREATE SEQUENCE IF NOT EXISTS vala.recovery_fencing_seq;

GRANT USAGE ON SEQUENCE vala.writer_fencing_seq   TO wyrd_app;
GRANT USAGE ON SEQUENCE vala.recovery_fencing_seq TO vala_recovery_owner;

-- 4. Recovery audit table.
CREATE TABLE vala.olap_recovery_events (
    event_id        uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    data_tenant_id  uuid  NOT NULL REFERENCES platform.tenants(data_tenant_id),
    table_uid       bytea NOT NULL CHECK (octet_length(table_uid) = 16),
    batch_id        bytea NOT NULL CHECK (octet_length(batch_id) = 16),
    event_kind      text  NOT NULL DEFAULT 'recovery_decision'
                        CHECK (event_kind IN ('recovery_decision', 'fence_lost_after_append')),
    old_state       text NOT NULL,
    new_state       text NOT NULL,
    oracle_result   text,
    snapshot_id     bigint,
    recovery_owner  uuid,
    fencing_token   bigint,
    rolled_back     boolean,
    reason          text,
    error           text,
    recorded_at     timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX olap_recovery_events_by_batch
    ON vala.olap_recovery_events (data_tenant_id, table_uid, batch_id, recorded_at DESC);

ALTER TABLE vala.olap_recovery_events ENABLE ROW LEVEL SECURITY;
ALTER TABLE vala.olap_recovery_events FORCE  ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON vala.olap_recovery_events
    USING      (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());

GRANT INSERT ON vala.olap_recovery_events TO wyrd_app;

-- 5. Assert both recovery roles exist — roles.sql / db:setup-roles must provision
--    them before running migrations.
DO $$ BEGIN
  IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'vala_recovery_owner') THEN
    RAISE EXCEPTION 'vala_recovery_owner missing — run bootstrap/roles.sql / db:setup-roles';
  END IF;
  IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'vala_recovery') THEN
    RAISE EXCEPTION 'vala_recovery missing — run bootstrap/roles.sql / db:setup-roles';
  END IF;
END $$;

-- 6. Grant schema access and object access to vala_recovery_owner so the
--    SECURITY DEFINER functions execute correctly under that role's privileges.
--    USAGE on the schema is required in addition to table/sequence grants because
--    BYPASSRLS skips RLS policies but not privilege checks on the schema itself.
--    CREATE is required so that PostgreSQL 17 allows ALTER FUNCTION OWNER TO
--    vala_recovery_owner (PG17 enforces that the new owner has CREATE on the
--    function's schema). vala_sql::migrate() pre-commits this grant outside any
--    migration transaction so the ACL cache sees it during ALTER FUNCTION OWNER.
GRANT USAGE ON SCHEMA vala TO vala_recovery_owner, vala_recovery;
GRANT CREATE ON SCHEMA vala TO vala_recovery_owner;

GRANT SELECT ON vala.bifrost_tables TO vala_recovery_owner;
GRANT SELECT, UPDATE ON vala.olap_commits TO vala_recovery_owner;
GRANT INSERT ON vala.olap_recovery_events TO vala_recovery_owner;

-- 7. SECURITY DEFINER recovery routines. Each executes under vala_recovery_owner
--    (BYPASSRLS) so it can read/write across all tenants without RLS interference.

CREATE OR REPLACE FUNCTION vala.claim_stale_precommits(p_owner uuid, p_limit int)
RETURNS TABLE(
    data_tenant_id    uuid,
    table_uid         bytea,
    batch_id          bytea,
    fencing_token     bigint,
    fqn               text,
    namespace         text,
    name              text,
    recovery_attempts integer
)
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog, vala, platform
AS $$
BEGIN
    RETURN QUERY
    WITH candidates AS (
        SELECT c.data_tenant_id, c.table_uid, c.batch_id
          FROM vala.olap_commits c
         WHERE c.state = 'precommit'
           AND (c.writer_lease_expires_at IS NULL OR c.writer_lease_expires_at < now())
           AND c.recovery_fencing_token IS NULL
         ORDER BY c.precommit_at
         LIMIT p_limit
        FOR UPDATE SKIP LOCKED
    ),
    claimed AS (
        UPDATE vala.olap_commits c
           SET recovery_fencing_token = nextval('vala.recovery_fencing_seq'),
               recovery_owner         = p_owner
          FROM candidates
         WHERE c.data_tenant_id = candidates.data_tenant_id
           AND c.table_uid      = candidates.table_uid
           AND c.batch_id       = candidates.batch_id
        -- recovery_attempts is the accumulated count BEFORE this claim; returning
        -- it here lets the sweep detect stuck rows without a second (RLS-blocked)
        -- read of vala.olap_commits from the non-BYPASSRLS recovery role.
        RETURNING c.data_tenant_id, c.table_uid, c.batch_id,
                  c.recovery_fencing_token AS fencing_token, c.recovery_attempts
    )
    SELECT
        cl.data_tenant_id,
        cl.table_uid,
        cl.batch_id,
        cl.fencing_token,
        bt.fqn,
        split_part(bt.fqn, '.', 1) AS namespace,
        split_part(bt.fqn, '.', 2) AS name,
        cl.recovery_attempts
    FROM claimed cl
    JOIN vala.bifrost_tables bt
      ON bt.data_tenant_id = cl.data_tenant_id AND bt.table_uid = cl.table_uid;
END;
$$;
-- Grant EXECUTE while wyrd_migrator still owns the function (before owner transfer).
GRANT EXECUTE ON FUNCTION vala.claim_stale_precommits(uuid, int) TO vala_recovery;
ALTER FUNCTION vala.claim_stale_precommits(uuid, int) OWNER TO vala_recovery_owner;

CREATE OR REPLACE FUNCTION vala.finalize_recovered_committed(
    p_data_tenant_id uuid,
    p_table_uid   bytea,
    p_batch_id    bytea,
    p_snapshot_id bigint,
    p_token       bigint
)
RETURNS void
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog, vala, platform
AS $$
BEGIN
    UPDATE vala.olap_commits
       SET state        = 'committed',
           committed_at = now(),
           snapshot_id  = p_snapshot_id
     WHERE table_uid              = p_table_uid
       AND data_tenant_id         = p_data_tenant_id
       AND batch_id               = p_batch_id
       AND state                  = 'precommit'
       AND recovery_fencing_token = p_token;

    IF NOT FOUND THEN
        RETURN;
    END IF;

    INSERT INTO vala.olap_recovery_events
        (data_tenant_id, table_uid, batch_id, event_kind, old_state, new_state,
         oracle_result, snapshot_id, recovery_owner, fencing_token, recorded_at)
    SELECT data_tenant_id, p_table_uid, p_batch_id,
           'recovery_decision', 'precommit', 'committed',
           'snapshot_found', p_snapshot_id, recovery_owner, p_token, now()
      FROM vala.olap_commits
     WHERE data_tenant_id = p_data_tenant_id
       AND table_uid = p_table_uid AND batch_id = p_batch_id
       AND recovery_fencing_token = p_token;
END;
$$;
GRANT EXECUTE ON FUNCTION vala.finalize_recovered_committed(uuid, bytea, bytea, bigint, bigint) TO vala_recovery;
ALTER FUNCTION vala.finalize_recovered_committed(uuid, bytea, bytea, bigint, bigint) OWNER TO vala_recovery_owner;

CREATE OR REPLACE FUNCTION vala.finalize_recovered_aborted(
    p_data_tenant_id uuid,
    p_table_uid bytea,
    p_batch_id  bytea,
    p_token     bigint,
    p_reason    text
)
RETURNS void
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog, vala, platform
AS $$
BEGIN
    UPDATE vala.olap_commits
       SET state = 'aborted'
     WHERE table_uid              = p_table_uid
       AND data_tenant_id         = p_data_tenant_id
       AND batch_id               = p_batch_id
       AND state                  = 'precommit'
       AND recovery_fencing_token = p_token;

    IF NOT FOUND THEN
        RETURN;
    END IF;

    INSERT INTO vala.olap_recovery_events
        (data_tenant_id, table_uid, batch_id, event_kind, old_state, new_state,
         oracle_result, recovery_owner, fencing_token, reason, recorded_at)
    SELECT data_tenant_id, p_table_uid, p_batch_id,
           'recovery_decision', 'precommit', 'aborted',
           'snapshot_absent', recovery_owner, p_token, p_reason, now()
      FROM vala.olap_commits
     WHERE data_tenant_id = p_data_tenant_id
       AND table_uid = p_table_uid AND batch_id = p_batch_id
       AND recovery_fencing_token = p_token;
END;
$$;
GRANT EXECUTE ON FUNCTION vala.finalize_recovered_aborted(uuid, bytea, bytea, bigint, text) TO vala_recovery;
ALTER FUNCTION vala.finalize_recovered_aborted(uuid, bytea, bytea, bigint, text) OWNER TO vala_recovery_owner;

CREATE OR REPLACE FUNCTION vala.mark_recovery_scan_failed(
    p_data_tenant_id uuid,
    p_table_uid bytea,
    p_batch_id  bytea,
    p_token     bigint,
    p_error     text
)
RETURNS void
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog, vala, platform
AS $$
BEGIN
    -- Audit first, while recovery_owner / recovery_fencing_token are still
    -- populated. The UPDATE below clears the token so the row can be re-claimed.
    INSERT INTO vala.olap_recovery_events
        (data_tenant_id, table_uid, batch_id, event_kind, old_state, new_state,
         oracle_result, recovery_owner, fencing_token, error, recorded_at)
    SELECT data_tenant_id, p_table_uid, p_batch_id,
           'recovery_decision', 'precommit', 'precommit',
           'scan_failed', recovery_owner, p_token, p_error, now()
      FROM vala.olap_commits
     WHERE data_tenant_id = p_data_tenant_id
       AND table_uid = p_table_uid AND batch_id = p_batch_id
       AND recovery_fencing_token = p_token;

    -- Clear the fencing token/owner so claim_stale_precommits (which filters on
    -- recovery_fencing_token IS NULL) can re-claim this row on the next pass.
    UPDATE vala.olap_commits
       SET recovery_attempts      = recovery_attempts + 1,
           recovery_last_error    = p_error,
           recovery_last_error_at = now(),
           recovery_fencing_token = NULL,
           recovery_owner         = NULL
     WHERE data_tenant_id         = p_data_tenant_id
       AND table_uid              = p_table_uid
       AND batch_id               = p_batch_id
       AND state                  = 'precommit'
       AND recovery_fencing_token = p_token;
END;
$$;
GRANT EXECUTE ON FUNCTION vala.mark_recovery_scan_failed(uuid, bytea, bytea, bigint, text) TO vala_recovery;
ALTER FUNCTION vala.mark_recovery_scan_failed(uuid, bytea, bytea, bigint, text) OWNER TO vala_recovery_owner;

-- vala_recovery_owner only needs CREATE ON SCHEMA vala for the ALTER FUNCTION
-- OWNER transfers above. Revoke it now that all ownership transfers are done.
REVOKE CREATE ON SCHEMA vala FROM vala_recovery_owner;
