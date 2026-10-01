-- Durable observation ordinal for continuous Eval `every_nth` sampling.
--
-- Each observation run stores its one-based position among its binding's
-- observation runs, assigned once in the enqueue transaction while that
-- binding's row is locked, so concurrent enqueues serialize and every attempt
-- of one run reads the same position. A duplicate enqueue inserts nothing and
-- consumes no ordinal. Existing observation runs take their creation order.
ALTER TABLE wyrd.verifier_runs
    ADD COLUMN observation_ordinal BIGINT;

UPDATE wyrd.verifier_runs AS r
   SET observation_ordinal = ranked.ordinal
  FROM (SELECT run_id,
               row_number() OVER (PARTITION BY data_tenant_id, binding_id
                                  ORDER BY created_at, run_id) AS ordinal
          FROM wyrd.verifier_runs
         WHERE origin = 'observation') AS ranked
 WHERE r.run_id = ranked.run_id;

ALTER TABLE wyrd.verifier_runs
    -- Observation runs, and only they, carry a positive ordinal.
    ADD CONSTRAINT verifier_runs_observation_ordinal_shape CHECK (
        (origin = 'observation') = (observation_ordinal IS NOT NULL)
        AND (observation_ordinal IS NULL OR observation_ordinal > 0)
    );

CREATE UNIQUE INDEX verifier_runs_observation_ordinal
    ON wyrd.verifier_runs (data_tenant_id, binding_id, observation_ordinal)
    WHERE origin = 'observation';
