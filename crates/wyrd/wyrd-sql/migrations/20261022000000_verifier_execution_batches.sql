-- Batched completion and lease renewal of leased Verifier executions.
--
-- A runner completes many executions, and renews many leases, in one round
-- trip each. Both functions touch one row at a time by execution_id, so every
-- row is found through the primary key whatever the table statistics say: a
-- single statement joining the batch arrays left the plan to row estimates,
-- and the row-level-security tenant predicate, whose value the planner cannot
-- see, is estimated at a fraction of the table even when one tenant owns
-- nearly all of it, so such a statement scanned every row of the tenant per
-- batch. Both functions lock their rows in execution_id order, the one order
-- every batched writer of leased executions uses, so a completion batch and a
-- renewal sharing executions never deadlock.
--
-- Both run with the caller's privileges, so RLS bounds every row to the
-- caller's bound tenant, and both stamp times with statement_timestamp(), the
-- caller's statement start.

-- Complete each leased non-group execution of the batch, or re-apply the same
-- completion idempotently, exactly as the single completion statement does:
-- p_executions, p_tokens, and p_results align by position. Returns each
-- applied execution with its binding and frozen Operators; a group execution,
-- a lost lease, or a different result returns nothing.
CREATE FUNCTION wyrd.complete_verifier_executions(
    p_executions uuid[],
    p_tokens uuid[],
    p_results uuid[]
) RETURNS TABLE (
    execution_id uuid,
    binding_id uuid,
    operators jsonb
)
LANGUAGE plpgsql
VOLATILE
SECURITY INVOKER
SET search_path = ''
AS $$
#variable_conflict use_column
DECLARE
    item record;
BEGIN
    FOR item IN
        SELECT u.execution_id, u.lease_token, u.result_id
          FROM unnest(p_executions, p_tokens, p_results)
               AS u(execution_id, lease_token, result_id)
         ORDER BY u.execution_id
    LOOP
        RETURN QUERY
        UPDATE wyrd.verifier_executions e
           SET status = 'completed', result_id = item.result_id, error = NULL,
               group_verdict = NULL,
               lease_expires_at = NULL, next_attempt_at = NULL,
               settled_at = COALESCE(e.settled_at, pg_catalog.statement_timestamp()),
               updated_at = pg_catalog.statement_timestamp()
         WHERE e.execution_id = item.execution_id
           AND e.lease_token = item.lease_token
           AND e.origin <> 'group'
           AND (e.status = 'running'
                OR (e.status = 'completed' AND e.result_id = item.result_id))
        RETURNING e.execution_id, e.binding_id, e.operators;
    END LOOP;
END;
$$;

-- Renew every listed lease still held, on the database clock: p_lease_ms is
-- the lease length and p_renew_under_ms the remaining time under which a lease
-- is renewed, so a lease is extended only once a third of it has passed.
-- Returns every unexpired lease still held, renewed or not; a token absent
-- from the result has expired, been reclaimed, or been settled.
CREATE FUNCTION wyrd.renew_verifier_leases(
    p_tokens uuid[],
    p_lease_ms bigint,
    p_renew_under_ms bigint
) RETURNS SETOF uuid
LANGUAGE plpgsql
VOLATILE
SECURITY INVOKER
SET search_path = ''
AS $$
DECLARE
    held record;
    kept uuid;
BEGIN
    FOR held IN
        SELECT e.execution_id, e.lease_token
          FROM wyrd.verifier_executions e
         WHERE e.status = 'running' AND e.lease_token = ANY(p_tokens)
           AND e.lease_expires_at > pg_catalog.statement_timestamp()
         ORDER BY e.execution_id
    LOOP
        UPDATE wyrd.verifier_executions e
           SET lease_expires_at = CASE
                   WHEN e.lease_expires_at <= pg_catalog.statement_timestamp()
                                              + p_renew_under_ms * INTERVAL '1 millisecond'
                   THEN pg_catalog.statement_timestamp() + p_lease_ms * INTERVAL '1 millisecond'
                   ELSE e.lease_expires_at
               END,
               updated_at = pg_catalog.statement_timestamp()
         WHERE e.execution_id = held.execution_id
           AND e.lease_token = held.lease_token
           AND e.status = 'running'
           AND e.lease_expires_at > pg_catalog.statement_timestamp()
        RETURNING e.lease_token INTO kept;
        IF FOUND THEN
            RETURN NEXT kept;
        END IF;
    END LOOP;
END;
$$;
