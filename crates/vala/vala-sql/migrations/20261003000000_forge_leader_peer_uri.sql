-- The elected Forge leader publishes its private peer URI beside its owner and
-- fencing token, so a remote compactor dials only the current live term.
ALTER TABLE vala.forge_scheduler_state ADD COLUMN peer_uri text
    CHECK (peer_uri IS NULL OR peer_uri <> '');
