-- Manual Verifier run idempotency.
--
-- `POST /v1/verification/runs` honors the existing `Idempotency-Key` header:
-- a retried request with the same key and body refers to the run the first
-- request created, and reusing the key for a different body is refused. The
-- key belongs to the run it created, so it lives on that run row rather than
-- in a separate expiring cache. Keys are scoped to the requesting principal,
-- matching Card registration, so two callers never collide on a key.
ALTER TABLE wyrd.verifier_runs
    ADD COLUMN idempotency_key TEXT,
    ADD COLUMN request_sha256  BYTEA,
    -- A key always carries the request digest it was first used with, and
    -- only an authenticated manual request can supply one.
    ADD CONSTRAINT verifier_runs_manual_idempotency_shape CHECK (
        (idempotency_key IS NULL) = (request_sha256 IS NULL)
        AND (idempotency_key IS NULL OR origin = 'manual')
    );

CREATE UNIQUE INDEX verifier_runs_manual_idempotency
    ON wyrd.verifier_runs (data_tenant_id, requested_by_principal_id, idempotency_key)
    WHERE idempotency_key IS NOT NULL;
