-- Remove the cluster-wide Forge large-lane singleton lease.
--
-- Oversized single-file compactions previously serialized through the
-- singleton `vala.forge_large_lane_lease` row: one large task at a time across
-- every tenant and worker. That cluster-wide fence is the compaction-throughput
-- ceiling under multi-tenant debt (D78). Concurrency is now bounded by the
-- retained per-table publication index (`forge_tasks_publication_active`), the
-- per-tenant active cap, and a per-owner one-active-large rule enforced in the
-- fair-claim statement. The single-file CHECK and publication index are
-- unchanged authority and are intentionally preserved.

DROP FUNCTION vala.release_forge_large_lane(uuid, uuid, uuid);
DROP TABLE vala.forge_large_lane_lease;
