-- Execute-only authority for the fixed system-owned Oracle recovery audit.
-- The caller retains its operator transaction; this function exposes no tenant,
-- principal, operation, resource, or arbitrary payload input.

DO $$ BEGIN
  IF NOT EXISTS (SELECT 1 FROM pg_catalog.pg_roles WHERE rolname = 'wyrd_platform_admin') THEN
    RAISE EXCEPTION 'wyrd_platform_admin missing — run bootstrap/roles.sql / db:setup-roles';
  END IF;
END $$;

CREATE FUNCTION vala.append_oracle_admission_recovery_audit(
    p_request_id text,
    p_expired_lease_count bigint,
    p_active_lease_count bigint,
    p_interactive_slots bigint,
    p_analytical_slots bigint,
    p_total_slots bigint
) RETURNS bigint
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = ''
AS $$
DECLARE
    v_system_owner constant uuid := '00000000-0000-0000-0000-000000000000'::uuid;
    v_prev_hash bytea;
    v_seq bigint;
    v_detail text;
    v_preimage bytea;
    v_entry_hash bytea;
BEGIN
    IF p_request_id IS NULL OR p_request_id = '' THEN
        RAISE EXCEPTION 'Oracle recovery request id is required' USING ERRCODE = '22023';
    END IF;
    IF p_expired_lease_count IS NULL OR p_expired_lease_count < 0
       OR p_active_lease_count IS NULL OR p_active_lease_count < 0
       OR p_interactive_slots IS NULL OR p_interactive_slots < 0
       OR p_analytical_slots IS NULL OR p_analytical_slots < 0
       OR p_total_slots IS NULL OR p_total_slots < 0 THEN
        RAISE EXCEPTION 'Oracle recovery counters must be non-negative' USING ERRCODE = '22003';
    END IF;

    INSERT INTO vala.audit_chain_head (data_tenant_id)
    VALUES (v_system_owner)
    ON CONFLICT (data_tenant_id) DO NOTHING;

    SELECT last_seq, head_hash
      INTO STRICT v_seq, v_prev_hash
      FROM vala.audit_chain_head
     WHERE data_tenant_id = v_system_owner
    FOR UPDATE;

    IF v_seq = 9223372036854775807 THEN
        RAISE EXCEPTION 'Oracle recovery audit sequence exhausted' USING ERRCODE = '22003';
    END IF;
    v_seq := v_seq + 1;
    v_detail := pg_catalog.format(
        '{"active_lease_count":%s,"analytical_slots":%s,"expired_lease_count":%s,"interactive_slots":%s,"kind":"oracle_admission_recovery","total_slots":%s}',
        p_active_lease_count,
        p_analytical_slots,
        p_expired_lease_count,
        p_interactive_slots,
        p_total_slots
    );

    -- This mirrors `audit_outbox::entry_hash`: big-endian sequence, one-byte
    -- option tags, big-endian UTF-8 byte lengths, then exact field bytes.
    v_preimage := v_prev_hash || pg_catalog.int8send(v_seq);
    v_preimage := v_preimage || pg_catalog.int8send(pg_catalog.octet_length(pg_catalog.convert_to(p_request_id, 'UTF8'))) || pg_catalog.convert_to(p_request_id, 'UTF8');
    v_preimage := v_preimage || '\x00'::bytea;
    v_preimage := v_preimage || pg_catalog.int8send(33) || pg_catalog.convert_to('bifrost.oracle.admission_recovery', 'UTF8');
    v_preimage := v_preimage || pg_catalog.int8send(24) || pg_catalog.convert_to('bifrost.oracle.admission', 'UTF8');
    v_preimage := v_preimage || '\x00'::bytea;
    v_preimage := v_preimage || pg_catalog.uuid_send('00000000-0000-0000-0000-000000000000'::uuid);
    v_preimage := v_preimage || pg_catalog.int8send(7) || pg_catalog.convert_to('service', 'UTF8');
    v_preimage := v_preimage || pg_catalog.int8send(8) || pg_catalog.convert_to('internal', 'UTF8');
    v_preimage := v_preimage || pg_catalog.int8send(14) || pg_catalog.convert_to('bifrost:oracle', 'UTF8');
    v_preimage := v_preimage || pg_catalog.int8send(5) || pg_catalog.convert_to('allow', 'UTF8');
    v_preimage := v_preimage || pg_catalog.int8send(7) || pg_catalog.convert_to('success', 'UTF8');
    v_preimage := v_preimage || pg_catalog.int8send(37) || pg_catalog.convert_to('recovered Oracle admission aggregates', 'UTF8');
    v_preimage := v_preimage || '\x01'::bytea || pg_catalog.int8send(pg_catalog.octet_length(pg_catalog.convert_to(v_detail, 'UTF8'))) || pg_catalog.convert_to(v_detail, 'UTF8');
    v_entry_hash := pg_catalog.sha256(v_preimage);

    INSERT INTO vala.audit_outbox
        (data_tenant_id, seq, prev_hash, entry_hash, request_id, trace_id,
         operation, resource, card_ref, principal_id, principal_kind,
         auth_method, permission, decision, result, payload_summary, detail)
    VALUES
        (v_system_owner, v_seq, v_prev_hash, v_entry_hash, p_request_id, NULL,
         'bifrost.oracle.admission_recovery', 'bifrost.oracle.admission', NULL,
         '00000000-0000-0000-0000-000000000000'::uuid, 'service', 'internal',
         'bifrost:oracle', 'allow', 'success',
         'recovered Oracle admission aggregates', v_detail);

    UPDATE vala.audit_chain_head
       SET last_seq = v_seq, head_hash = v_entry_hash, updated_at = pg_catalog.now()
     WHERE data_tenant_id = v_system_owner;

    RETURN v_seq;
END;
$$;

ALTER FUNCTION vala.append_oracle_admission_recovery_audit(text, bigint, bigint, bigint, bigint, bigint)
    OWNER TO wyrd_migrator;
REVOKE ALL ON FUNCTION vala.append_oracle_admission_recovery_audit(text, bigint, bigint, bigint, bigint, bigint)
    FROM PUBLIC;
GRANT EXECUTE ON FUNCTION vala.append_oracle_admission_recovery_audit(text, bigint, bigint, bigint, bigint, bigint)
    TO wyrd_platform_admin;
