-- Oracle cut invalidation.
--
-- Oracle nodes cache each table's read cut (catalog pointer plus unresolved
-- file_list candidates) and drop it when this channel names the table. Every
-- write that changes which rows a cut sees changes vala.file_list: Scribe's
-- staging publication inserts candidates, Forge's promotion settlement marks
-- them committed, and expired cleanup deletes settled rows. NOTIFY is
-- transactional, so a listener hears it only after the write commits and
-- never for a rolled-back one; identical payloads in one transaction are
-- delivered once.
--
-- Payload: '<data_tenant_id>|<namespace>|<table_name>'. Pointer moves that
-- leave the candidates alone (compaction rewrite, expiry, manifest rewrite)
-- send nothing: the older pointer still returns the same rows.

CREATE FUNCTION vala.notify_bifrost_cut() RETURNS trigger
LANGUAGE plpgsql
SET search_path = ''
AS $$
DECLARE
    v_row vala.file_list;
BEGIN
    IF TG_OP = 'DELETE' THEN
        v_row := OLD;
    ELSE
        v_row := NEW;
    END IF;
    PERFORM pg_catalog.pg_notify(
        'bifrost_cut',
        v_row.data_tenant_id::text || '|' || v_row.namespace || '|' || v_row.table_name);
    RETURN NULL;
END
$$;

REVOKE ALL ON FUNCTION vala.notify_bifrost_cut() FROM PUBLIC;

CREATE TRIGGER file_list_notify_bifrost_cut
    AFTER INSERT OR UPDATE OR DELETE ON vala.file_list
    FOR EACH ROW EXECUTE FUNCTION vala.notify_bifrost_cut();
