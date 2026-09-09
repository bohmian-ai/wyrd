-- Durable bounded Forge failure qualification.

ALTER TABLE vala.forge_tasks
    ADD COLUMN attempt_count integer NOT NULL DEFAULT 0 CHECK (attempt_count >= 0),
    ADD COLUMN failure_class text CHECK (failure_class IN ('data_refusal','transient_object_store','transient_coordination','storage_health','capacity_refused','internal_invariant')),
    ADD COLUMN next_eligible_at timestamptz NOT NULL DEFAULT statement_timestamp();

CREATE INDEX forge_tasks_retry_eligibility
    ON vala.forge_tasks (next_eligible_at, ready_at)
    WHERE state IN ('ready', 'retryable');
