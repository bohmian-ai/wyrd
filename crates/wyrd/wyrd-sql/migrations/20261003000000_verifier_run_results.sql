-- Staged Verifier results.
--
-- A run's result is decided once. When execution finishes, the runner stores
-- the complete result here in one lease-fenced transaction before any of it
-- reaches Bifrost: the result ID, the result event time, the verdict, summary,
-- and counts its completion freezes, the exact Verifier the rows are
-- attributed to, and one batch ID plus Arrow IPC stream per result table in
-- write order. Every later write of the run, by any claimant, replays these
-- bytes under these batch IDs, so Scribe's batch fence absorbs every repeat.
-- The settlement that completes or terminally ends the run deletes the row.
CREATE TABLE wyrd.verifier_run_results (
    run_id          UUID        NOT NULL PRIMARY KEY
        REFERENCES wyrd.verifier_runs(run_id) ON DELETE CASCADE,
    data_tenant_id  UUID        NOT NULL
        REFERENCES platform.tenants(data_tenant_id),
    result_id       UUID        NOT NULL,
    event_time      TIMESTAMPTZ NOT NULL,
    verdict         JSONB       NOT NULL,
    summary         TEXT        NOT NULL,
    counts          JSONB       NOT NULL,
    verifier        JSONB       NOT NULL,
    tables          TEXT[]      NOT NULL,
    batch_ids       UUID[]      NOT NULL,
    payloads        BYTEA[]     NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT statement_timestamp(),
    CHECK (cardinality(tables) > 0
           AND cardinality(tables) = cardinality(batch_ids)
           AND cardinality(tables) = cardinality(payloads))
);

ALTER TABLE wyrd.verifier_run_results ENABLE ROW LEVEL SECURITY;
ALTER TABLE wyrd.verifier_run_results FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON wyrd.verifier_run_results
    USING (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());

REVOKE ALL ON TABLE wyrd.verifier_run_results FROM wyrd_app, wyrd_platform_admin;
GRANT SELECT, INSERT, DELETE ON wyrd.verifier_run_results TO wyrd_app;
