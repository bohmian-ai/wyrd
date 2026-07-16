-- Add the registration lifecycle states and pending-card bookkeeping.

ALTER TABLE wyrd.cards
    DROP CONSTRAINT cards_status_check,
    ADD CONSTRAINT cards_status_check CHECK (
        status IN ('pending', 'active', 'deprecated', 'failed', 'expired', 'deleted')
    );

DROP INDEX IF EXISTS wyrd.cards_identity_unique;
CREATE UNIQUE INDEX cards_identity_unique
    ON wyrd.cards (data_tenant_id, kind, space, name, version)
    WHERE status NOT IN ('deleted', 'failed', 'expired');

DROP INDEX IF EXISTS wyrd.idx_cards_version_latest;
CREATE INDEX idx_cards_version_latest
    ON wyrd.cards (
        data_tenant_id, kind, space, name,
        version_major DESC, version_minor DESC, version_patch DESC
    )
    WHERE status = 'active' AND NOT version_is_prerelease;

DROP INDEX IF EXISTS wyrd.idx_cards_spec_hash;
CREATE INDEX idx_cards_spec_hash
    ON wyrd.cards (data_tenant_id, kind, space, name, spec_hash)
    WHERE status = 'active';

ALTER TABLE wyrd.cards
    ADD COLUMN IF NOT EXISTS registration_operation_id UUID
        REFERENCES wyrd.card_registration_operations(operation_id),
    ADD COLUMN IF NOT EXISTS pending_since TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS finalized_at TIMESTAMPTZ;

CREATE INDEX cards_pending_sweep_idx
    ON wyrd.cards (data_tenant_id, status, pending_since)
    WHERE status = 'pending';
