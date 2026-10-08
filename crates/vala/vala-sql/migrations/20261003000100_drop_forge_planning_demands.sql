-- The elected Forge leader schedules maintenance from its in-memory roster on
-- an hourly timer, so the durable planning-demand queue and the planner's
-- round-robin tenant cursor have no remaining reader or writer.
DROP TABLE vala.forge_planning_demands;
ALTER TABLE vala.forge_scheduler_state DROP COLUMN last_tenant_id;
