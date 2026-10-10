-- Retire audit staging and its publication state.
--
-- Audit decisions are now staged on the process's in-memory Scribe outbox and
-- written straight to vala.system.audit_log; nothing is held in PostgreSQL
-- before Bifrost, and the gapless chain is gone with its staging rows.
DROP TABLE vala.audit_publication;
DROP TABLE vala.audit_staging;
DROP TABLE vala.audit_chain_head;
DROP FUNCTION vala.audit_staging_immutable();
