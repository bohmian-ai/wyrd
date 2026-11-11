-- Gateway quota usage visible across a rolling deployment.
--
-- Servers of the previous release charge the strict admission tables
-- (gateway_limit_windows, gateway_call_leases) through wyrd.gateway_admit and
-- wyrd.gateway_settle; current servers write absolute per-server rows to
-- gateway_usage_windows and gateway_usage_concurrency. While both run, each
-- reader must count the other's committed usage once. Current servers read
-- the strict tables beside the contribution rows (see
-- wyrd_sql::queries::gateway_admission); this migration makes the previous
-- release's admission read the contribution rows beside its own.
--
-- Only the reads change: wyrd.gateway_admit still writes, and
-- wyrd.gateway_settle still settles and prunes, the strict tables alone, so
-- no charge is written twice. Budget spend was already common: both
-- releases read the same accounting ledger.

-- Unchanged from 20261023000001 except that a limit's window counts and
-- running calls also sum every current server's unexpired contribution rows.
CREATE OR REPLACE FUNCTION wyrd.gateway_admit(
    p_data_tenant_id uuid,
    p_call_id uuid,
    p_window_start timestamptz,
    p_now timestamptz,
    p_expires_at timestamptz,
    p_limits jsonb,
    p_budgets jsonb
) RETURNS TABLE (exhausted text[], budget_exceeded boolean)
LANGUAGE plpgsql
VOLATILE
SECURITY INVOKER
SET search_path = ''
AS $$
DECLARE
    v_limit jsonb;
    v_budget jsonb;
    v_requests bigint;
    v_tokens bigint;
    v_leases bigint;
    v_spend numeric;
    v_exhausted text[] := '{}';
BEGIN
    IF pg_catalog.current_setting('app.current_tenant', true)
           NOT IN ('', p_data_tenant_id::text) THEN
        RAISE EXCEPTION 'gateway admission tenant differs from the bound tenant'
            USING ERRCODE = 'invalid_parameter_value';
    END IF;
    PERFORM pg_catalog.set_config('app.current_tenant', p_data_tenant_id::text, true);
    PERFORM pg_catalog.pg_advisory_xact_lock(
        pg_catalog.hashtextextended('wyrd.gateway_admission:' || p_data_tenant_id::text, 0));

    FOR v_limit IN SELECT value FROM pg_catalog.jsonb_array_elements(p_limits) LOOP
        SELECT COALESCE(SUM(u.requests), 0), COALESCE(SUM(u.tokens), 0)
          INTO v_requests, v_tokens
          FROM (
              SELECT w.requests, w.tokens
                FROM wyrd.gateway_limit_windows w
               WHERE w.data_tenant_id = p_data_tenant_id
                 AND w.limit_key = v_limit ->> 'key'
                 AND w.window_start = p_window_start
              UNION ALL
              SELECT c.requests, c.tokens
                FROM wyrd.gateway_usage_windows c
               WHERE c.data_tenant_id = p_data_tenant_id
                 AND c.limit_key = v_limit ->> 'key'
                 AND c.window_start = p_window_start
          ) AS u;
        SELECT (SELECT count(*)
                  FROM wyrd.gateway_call_leases l
                 WHERE l.data_tenant_id = p_data_tenant_id
                   AND l.limit_key = v_limit ->> 'key'
                   AND l.expires_at > p_now)
             + (SELECT COALESCE(SUM(c.active), 0)
                  FROM wyrd.gateway_usage_concurrency c
                 WHERE c.data_tenant_id = p_data_tenant_id
                   AND c.limit_key = v_limit ->> 'key'
                   AND c.expires_at > p_now)
          INTO v_leases;
        IF (v_limit ->> 'requests')::bigint <= v_requests
           OR (v_limit ->> 'tokens')::bigint < v_tokens + (v_limit ->> 'hold')::bigint
           OR (v_limit ->> 'concurrent')::bigint <= v_leases THEN
            v_exhausted := v_exhausted || (v_limit ->> 'key');
        END IF;
    END LOOP;
    IF pg_catalog.cardinality(v_exhausted) > 0 THEN
        RETURN QUERY SELECT v_exhausted, false;
        RETURN;
    END IF;

    FOR v_budget IN SELECT value FROM pg_catalog.jsonb_array_elements(p_budgets) LOOP
        SELECT COALESCE(SUM(COALESCE(
                   (s.entry -> 'budget_reservation_settled' ->> 'actual_cost')::numeric,
                   (c.entry -> 'budget_reservation_created' ->> 'reserved_cost')::numeric
               )), 0)
          INTO v_spend
          FROM wyrd.gateway_accounting_entries c
          LEFT JOIN wyrd.gateway_accounting_entries s
            ON s.data_tenant_id = c.data_tenant_id
           AND s.kind = 'budget_reservation_settled'
           AND s.entry -> 'budget_reservation_settled' ->> 'reservation_id'
             = c.entry -> 'budget_reservation_created' ->> 'reservation_id'
         WHERE c.data_tenant_id = p_data_tenant_id
           AND c.kind = 'budget_reservation_created'
           AND c.entry -> 'budget_reservation_created' ->> 'period_start'
             = v_budget ->> 'period_start'
           AND c.entry -> 'budget_reservation_created' ->> 'period_end'
             = v_budget ->> 'period_end'
           AND c.entry -> 'budget_reservation_created' -> 'subject' = v_budget -> 'subject';
        IF v_spend + (v_budget ->> 'reserve')::numeric > (v_budget ->> 'amount')::numeric THEN
            RETURN QUERY SELECT v_exhausted, true;
            RETURN;
        END IF;
    END LOOP;

    FOR v_limit IN SELECT value FROM pg_catalog.jsonb_array_elements(p_limits) LOOP
        INSERT INTO wyrd.gateway_limit_windows AS w (
            data_tenant_id, limit_key, window_start, requests, tokens
        ) VALUES (
            p_data_tenant_id, v_limit ->> 'key', p_window_start, 1, (v_limit ->> 'hold')::bigint
        )
        ON CONFLICT (data_tenant_id, limit_key, window_start) DO UPDATE SET
            requests = w.requests + 1,
            tokens = w.tokens + EXCLUDED.tokens;
        IF (v_limit ->> 'lease')::boolean THEN
            INSERT INTO wyrd.gateway_call_leases (data_tenant_id, limit_key, call_id, expires_at)
            VALUES (p_data_tenant_id, v_limit ->> 'key', p_call_id, p_expires_at)
            ON CONFLICT DO NOTHING;
        END IF;
    END LOOP;
    INSERT INTO wyrd.gateway_accounting_entries (data_tenant_id, entry_id, call_id, kind, entry)
    SELECT p_data_tenant_id, (b ->> 'entry_id')::uuid, p_call_id,
           'budget_reservation_created', b -> 'entry'
      FROM pg_catalog.jsonb_array_elements(p_budgets) AS b
     WHERE b -> 'entry' IS NOT NULL AND b -> 'entry' <> 'null'::jsonb
    ON CONFLICT DO NOTHING;
    RETURN QUERY SELECT v_exhausted, false;
END;
$$;
