-- Independent durable fairness cursor for Forge worker admission.

CREATE TABLE vala.forge_worker_claim_state (
    singleton boolean PRIMARY KEY DEFAULT true CHECK (singleton),
    last_tenant_id uuid,
    updated_at timestamptz NOT NULL DEFAULT now()
);

INSERT INTO vala.forge_worker_claim_state (singleton) VALUES (true);

GRANT SELECT, UPDATE ON vala.forge_worker_claim_state TO wyrd_platform_admin;
