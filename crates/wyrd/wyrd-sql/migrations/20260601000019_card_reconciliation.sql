-- Durable, bounded reconciliation state for Card lifecycle side effects.
--
-- The state lives on the tenant-owned card row so a registration operation
-- containing several cards cannot make one card's storage failure hide another's
-- recovery state. Claims are made through the audited platform-admin pool and
-- all lifecycle transitions remain tenant-scoped.

ALTER TABLE wyrd.cards
    ADD COLUMN reconcile_kind TEXT NOT NULL DEFAULT 'registration',
    ADD COLUMN reconcile_status TEXT NOT NULL DEFAULT 'idle',
    ADD COLUMN reconcile_attempts INTEGER NOT NULL DEFAULT 0,
    ADD COLUMN reconcile_next_attempt_at TIMESTAMPTZ,
    ADD COLUMN reconcile_lease_owner UUID,
    ADD COLUMN reconcile_lease_expires_at TIMESTAMPTZ,
    ADD COLUMN reconcile_last_error_code TEXT,
    ADD COLUMN reconcile_last_error_message TEXT,
    ADD COLUMN reconcile_dead_lettered_at TIMESTAMPTZ;

ALTER TABLE wyrd.cards
    ADD CONSTRAINT cards_reconcile_kind_check CHECK (
        reconcile_kind IN ('registration', 'blob', 'finalization', 'cleanup')
    ),
    ADD CONSTRAINT cards_reconcile_status_check CHECK (
        reconcile_status IN ('idle', 'pending', 'leased', 'dead_lettered')
    ),
    ADD CONSTRAINT cards_reconcile_attempts_check CHECK (
        reconcile_attempts BETWEEN 0 AND 3
    ),
    ADD CONSTRAINT cards_reconcile_lease_check CHECK (
        (reconcile_status = 'leased' AND reconcile_lease_owner IS NOT NULL
            AND reconcile_lease_expires_at IS NOT NULL)
        OR reconcile_status <> 'leased'
    );

UPDATE wyrd.cards
   SET reconcile_status = 'pending',
       reconcile_next_attempt_at = COALESCE(pending_since, now())
 WHERE status = 'pending';

CREATE INDEX cards_reconciliation_claim_idx
    ON wyrd.cards (reconcile_status, reconcile_next_attempt_at, reconcile_lease_expires_at)
    WHERE reconcile_status IN ('pending', 'leased');
