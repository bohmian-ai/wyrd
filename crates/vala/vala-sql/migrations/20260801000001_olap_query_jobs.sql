-- Stage 3 async query-job storage.
-- vala.olap_query_jobs: durable async-query submission ledger (one row per job).
-- Tenant-scoped with RLS + per-tenant idempotency-key uniqueness. Mirrors the
-- landed vala.bifrost_tables / vala.olap_commits RLS idiom (ENABLE + FORCE RLS,
-- tenant_isolation policy on wyrd.current_tenant(), wyrd_app grant).
--
-- query_class / admission_mode are Stage-5 classifier outputs: nullable, never
-- written in Stage 3. A queued row carries executor_availability = 'pending_stage5'
-- because no executor exists yet (review M-06). sql_normalized / params_redacted
-- persist the redacted query only; raw literals never reach this table.

CREATE TABLE vala.olap_query_jobs (
    data_tenant_id          UUID    NOT NULL REFERENCES platform.tenants(data_tenant_id),
    job_uid                 UUID    NOT NULL,
    idempotency_key         TEXT    NOT NULL,
    state                   TEXT    NOT NULL
        CHECK (state IN ('queued', 'claimed', 'running', 'succeeded', 'failed', 'canceled'))
        DEFAULT 'queued',
    executor_availability   TEXT    NOT NULL
        CHECK (executor_availability IN ('available', 'pending_stage5'))
        DEFAULT 'pending_stage5',
    sql_normalized          TEXT    NOT NULL,
    params_redacted         JSONB   NOT NULL DEFAULT '[]'::jsonb,
    query_class             TEXT,
    admission_mode          TEXT,
    error_code              TEXT,
    error_detail            TEXT,
    submitted_at            TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at              TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (data_tenant_id, job_uid),
    UNIQUE (data_tenant_id, idempotency_key)
);

ALTER TABLE vala.olap_query_jobs ENABLE ROW LEVEL SECURITY;
ALTER TABLE vala.olap_query_jobs FORCE  ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON vala.olap_query_jobs
    USING      (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());

GRANT SELECT, INSERT, UPDATE, DELETE
    ON vala.olap_query_jobs
    TO wyrd_app;
