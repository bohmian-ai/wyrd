-- vala.scribe_batch_commits: the table version each batch was accepted under.
--
-- A retry of an accepted batch is compared under the version that accepted
-- it, never the table's current one. Rows fenced before this column existed
-- keep NULL: their version is unknown, so a retry that names any version
-- contradicts them and is refused rather than matched against a guess.

ALTER TABLE vala.scribe_batch_commits
    ADD COLUMN table_version bigint CHECK (table_version > 0);
