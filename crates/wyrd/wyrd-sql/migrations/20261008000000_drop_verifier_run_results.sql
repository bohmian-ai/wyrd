-- Retire staged Verifier results.
--
-- A completed run now stages its result on the process's in-memory Scribe
-- outbox and settles; no result is held in PostgreSQL before Bifrost.
DROP TABLE wyrd.verifier_run_results;
