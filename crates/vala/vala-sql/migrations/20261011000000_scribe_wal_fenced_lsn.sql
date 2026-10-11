-- WAL recovery ends a shard's chain at its first bad record only when no
-- acknowledged batch lies beyond it. A batch is acknowledged only after its
-- COMMIT is fsynced and its fence row commits, so the newest fence of one WAL
-- shard stream bounds what replay may discard. Fences are tenant-scoped; this
-- function returns only that fence's WAL position, never a tenant's rows.
--
-- Postgres refuses an ordinary owner a custom parameter in a function's SET
-- clause, so the body raises the operator flag transaction-locally and restores
-- the caller's value before returning; an error aborts the transaction, or the
-- enclosing savepoint, which reverts it too.
CREATE INDEX scribe_batch_commits_wal_stream_idx
    ON vala.scribe_batch_commits (wal_node_id, wal_writer_epoch, wal_shard_id, wal_lsn_max);

CREATE FUNCTION vala.scribe_wal_newest_fence(
    p_node_id uuid,
    p_writer_epoch bigint,
    p_shard_id smallint
) RETURNS TABLE (wal_segment_sequence bigint, wal_lsn_max bigint)
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog, vala
AS $$
DECLARE
    prior text := coalesce(current_setting('app.operator', true), '');
BEGIN
    PERFORM set_config('app.operator', 'on', true);
    RETURN QUERY
    SELECT c.wal_segment_sequence, c.wal_lsn_max
      FROM vala.scribe_batch_commits c
     WHERE c.wal_node_id = p_node_id
       AND c.wal_writer_epoch = p_writer_epoch
       AND c.wal_shard_id = p_shard_id
     ORDER BY c.wal_lsn_max DESC
     LIMIT 1;
    PERFORM set_config('app.operator', prior, true);
END;
$$;
