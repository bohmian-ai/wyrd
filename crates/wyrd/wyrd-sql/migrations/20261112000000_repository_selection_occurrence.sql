-- Fence a Repository selection by its occurrence, not only its version.
--
-- Selecting A, then B, then deliberately A again leaves the same selected
-- version twice. `selection_occurrence` names each selection change by the
-- operation that made it. A declaration records the version and occurrence
-- it was written from and selects only while both are still current, so a
-- delayed declaration written from the first A cannot overwrite the later
-- rollback. Clearing a selection clears its occurrence with it.

ALTER TABLE wyrd.cards ADD COLUMN selection_occurrence UUID;

ALTER TABLE wyrd.cards ADD CONSTRAINT cards_selection_occurrence CHECK (
    (selected_at IS NULL) = (selection_occurrence IS NULL)
);

ALTER TABLE wyrd.card_declaration_operations
    ADD COLUMN selection_expected_occurrence UUID;
