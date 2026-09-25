-- Gateway admission state shared by every wyrd-server replica: fixed-minute
-- request/token windows, concurrency leases, and the idempotency fences of the
-- append-only accounting ledger.
--
-- Admission serializes per tenant on a transaction advisory lock, so these
-- tables need no row-level locking protocol of their own.

-- ---------------------------------------------------------------------------
-- Fixed one-minute request and token counters per governance limit.
-- ---------------------------------------------------------------------------
CREATE TABLE wyrd.gateway_limit_windows (
    data_tenant_id      UUID        NOT NULL REFERENCES platform.tenants(data_tenant_id),
    -- Canonical JSON of the limit's (subject, target) key.
    limit_key           TEXT        NOT NULL,
    window_start        TIMESTAMPTZ NOT NULL,
    requests            BIGINT      NOT NULL DEFAULT 0 CHECK (requests >= 0),
    tokens              BIGINT      NOT NULL DEFAULT 0 CHECK (tokens >= 0),
    PRIMARY KEY (data_tenant_id, limit_key, window_start)
);

ALTER TABLE wyrd.gateway_limit_windows ENABLE ROW LEVEL SECURITY;
ALTER TABLE wyrd.gateway_limit_windows FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON wyrd.gateway_limit_windows
    USING (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());

-- ---------------------------------------------------------------------------
-- Concurrency leases: one row per admitted call per concurrency limit. A
-- lease is deleted when the call is accounted and otherwise lapses at
-- `expires_at`, so a crashed replica cannot hold capacity forever.
-- ---------------------------------------------------------------------------
CREATE TABLE wyrd.gateway_call_leases (
    data_tenant_id      UUID        NOT NULL REFERENCES platform.tenants(data_tenant_id),
    limit_key           TEXT        NOT NULL,
    call_id             UUID        NOT NULL,
    expires_at          TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (data_tenant_id, limit_key, call_id)
);

CREATE INDEX gateway_call_leases_call_idx
    ON wyrd.gateway_call_leases (data_tenant_id, call_id);

ALTER TABLE wyrd.gateway_call_leases ENABLE ROW LEVEL SECURITY;
ALTER TABLE wyrd.gateway_call_leases FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON wyrd.gateway_call_leases
    USING (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());

-- ---------------------------------------------------------------------------
-- Ledger idempotency fences: one creation and one settlement per reservation,
-- one attempt entry per (call, ordinal), and one call entry per call.
-- ---------------------------------------------------------------------------
CREATE UNIQUE INDEX gateway_accounting_reservation_created_uidx
    ON wyrd.gateway_accounting_entries (
        data_tenant_id, ((entry -> 'budget_reservation_created' ->> 'reservation_id'))
    )
    WHERE kind = 'budget_reservation_created';

CREATE UNIQUE INDEX gateway_accounting_reservation_settled_uidx
    ON wyrd.gateway_accounting_entries (
        data_tenant_id, ((entry -> 'budget_reservation_settled' ->> 'reservation_id'))
    )
    WHERE kind = 'budget_reservation_settled';

CREATE UNIQUE INDEX gateway_accounting_attempt_uidx
    ON wyrd.gateway_accounting_entries (
        data_tenant_id, call_id, ((entry -> 'attempt_accounted' ->> 'attempt_ordinal'))
    )
    WHERE kind = 'attempt_accounted';

CREATE UNIQUE INDEX gateway_accounting_call_uidx
    ON wyrd.gateway_accounting_entries (data_tenant_id, call_id)
    WHERE kind = 'call_accounted';

-- Budget spend reads reservations by their admission-fixed period.
CREATE INDEX gateway_accounting_reservation_period_idx
    ON wyrd.gateway_accounting_entries (
        data_tenant_id, ((entry -> 'budget_reservation_created' ->> 'period_start'))
    )
    WHERE kind = 'budget_reservation_created';
