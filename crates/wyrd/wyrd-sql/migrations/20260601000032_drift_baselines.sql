-- Fitted Drift baselines: the fitter's work queue and the Verifier's status.
--
-- Registration of a PSI or SPC Drift Verifier inserts one `pending` row in the
-- Card's registration transaction, pinned to the exact Verifier Card version
-- and the exact baseline Data Card version it fits from. The bounded fitter
-- claims due rows with a PostgreSQL-clock lease (`building`), then settles the
-- same row `ready` with the fitted profile or `failed` with a structured error.
-- A failed or lease-expired fit with attempts left is retried through the same
-- row; a failed row without `next_attempt_at` is final. Custom Drift never has
-- a row.

CREATE TABLE wyrd.drift_baselines (
    data_tenant_id   UUID        NOT NULL
        REFERENCES platform.tenants(data_tenant_id),
    verifier_uid     UUID        NOT NULL,
    data_card_uid    UUID        NOT NULL,
    state            TEXT        NOT NULL DEFAULT 'pending'
        CHECK (state IN ('pending','building','ready','failed')),
    fitted           JSONB,
    error            JSONB,
    attempts         INTEGER     NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    max_attempts     INTEGER     NOT NULL CHECK (max_attempts > 0),
    next_attempt_at  TIMESTAMPTZ,
    lease_token      UUID,
    lease_expires_at TIMESTAMPTZ,
    created_at       TIMESTAMPTZ NOT NULL,
    updated_at       TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (data_tenant_id, verifier_uid),
    FOREIGN KEY (data_tenant_id, verifier_uid)
        REFERENCES wyrd.cards(data_tenant_id, card_uid),
    FOREIGN KEY (data_tenant_id, data_card_uid)
        REFERENCES wyrd.cards(data_tenant_id, card_uid),
    -- Only a ready row holds a profile; only a failed row explains itself.
    CHECK ((state = 'ready') = (fitted IS NOT NULL)),
    CHECK ((state = 'failed') = (error IS NOT NULL)),
    -- A pending row is always due; a building row holds a live lease.
    CHECK (state <> 'pending' OR next_attempt_at IS NOT NULL),
    CHECK (state IN ('pending','failed') OR next_attempt_at IS NULL),
    CHECK ((state = 'building') = (lease_expires_at IS NOT NULL)),
    CHECK (state <> 'building' OR lease_token IS NOT NULL)
);

CREATE INDEX drift_baselines_due
    ON wyrd.drift_baselines (data_tenant_id, next_attempt_at)
    WHERE state IN ('pending','failed') AND next_attempt_at IS NOT NULL;
CREATE INDEX drift_baselines_leased
    ON wyrd.drift_baselines (data_tenant_id, lease_expires_at)
    WHERE state = 'building';

ALTER TABLE wyrd.drift_baselines ENABLE ROW LEVEL SECURITY;
ALTER TABLE wyrd.drift_baselines FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON wyrd.drift_baselines
    USING (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());

REVOKE ALL ON TABLE wyrd.drift_baselines FROM wyrd_app, wyrd_platform_admin;
GRANT SELECT, INSERT, UPDATE ON wyrd.drift_baselines TO wyrd_app;
-- Cross-tenant fitter work discovery only reads.
GRANT SELECT ON wyrd.drift_baselines TO wyrd_platform_admin;

-- A PSI or SPC Drift Verifier is ready only once its exact fitted baseline is.
CREATE OR REPLACE FUNCTION wyrd.verifier_readiness(p_verifier_uid UUID) RETURNS TEXT
LANGUAGE sql STABLE AS $$
    SELECT CASE
        WHEN v.card_uid IS NULL
          OR v.kind <> 'Verifier'
          OR v.status NOT IN ('active', 'deprecated')
            THEN 'verifier_unavailable'
        WHEN v.spec #>> '{implementation,kind}' = 'drift'
         AND v.spec #>> '{implementation,spec,method}' IN ('Psi', 'Spc')
         AND NOT EXISTS (
             SELECT 1 FROM wyrd.drift_baselines b
              WHERE b.verifier_uid = v.card_uid AND b.state = 'ready')
            THEN 'baseline_not_ready'
        ELSE 'ready'
    END
      FROM (SELECT 1) AS probe
      LEFT JOIN wyrd.cards v ON v.card_uid = p_verifier_uid
$$;
