-- Independent durable fairness cursor for Forge worker admission.

CREATE TABLE vala.forge_worker_claim_state (
    singleton boolean PRIMARY KEY DEFAULT true CHECK (singleton),
    last_tenant_id uuid,
    updated_at timestamptz NOT NULL DEFAULT now()
);

ALTER TABLE vala.forge_worker_claim_state ENABLE ROW LEVEL SECURITY;
ALTER TABLE vala.forge_worker_claim_state FORCE ROW LEVEL SECURITY;
CREATE POLICY operator_access ON vala.forge_worker_claim_state TO CURRENT_USER
    USING (wyrd.operator_session()) WITH CHECK (wyrd.operator_session());

INSERT INTO vala.forge_worker_claim_state (singleton) VALUES (true);
