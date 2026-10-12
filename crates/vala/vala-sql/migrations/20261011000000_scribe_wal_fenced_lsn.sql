-- WAL recovery ends a shard's chain at its first bad record only when no
-- acknowledged batch lies beyond it. A batch is acknowledged only after its
-- COMMIT is fsynced and its fence row commits, so the newest fence of one WAL
-- shard stream bounds what replay may discard. Fences are tenant-scoped; this
-- function returns only that fence's WAL position, never a tenant's rows.
CREATE INDEX scribe_batch_commits_wal_stream_idx
    ON vala.scribe_batch_commits (wal_node_id, wal_writer_epoch, wal_shard_id, wal_lsn_max);

CREATE FUNCTION vala.scribe_wal_newest_fence(
    p_node_id uuid,
    p_writer_epoch bigint,
    p_shard_id smallint
) RETURNS TABLE (wal_segment_sequence bigint, wal_lsn_max bigint)
LANGUAGE sql
STABLE
SECURITY DEFINER
SET search_path = pg_catalog, vala
AS $$
    SELECT wal_segment_sequence, wal_lsn_max
      FROM vala.scribe_batch_commits
     WHERE wal_node_id = p_node_id
       AND wal_writer_epoch = p_writer_epoch
       AND wal_shard_id = p_shard_id
     ORDER BY wal_lsn_max DESC
     LIMIT 1
$$;
