-- A terminal Verifier execution owes its Bifrost execution state its
-- terminal record.
--
-- A cancelled, revoked, refused, timed-out, or errored execution stops
-- computing the moment its terminal status commits, but its decision is part
-- of its retained state only once a Scribe acknowledgement covers it. Until
-- then `delivery_due_at` keeps that delivery claimable: a runner claims it by
-- moving the instant one lease ahead, records the decision without
-- computing, and clears it once acknowledged, so the delivery survives every
-- process interruption. Only the writers settling a terminal status without
-- a verdict set it; executions settled before this migration keep their
-- queue record only.
ALTER TABLE wyrd.verifier_executions
    ADD COLUMN delivery_due_at timestamptz;

CREATE INDEX verifier_executions_delivery_due
    ON wyrd.verifier_executions (data_tenant_id, delivery_due_at)
    WHERE delivery_due_at IS NOT NULL;
