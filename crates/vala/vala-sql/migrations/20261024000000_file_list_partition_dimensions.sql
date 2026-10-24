-- vala.file_list: the partition dimensions beside the time partition.
--
-- A table may partition on dimensions beyond time, so the time columns alone
-- no longer identify a file's physical partition. partition_dimensions holds
-- the writer's canonical encoding of everything but the time partition: the
-- partition-spec id, the time field's position, and each typed dimension
-- value. A file's partition is (partition_granularity, partition_start,
-- partition_dimensions), compared whole. The default is the encoding of a
-- time-only partition under the initial spec, which every earlier row is.

ALTER TABLE vala.file_list
    ADD COLUMN partition_dimensions bytea NOT NULL DEFAULT '\x000000000000'::bytea;
