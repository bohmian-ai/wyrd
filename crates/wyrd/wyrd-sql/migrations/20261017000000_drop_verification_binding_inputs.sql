-- Retire schedule-binding sample selectors.
--
-- A scheduled execution reads every samples slot its exact Verifier declares
-- for the subject's window, so a binding no longer stores a second selector
-- map for those slots.
ALTER TABLE wyrd.verification_bindings DROP COLUMN inputs;
