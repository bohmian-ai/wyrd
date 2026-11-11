-- Fence a Repository selection by its occurrence, not only its version.
--
-- Selecting A, then B, then deliberately A again leaves the same selected
-- version twice. `selection_occurrence` names each selection change: the
-- operation that made it, or a unique backfilled value for an existing
-- selection. A declaration records the version and occurrence it was written
-- from and selects only while both are still current, so a delayed
-- declaration written from the first A cannot overwrite the later rollback.
-- Clearing a selection clears its occurrence with it.

ALTER TABLE wyrd.cards ADD COLUMN selection_occurrence UUID;

UPDATE wyrd.cards SET selection_occurrence = gen_random_uuid()
 WHERE selected_at IS NOT NULL;

ALTER TABLE wyrd.cards ADD CONSTRAINT cards_selection_occurrence CHECK (
    (selected_at IS NULL) = (selection_occurrence IS NULL)
);

ALTER TABLE wyrd.card_declaration_operations
    ADD COLUMN selection_expected_occurrence UUID;

-- A pending declaration keeps the occurrence its expected version holds now;
-- one whose expected version is no longer selected gets an occurrence nothing
-- holds, so it still settles as a conflict.
UPDATE wyrd.card_declaration_operations o
   SET selection_expected_occurrence = COALESCE(
       (SELECT c.selection_occurrence FROM wyrd.cards c
         WHERE c.data_tenant_id = o.data_tenant_id AND c.card_uid = o.selection_expected_uid
           AND c.selected_at IS NOT NULL),
       gen_random_uuid())
 WHERE o.selection_expected_uid IS NOT NULL;
