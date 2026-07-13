-- Audit-seal checkpoint table: persists the Ed25519-signed sealed ranges.
--
-- Each row records a signed `[seq_lo, seq_hi]` range for one tenant. The seal
-- worker writes checkpoints under the tenant's own bind (RLS-scoped INSERT);
-- the verifier reads them back and recomputes each range hash from the Iceberg
-- `audit_log` content columns to confirm the signature still holds.
--
-- Pruning the sealed+shipped outbox rows is a separate, role-scoped step (a
-- `vala_audit_seal` DELETE exempted from the append-only trigger) and is NOT
-- established here — this migration only creates the checkpoint store.

CREATE TABLE vala.audit_seal_checkpoints (
    data_tenant_id  UUID        NOT NULL,
    seq_lo          BIGINT      NOT NULL,
    seq_hi          BIGINT      NOT NULL,
    range_hash      BYTEA       NOT NULL,       -- SHA256 over [seq_lo, seq_hi] entry hashes
    signature       BYTEA       NOT NULL,       -- Ed25519 over AUDIT_SEAL_DOMAIN || range_hash
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT audit_seal_checkpoints_pkey PRIMARY KEY (data_tenant_id, seq_lo, seq_hi),
    CONSTRAINT audit_seal_checkpoints_seq_order CHECK (seq_lo <= seq_hi),
    CONSTRAINT audit_seal_checkpoints_range_hash_len CHECK (length(range_hash) = 32),
    CONSTRAINT audit_seal_checkpoints_signature_len CHECK (length(signature) = 64)
);

-- RLS: each tenant can only see and write their own checkpoints.
ALTER TABLE vala.audit_seal_checkpoints ENABLE ROW LEVEL SECURITY;
ALTER TABLE vala.audit_seal_checkpoints FORCE  ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation
    ON vala.audit_seal_checkpoints
    USING      (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());

-- The seal worker inserts checkpoints and the verifier reads them, both under
-- the tenant's own RLS bind. Checkpoints are never updated or deleted by the
-- app role (the upsert is INSERT ... ON CONFLICT DO NOTHING).
GRANT SELECT, INSERT ON vala.audit_seal_checkpoints TO wyrd_app;

-- Index to speed up per-tenant checkpoint listing.
CREATE INDEX IF NOT EXISTS idx_audit_seal_checkpoints_tenant_seq
    ON vala.audit_seal_checkpoints (data_tenant_id, seq_lo);
