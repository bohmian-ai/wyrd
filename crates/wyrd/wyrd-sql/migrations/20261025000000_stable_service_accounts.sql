-- Stable Service accounts.
--
-- A Service account is created explicitly and outlives every Card version.
-- Which exact Service or Agent version a request runs as is execution context
-- selected per token, never account identity, so accounts lose every Card
-- column. An account may instead carry one durable workload restriction: a
-- `(kind, name)` family it is limited to, and for which it is the unique
-- scheduled runtime account. Runtime activity moves from the account row to
-- one row per exact root and account, so two releases sharing one account are
-- activated independently and a reassociated family needs fresh activity from
-- its new account.

-- Registration no longer projects Card-version principals, and the Agent
-- principal kind is removed (Agent Cards remain). Their keys and role grants
-- cascade; refresh tokens have no foreign key and are removed explicitly.

DELETE FROM wyrd.auth_refresh_tokens WHERE principal_kind = 'agent';
DELETE FROM wyrd.auth_service_accounts
 WHERE card_uid IS NOT NULL OR principal_kind = 'agent';
DELETE FROM wyrd.auth_workload_bindings;

ALTER TABLE wyrd.auth_service_accounts
    DROP CONSTRAINT IF EXISTS auth_service_accounts_kind_card_check,
    DROP CONSTRAINT IF EXISTS auth_service_accounts_card_binding_check,
    DROP CONSTRAINT IF EXISTS auth_service_accounts_principal_kind_check;

DROP INDEX IF EXISTS wyrd.auth_service_accounts_card_ref_gin;
DROP INDEX IF EXISTS wyrd.auth_service_accounts_by_card_uid;
DROP INDEX IF EXISTS wyrd.auth_service_accounts_cardless_name;

ALTER TABLE wyrd.auth_service_accounts
    DROP COLUMN card_kind,
    DROP COLUMN card_uid,
    DROP COLUMN card_ref,
    DROP COLUMN version,
    DROP COLUMN last_authenticated_at,
    ADD COLUMN restriction_kind TEXT CHECK (restriction_kind = 'Service'),
    ADD COLUMN restriction_name TEXT,
    ADD CONSTRAINT auth_service_accounts_principal_kind_check
        CHECK (principal_kind IN ('tenant_admin', 'service', 'system')),
    ADD CONSTRAINT auth_service_accounts_restriction_check
        CHECK (
            (restriction_kind IS NULL AND restriction_name IS NULL)
            OR (restriction_kind IS NOT NULL AND restriction_name IS NOT NULL
                AND principal_kind = 'service')
        );

-- Account names are unique per tenant; SYSTEM keeps its fixed name.
CREATE UNIQUE INDEX auth_service_accounts_name
    ON wyrd.auth_service_accounts (data_tenant_id, name)
    WHERE principal_kind <> 'system';

-- One live account per restricted family: the family's scheduled runtime
-- account. A deleted account frees its family.
CREATE UNIQUE INDEX auth_service_accounts_one_family
    ON wyrd.auth_service_accounts (data_tenant_id, restriction_kind, restriction_name)
    WHERE restriction_kind IS NOT NULL AND status <> 'deleted';

ALTER TABLE wyrd.auth_refresh_tokens
    DROP CONSTRAINT IF EXISTS auth_refresh_tokens_principal_kind_check,
    ADD CONSTRAINT auth_refresh_tokens_principal_kind_check
        CHECK (principal_kind IN ('tenant_admin', 'user', 'service'));

-- Federated workload bindings map a trusted subject to a stable account and
-- an optional pinned default root, never to a Card-version principal.
DROP INDEX IF EXISTS wyrd.auth_workload_bindings_card_ref_gin;
ALTER TABLE wyrd.auth_workload_bindings
    DROP COLUMN card_ref,
    ADD COLUMN principal_id UUID NOT NULL,
    ADD COLUMN workload_uid UUID,
    ADD FOREIGN KEY (data_tenant_id, principal_id)
        REFERENCES wyrd.auth_service_accounts (data_tenant_id, id) ON DELETE CASCADE;

-- Last qualifying direct machine use of one account under one exact root.
-- PostgreSQL stamps it with `statement_timestamp()`.
CREATE TABLE wyrd.workload_activity (
    data_tenant_id          UUID        NOT NULL
        REFERENCES platform.tenants(data_tenant_id),
    workload_card_uid       UUID        NOT NULL,
    principal_id            UUID        NOT NULL,
    last_authenticated_at   TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (data_tenant_id, workload_card_uid, principal_id),
    FOREIGN KEY (data_tenant_id, workload_card_uid)
        REFERENCES wyrd.cards (data_tenant_id, card_uid) ON DELETE CASCADE,
    FOREIGN KEY (data_tenant_id, principal_id)
        REFERENCES wyrd.auth_service_accounts (data_tenant_id, id) ON DELETE CASCADE
);

ALTER TABLE wyrd.workload_activity ENABLE ROW LEVEL SECURITY;
ALTER TABLE wyrd.workload_activity FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON wyrd.workload_activity
    USING (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());
CREATE POLICY operator_access ON wyrd.workload_activity TO CURRENT_USER
    USING (wyrd.operator_session()) WITH CHECK (wyrd.operator_session());

