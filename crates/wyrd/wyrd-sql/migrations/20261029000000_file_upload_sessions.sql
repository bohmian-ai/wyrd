-- A verification File upload shares the upload session lifecycle but has no
-- owning Card: a NULL card_uid marks File ownership.

ALTER TABLE wyrd.storage_multipart_uploads ALTER COLUMN card_uid DROP NOT NULL;
