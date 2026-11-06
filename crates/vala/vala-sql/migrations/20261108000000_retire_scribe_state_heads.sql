-- Retire `vala.scribe_state_heads`.
--
-- Verifier execution state no longer has a PostgreSQL head: each accepted
-- revision is a `bifrost.verification.execution_events` row written under a
-- batch identity derived from its execution and revision, so the generic
-- `vala.scribe_batch_commits` fences already name every accepted revision
-- and the canonical row holds its state. The head held no payload, so
-- dropping it loses no accepted data.
DROP TABLE vala.scribe_state_heads;
