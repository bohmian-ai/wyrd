WITH cursor AS MATERIALIZED (
    SELECT last_tenant_id
    FROM vala.forge_worker_claim_state
    WHERE singleton
    FOR UPDATE
), claimable AS MATERIALIZED (
    SELECT t.*
    FROM vala.forge_tasks t
    WHERE t.state IN ('ready', 'retryable')
      AND t.ready_at <= statement_timestamp()
      AND t.next_eligible_at <= statement_timestamp()
      AND ($5::text[] IS NULL OR t.strategy = ANY($5::text[]))
      AND (
          NOT $6::bool
          OR (
              t.strategy IN ('expired_cleanup', 'orphan_cleanup')
              AND t.evidence IS NOT NULL
          )
      )
      AND NOT EXISTS (
          SELECT 1
          FROM vala.forge_tasks active
          WHERE active.data_tenant_id = t.data_tenant_id
            AND active.catalog_name = t.catalog_name
            AND active.namespace_name = t.namespace_name
            AND active.table_name = t.table_name
            AND active.state IN ('claimed', 'running', 'prepared')
      )
), eligible_tenants AS MATERIALIZED (
    SELECT t.data_tenant_id
    FROM claimable t
    CROSS JOIN cursor c
    WHERE (
        SELECT count(*)
        FROM vala.forge_tasks active
        WHERE active.data_tenant_id = t.data_tenant_id
          AND active.state IN ('claimed', 'running', 'prepared')
    ) < $2
    GROUP BY t.data_tenant_id, c.last_tenant_id
    ORDER BY (c.last_tenant_id IS NULL OR t.data_tenant_id > c.last_tenant_id) DESC,
             t.data_tenant_id
    LIMIT 1
), candidate AS MATERIALIZED (
    SELECT t.task_id, t.data_tenant_id,
           e.data_tenant_id AS execution_tenant_id
    FROM vala.forge_tasks t
    JOIN claimable q USING (task_id)
    JOIN eligible_tenants e ON e.data_tenant_id = t.data_tenant_id
    ORDER BY t.ready_at, t.task_id
    FOR UPDATE OF t SKIP LOCKED
    LIMIT 1
), claimed AS (
    UPDATE vala.forge_tasks t
    SET state = 'claimed', attempt_id = $3, claimed_by = $1,
        claim_expires_at = statement_timestamp() + ($4 * interval '1 second'),
        updated_at = statement_timestamp()
    FROM candidate c
    WHERE t.task_id = c.task_id
      AND t.state IN ('ready', 'retryable')
    RETURNING t.*
), cursor_update AS (
    UPDATE vala.forge_worker_claim_state s
    SET last_tenant_id = c.data_tenant_id, updated_at = statement_timestamp()
    FROM candidate c
    WHERE EXISTS (SELECT 1 FROM claimed)
    RETURNING s.singleton
)
SELECT c.execution_tenant_id,
       t.task_id, t.data_tenant_id, t.catalog_name, t.namespace_name,
       t.table_name, t.strategy, t.base_snapshot_id, t.plan,
       t.estimated_files, t.estimated_bytes,
       t.state, t.attempt_id, t.claimed_by,
       t.claim_expires_at, t.watermark_snapshot_id,
       t.watermark_timestamp_ms, t.evidence, t.attempt_count, t.failure_class,
       t.next_eligible_at, t.ready_at, t.created_at,
       t.updated_at
FROM claimed t
JOIN candidate c USING (task_id)
