-- Persist Forge progress-effect acknowledgement beside each planning demand.

ALTER TABLE vala.forge_planning_demands
    ADD COLUMN acknowledged_snapshot_id bigint,
    ADD COLUMN acknowledged_commit_count bigint
        CHECK (acknowledged_commit_count IS NULL OR acknowledged_commit_count >= 0);

