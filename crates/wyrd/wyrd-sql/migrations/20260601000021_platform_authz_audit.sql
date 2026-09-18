-- Platform-control-plane authorization audit.
--
-- Every decision that evaluates a platform principal's permission appends one
-- row here, allowed and denied alike, in the same transaction as the decision.
-- A decision that cannot be recorded fails closed.
--
-- The tenant plane's equivalent, `wyrd.audit_authz_check`, is row-level-secured
-- by `data_tenant_id`. A platform decision has no owning tenant, so this table
-- lives at platform scope and is reached only through the operator role.
-- `target_tenant_id` is not a tenancy scope: it names the tenant the operation
-- was *about* — the tenant created, suspended, or recovered — and is null for a
-- decision that names no tenant, such as listing the directory.
CREATE TABLE platform.audit_authz (
    id                BIGSERIAL    PRIMARY KEY,
    request_id        TEXT         NOT NULL,
    principal_id      UUID         NOT NULL,
    credential_id     UUID,
    resource          TEXT         NOT NULL,
    action            TEXT         NOT NULL,
    target_tenant_id  UUID,
    decision          TEXT         NOT NULL CHECK (decision IN ('allow','deny')),
    deny_reason       TEXT,
    occurred_at       TIMESTAMPTZ  NOT NULL DEFAULT now(),
    CONSTRAINT platform_audit_authz_decision_consistency
        CHECK (
            (decision = 'allow' AND deny_reason IS NULL)
         OR (decision = 'deny'  AND deny_reason IS NOT NULL)
        )
);

CREATE INDEX platform_audit_authz_by_request
    ON platform.audit_authz (request_id);
CREATE INDEX platform_audit_authz_by_principal
    ON platform.audit_authz (principal_id, occurred_at DESC);
CREATE INDEX platform_audit_authz_by_target_tenant
    ON platform.audit_authz (target_tenant_id, occurred_at DESC)
    WHERE target_tenant_id IS NOT NULL;

GRANT SELECT, INSERT ON platform.audit_authz TO wyrd_platform_admin;
GRANT USAGE, SELECT ON SEQUENCE platform.audit_authz_id_seq TO wyrd_platform_admin;
