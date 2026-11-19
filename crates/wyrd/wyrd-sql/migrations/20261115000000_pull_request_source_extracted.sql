-- Whether a completed pull_request execution extracted its admitted source.
--
-- A completed execution is not proof that its source was extracted: a failed
-- pull_request extraction still completes with an inconclusive verdict. The
-- incremental review baseline is the latest completed execution of the same
-- PR and policy whose source was extracted, so the fact is written with the
-- completion itself, in the lease-fenced completion statement, and never
-- while work is unfinished or after it was cancelled. Existing rows stay
-- false, so their pull requests are reviewed in full until eligible history
-- accumulates.
ALTER TABLE wyrd.verifier_executions
    ADD COLUMN pr_source_extracted BOOLEAN NOT NULL DEFAULT false,
    ADD CONSTRAINT verifier_executions_pr_source_extracted CHECK (
        NOT pr_source_extracted OR (origin = 'pull_request' AND status = 'completed')
    );

-- The batched completion of 20261022000000, now also recording each
-- completion's `pr_source_extracted` (`p_extracted`, aligned by position)
-- exactly as the single completion statement does.
DROP FUNCTION wyrd.complete_verifier_executions(uuid[], uuid[], uuid[]);

CREATE FUNCTION wyrd.complete_verifier_executions(
    p_executions uuid[],
    p_tokens uuid[],
    p_results uuid[],
    p_extracted boolean[]
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
        SELECT u.execution_id, u.lease_token, u.result_id, u.extracted
          FROM unnest(p_executions, p_tokens, p_results, p_extracted)
               AS u(execution_id, lease_token, result_id, extracted)
         ORDER BY u.execution_id
    LOOP
        RETURN QUERY
        UPDATE wyrd.verifier_executions e
           SET status = 'completed', result_id = item.result_id, error = NULL,
               group_verdict = NULL, pr_source_extracted = item.extracted,
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
