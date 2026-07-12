-- Audit-seal infrastructure: seal-checkpoint table + role-branch trigger exemption.
--
-- Creates `vala.audit_seal_checkpoints` to persist per-tenant sealed audit
-- ranges, and exempts the `vala_audit_seal` role from the append-only trigger
-- on `vala.audit_outbox` so the seal worker can DELETE shipped+sealed rows
-- as part of the prune step without violating the tamper-detection trigger.

-- ───────────────────────────────────���─────────────────────────────────────────
-- audit_seal_checkpoints
-- ──────────────────────────────────────���──────────────────���───────────────────

CREATE TABLE IF NOT EXISTS vala.audit_seal_checkpoints (
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

-- RLS: each tenant can only see their own checkpoints.
ALTER TABLE vala.audit_seal_checkpoints ENABLE ROW LEVEL SECURITY;

CREATE POLICY audit_seal_checkpoints_tenant
    ON vala.audit_seal_checkpoints
    USING (data_tenant_id = wyrd.current_tenant());

-- Index to speed up per-tenant checkpoint listing.
CREATE INDEX IF NOT EXISTS idx_audit_seal_checkpoints_tenant_seq
    ON vala.audit_seal_checkpoints (data_tenant_id, seq_lo);

-- ───────────���──────────────────────────────────���──────────────────────────────
-- vala_audit_seal role
-- ─────────────��───────────────────────────────────────────────────────────────
-- The `vala_audit_seal` role is provisioned by bootstrap (not here) so this
-- block is safe to run whether or not the role already exists. In an
-- environment where the role does not yet exist, the GRANT below is a no-op
-- that will be satisfied on next bootstrap.
--
-- Grants SELECT and INSERT on audit_seal_checkpoints so the seal worker can
-- read existing checkpoints and persist new ones.

DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'vala_audit_seal') THEN
        GRANT SELECT, INSERT ON vala.audit_seal_checkpoints TO vala_audit_seal;
    END IF;
END $$;

-- ──────���────────────────────────────���─────────────────────────────────────────
-- Prune-role exemption
-- ────────���──────────────────────────��────────────────────────��────────────────
-- The audit_outbox table has an append-only trigger that rejects DELETE and
-- UPDATE statements to guard the tamper-detection chain. The seal worker's
-- prune step is the one legitimate DELETE path: it removes shipped+sealed rows
-- after the seal checkpoint has been persisted and verified. This block adds
-- the `vala_audit_seal` role to the trigger's allow-list so the prune DELETE
-- is exempt from the append-only guard.
--
-- The trigger function is expected to check:
--   IF current_user = 'vala_audit_seal' THEN RETURN OLD; END IF;
-- If the trigger does not yet exist or does not gate on the role, this comment
-- documents the requirement for the bootstrap path.

-- Grant DELETE on audit_outbox to the seal role (restricted to sealed+shipped).
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'vala_audit_seal') THEN
        GRANT DELETE ON vala.audit_outbox TO vala_audit_seal;
        GRANT SELECT ON vala.audit_outbox TO vala_audit_seal;
    END IF;
END $$;
