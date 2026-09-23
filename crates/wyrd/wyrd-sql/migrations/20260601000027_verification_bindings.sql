-- Verification binding projection and machine-principal runtime activity.
--
-- A Service, Service component occurrence, or standalone Agent attaches a
-- Verifier through an inline `verified_by` binding. Composite registration
-- projects each effective binding here in the same tenant transaction as the
-- owner Card and its Card-bound principal. Runtime activity is not a new
-- table: it is the last successful qualifying machine exchange recorded on the
-- existing principal row.

-- ---------------------------------------------------------------------------
-- Runtime activity on the existing machine-principal row
-- ---------------------------------------------------------------------------
-- Set by the tenant token issuer, in the issuing transaction, when an API-key
-- exchange or workload `jwt-bearer` grant mints a token for a Card-bound
-- Service or Agent. No other grant, request, or observation writes it.
ALTER TABLE wyrd.auth_service_accounts
    ADD COLUMN last_authenticated_at TIMESTAMPTZ;

-- Two exact Card versions of one Service or Agent share a name but are
-- distinct principals for A/B operation, keyed by the retained
-- UNIQUE (data_tenant_id, principal_kind, card_kind, card_uid). Name
-- uniqueness remains only for principals that bind no Card, whose name is
-- their only human-facing identity.
ALTER TABLE wyrd.auth_service_accounts
    DROP CONSTRAINT auth_service_accounts_data_tenant_id_name_key;

CREATE UNIQUE INDEX auth_service_accounts_cardless_name
    ON wyrd.auth_service_accounts (data_tenant_id, name)
    WHERE card_uid IS NULL;

-- Activity reads join an owner Card version to its principal.
CREATE INDEX auth_service_accounts_by_card_uid
    ON wyrd.auth_service_accounts (data_tenant_id, card_uid)
    WHERE card_uid IS NOT NULL;

-- ---------------------------------------------------------------------------
-- Verification bindings
-- ---------------------------------------------------------------------------
-- One row per effective binding of one exact owner Card version. The natural
-- key is (tenant, owner Card UID, subject occurrence, Verifier UID); the
-- subject occurrence is the reserved owner value `$owner` for a Service-level
-- or standalone-Agent binding and the component alias for a Service component
-- binding; composition validation refuses `$owner` as an alias, so the two
-- domains never collide. `binding_id` is minted on the first projection of a
-- natural key and kept thereafter.
--
-- The effective Trigger and every `on_failure` Operator are frozen as either a
-- referenced Card UID or, for an inline body, its canonical spec digest. The
-- schedule of a `schedule` Trigger is frozen alongside, so arming the cursor
-- never re-reads a Trigger Card. `next_run_at` stays NULL until the owner's
-- first qualifying machine exchange arms it; an `observations_ready` binding
-- never has a cursor.
CREATE TABLE wyrd.verification_bindings (
    binding_id              UUID        NOT NULL PRIMARY KEY,
    data_tenant_id          UUID        NOT NULL
        REFERENCES platform.tenants(data_tenant_id),
    owner_card_uid          UUID        NOT NULL,
    owner_card_kind         TEXT        NOT NULL
        CHECK (owner_card_kind IN ('Service','Agent')),
    subject_occurrence_key  TEXT        NOT NULL,
    subject_card_uid        UUID        NOT NULL,
    verifier_uid            UUID        NOT NULL,
    trigger_uid             UUID,
    trigger_digest          TEXT,
    operators               JSONB       NOT NULL DEFAULT '[]'::jsonb,
    activation              TEXT        NOT NULL
        CHECK (activation IN ('schedule','observations_ready')),
    schedule_cron           TEXT,
    schedule_tz             TEXT,
    next_run_at             TIMESTAMPTZ,
    created_at              TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (data_tenant_id, binding_id),
    CONSTRAINT verification_bindings_natural_key UNIQUE
        (data_tenant_id, owner_card_uid, subject_occurrence_key, verifier_uid),
    FOREIGN KEY (data_tenant_id, owner_card_uid)
        REFERENCES wyrd.cards(data_tenant_id, card_uid),
    CHECK ((trigger_uid IS NULL) <> (trigger_digest IS NULL)),
    CHECK (jsonb_typeof(operators) = 'array'),
    CHECK (
        (activation = 'schedule' AND schedule_cron IS NOT NULL)
        OR (activation = 'observations_ready' AND schedule_cron IS NULL
            AND schedule_tz IS NULL AND next_run_at IS NULL)
    ),
    CHECK (owner_card_kind = 'Service' OR subject_occurrence_key = '$owner')
);

-- Status reads list one owner's bindings; schedulers scan armed cursors.
CREATE INDEX verification_bindings_by_owner
    ON wyrd.verification_bindings (data_tenant_id, owner_card_uid);
CREATE INDEX verification_bindings_due
    ON wyrd.verification_bindings (data_tenant_id, next_run_at)
    WHERE next_run_at IS NOT NULL;

ALTER TABLE wyrd.verification_bindings ENABLE ROW LEVEL SECURITY;
ALTER TABLE wyrd.verification_bindings FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON wyrd.verification_bindings
    USING (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());

REVOKE ALL ON TABLE wyrd.verification_bindings FROM wyrd_app;
GRANT SELECT, INSERT, UPDATE ON wyrd.verification_bindings TO wyrd_app;
