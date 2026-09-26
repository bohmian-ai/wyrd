-- Verifier run and Operator dispatch control state.
--
-- `wyrd.verifier_runs` is the only Verifier execution queue. Scheduled,
-- manual, and observation-created work shares one row shape that freezes the
-- exact identities the run judges; the generic runner claims rows with an
-- expiring lease and settles them only with that lease's token.
-- `wyrd.operator_dispatches` holds one independent reaction per distinct
-- Operator of a failed binding-created run, inserted in the settlement
-- transaction. The verdict and Drift/Eval details live in Bifrost; a run keeps
-- only its execution status and result pointer.

-- ---------------------------------------------------------------------------
-- Generic Verifier readiness
-- ---------------------------------------------------------------------------
-- One owner for "may this Verifier run now", shared by the scheduler, manual
-- enqueue, and binding status. A missing, non-Verifier, or inactive Card is
-- unavailable. A PSI or SPC Drift Verifier needs a fitted baseline; until the
-- baseline store exists it is never ready, so no unfitted baseline can yield a
-- judgment. Custom Drift and Eval are ready once registered. SECURITY INVOKER
-- (the default) keeps the Card read under the caller's forced RLS.
CREATE FUNCTION wyrd.verifier_readiness(p_verifier_uid UUID) RETURNS TEXT
LANGUAGE sql STABLE AS $$
    SELECT CASE
        WHEN v.card_uid IS NULL
          OR v.kind <> 'Verifier'
          OR v.status NOT IN ('active', 'deprecated')
            THEN 'verifier_unavailable'
        WHEN v.spec #>> '{implementation,kind}' = 'drift'
         AND v.spec #>> '{implementation,spec,method}' IN ('Psi', 'Spc')
            THEN 'baseline_not_ready'
        ELSE 'ready'
    END
      FROM (SELECT 1) AS probe
      LEFT JOIN wyrd.cards v ON v.card_uid = p_verifier_uid
$$;

-- ---------------------------------------------------------------------------
-- Verifier runs
-- ---------------------------------------------------------------------------
-- Identity: a binding-created run (schedule or observation origin, or a
-- manual binding run) freezes its owner, binding, effective Trigger, and
-- Operators; a direct manual run stores all of them as NULL/empty. Every
-- manual run, and only a manual run, freezes its authenticated requester.
-- Input: a Drift window [window_start, window_end) or an Eval record plus its
-- committed server event time — never both.
-- Execution: pending/retrying rows are due at next_attempt_at; a running row
-- holds lease_token until lease_expires_at. Terminal rows keep their last
-- lease token so a settlement retry by the same holder is idempotent.
CREATE TABLE wyrd.verifier_runs (
    run_id                    UUID        NOT NULL PRIMARY KEY,
    data_tenant_id            UUID        NOT NULL
        REFERENCES platform.tenants(data_tenant_id),
    verifier_uid              UUID        NOT NULL,
    verifier_version          TEXT        NOT NULL,
    subject_card_uid          UUID        NOT NULL,
    origin                    TEXT        NOT NULL
        CHECK (origin IN ('schedule','manual','observation')),
    owner_card_uid            UUID,
    binding_id                UUID,
    trigger_uid               UUID,
    trigger_digest            TEXT,
    operators                 JSONB       NOT NULL DEFAULT '[]'::jsonb,
    window_start              TIMESTAMPTZ,
    window_end                TIMESTAMPTZ,
    input_record_id           TEXT,
    input_event_time          TIMESTAMPTZ,
    requested_by_principal_id UUID,
    status                    TEXT        NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending','running','retrying','completed',
                          'cancelled','timed_out','errored')),
    result_id                 UUID,
    error                     JSONB,
    attempts                  INTEGER     NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    max_attempts              INTEGER     NOT NULL CHECK (max_attempts > 0),
    next_attempt_at           TIMESTAMPTZ,
    lease_token               UUID,
    lease_expires_at          TIMESTAMPTZ,
    created_at                TIMESTAMPTZ NOT NULL,
    updated_at                TIMESTAMPTZ NOT NULL,
    settled_at                TIMESTAMPTZ,
    UNIQUE (data_tenant_id, run_id),
    FOREIGN KEY (data_tenant_id, verifier_uid)
        REFERENCES wyrd.cards(data_tenant_id, card_uid),
    FOREIGN KEY (data_tenant_id, subject_card_uid)
        REFERENCES wyrd.cards(data_tenant_id, card_uid),
    FOREIGN KEY (data_tenant_id, binding_id)
        REFERENCES wyrd.verification_bindings(data_tenant_id, binding_id),
    CHECK (jsonb_typeof(operators) = 'array'),
    -- Owner and binding are both present (binding-created) or both absent (direct).
    CHECK ((owner_card_uid IS NULL) = (binding_id IS NULL)),
    -- A binding run freezes exactly one Trigger identity; a direct run has
    -- no Trigger and no Operators.
    CHECK (
        (binding_id IS NOT NULL AND (trigger_uid IS NULL) <> (trigger_digest IS NULL))
        OR (binding_id IS NULL AND trigger_uid IS NULL AND trigger_digest IS NULL
            AND operators = '[]'::jsonb)
    ),
    -- Only manual runs have a requester; only manual runs may be direct.
    CHECK ((origin = 'manual') = (requested_by_principal_id IS NOT NULL)),
    CHECK (origin = 'manual' OR binding_id IS NOT NULL),
    -- Exactly one input shape; observation runs and only they use a record.
    CHECK (
        (window_start IS NOT NULL AND window_end IS NOT NULL AND window_start < window_end
         AND input_record_id IS NULL AND input_event_time IS NULL)
        OR (window_start IS NULL AND window_end IS NULL
            AND input_record_id IS NOT NULL AND input_event_time IS NOT NULL)
    ),
    CHECK ((origin = 'observation') = (input_record_id IS NOT NULL)),
    -- Only a completed run points at a result, and it carries no error.
    CHECK ((status = 'completed') = (result_id IS NOT NULL)),
    CHECK (status <> 'completed' OR error IS NULL),
    -- Terminal failures always explain themselves.
    CHECK (status NOT IN ('cancelled','timed_out','errored') OR error IS NOT NULL),
    -- Due rows carry their due time; a running row holds a live lease.
    CHECK ((status IN ('pending','retrying')) = (next_attempt_at IS NOT NULL)),
    CHECK ((status = 'running') = (lease_expires_at IS NOT NULL)),
    CHECK (status <> 'running' OR lease_token IS NOT NULL)
);

-- One scheduled run per (tenant, binding, due occurrence); one observation
-- run per (tenant, binding, input record).
CREATE UNIQUE INDEX verifier_runs_scheduled_occurrence
    ON wyrd.verifier_runs (data_tenant_id, binding_id, window_end)
    WHERE origin = 'schedule';
CREATE UNIQUE INDEX verifier_runs_observation_record
    ON wyrd.verifier_runs (data_tenant_id, binding_id, input_record_id)
    WHERE origin = 'observation';

-- Runner claims scan due and expired work; status reads find a binding's
-- latest run.
CREATE INDEX verifier_runs_due
    ON wyrd.verifier_runs (data_tenant_id, next_attempt_at)
    WHERE status IN ('pending','retrying');
CREATE INDEX verifier_runs_leased
    ON wyrd.verifier_runs (data_tenant_id, lease_expires_at)
    WHERE status = 'running';
CREATE INDEX verifier_runs_by_binding
    ON wyrd.verifier_runs (data_tenant_id, binding_id, run_id)
    WHERE binding_id IS NOT NULL;

ALTER TABLE wyrd.verifier_runs ENABLE ROW LEVEL SECURITY;
ALTER TABLE wyrd.verifier_runs FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON wyrd.verifier_runs
    USING (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());

REVOKE ALL ON TABLE wyrd.verifier_runs FROM wyrd_app, wyrd_platform_admin;
GRANT SELECT, INSERT, UPDATE ON wyrd.verifier_runs TO wyrd_app;
-- Cross-tenant work discovery and queue-depth telemetry only read.
GRANT SELECT ON wyrd.verifier_runs TO wyrd_platform_admin;

-- ---------------------------------------------------------------------------
-- Operator dispatches
-- ---------------------------------------------------------------------------
-- One row per distinct frozen Operator (Card UID or inline-spec digest) of a
-- failed binding-created run. The unique (tenant, run, Operator) identity makes
-- a settlement retry insert nothing new. Each row has its own delivery status,
-- lease, attempt count, and last error, driven by the Operator worker.
CREATE TABLE wyrd.operator_dispatches (
    dispatch_id      UUID        NOT NULL PRIMARY KEY,
    data_tenant_id   UUID        NOT NULL
        REFERENCES platform.tenants(data_tenant_id),
    run_id           UUID        NOT NULL,
    operator_uid     UUID,
    operator_digest  TEXT,
    status           TEXT        NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending','running','retrying','delivered','failed')),
    attempts         INTEGER     NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    next_attempt_at  TIMESTAMPTZ,
    lease_token      UUID,
    lease_expires_at TIMESTAMPTZ,
    last_error       JSONB,
    created_at       TIMESTAMPTZ NOT NULL,
    updated_at       TIMESTAMPTZ NOT NULL,
    UNIQUE (data_tenant_id, dispatch_id),
    FOREIGN KEY (data_tenant_id, run_id)
        REFERENCES wyrd.verifier_runs(data_tenant_id, run_id),
    CHECK ((operator_uid IS NULL) <> (operator_digest IS NULL)),
    CHECK ((status IN ('pending','retrying')) = (next_attempt_at IS NOT NULL)),
    CHECK ((status = 'running') = (lease_expires_at IS NOT NULL)),
    CHECK (status <> 'running' OR lease_token IS NOT NULL),
    CHECK (status <> 'failed' OR last_error IS NOT NULL)
);

CREATE UNIQUE INDEX operator_dispatches_run_operator
    ON wyrd.operator_dispatches
       (data_tenant_id, run_id, COALESCE(operator_uid::text, operator_digest));
CREATE INDEX operator_dispatches_due
    ON wyrd.operator_dispatches (data_tenant_id, next_attempt_at)
    WHERE status IN ('pending','retrying');

ALTER TABLE wyrd.operator_dispatches ENABLE ROW LEVEL SECURITY;
ALTER TABLE wyrd.operator_dispatches FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON wyrd.operator_dispatches
    USING (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());

REVOKE ALL ON TABLE wyrd.operator_dispatches FROM wyrd_app, wyrd_platform_admin;
GRANT SELECT, INSERT, UPDATE ON wyrd.operator_dispatches TO wyrd_app;
GRANT SELECT ON wyrd.operator_dispatches TO wyrd_platform_admin;

-- The scheduler discovers tenants with due bindings across every tenant.
GRANT SELECT ON wyrd.verification_bindings TO wyrd_platform_admin;
