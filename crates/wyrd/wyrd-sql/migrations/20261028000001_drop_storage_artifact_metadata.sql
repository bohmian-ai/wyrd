-- Completed uploaded-object metadata lives only in bifrost.storage.objects.
-- Upload sessions and Card manifests stay here as transfer and declaration
-- coordination; this table is no longer an inventory.

DROP TABLE wyrd.storage_artifact_metadata;
