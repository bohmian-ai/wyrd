-- Gateway admission and settlement, each in one round trip.
--
-- Both functions bind p_data_tenant_id as the transaction-local RLS tenant
-- themselves, so a caller runs each as one autocommit statement with no BEGIN
-- or COMMIT, and the binding ends with the statement's implicit transaction.
-- Inside a transaction that already bound a different tenant they refuse
-- rather than rebinding. They run with the caller's privileges, so RLS bounds
-- every row.
--
-- The server calls them only for a call that a limit or budget covers; a call
-- nothing covers touches no admission table.

-- Checks and charges one call's limits and budgets under the tenant's
-- admission advisory lock, so concurrent admissions on every replica observe
-- each other's committed charges and never oversubscribe. Each statement of
-- this VOLATILE function reads a fresh snapshot, so every read after the lock
-- sees every charge committed before it.
--
-- p_limits: [{key, requests, tokens, concurrent, hold, lease}], where the
--   three caps are null when unset, hold is the call's token exposure under
--   the limit, and lease whether the limit counts concurrency.
-- p_budgets: [{subject, period_start, period_end, amount, reserve, entry_id,
--   entry}], where entry is the reservation-creation ledger document, or null
--   when the reserve is zero.
--
-- Result: the keys of every exhausted limit, or, when none is, whether a
-- budget cannot cover its reserve. Nothing is charged unless both are clear.
CREATE FUNCTION wyrd.gateway_admit(
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
        SELECT COALESCE(w.requests, 0), COALESCE(w.tokens, 0)
          INTO v_requests, v_tokens
          FROM (SELECT 1) AS one
          LEFT JOIN wyrd.gateway_limit_windows w
            ON w.data_tenant_id = p_data_tenant_id
           AND w.limit_key = v_limit ->> 'key'
           AND w.window_start = p_window_start;
        SELECT count(*) INTO v_leases
          FROM wyrd.gateway_call_leases l
         WHERE l.data_tenant_id = p_data_tenant_id
           AND l.limit_key = v_limit ->> 'key'
           AND l.expires_at > p_now;
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

-- Settles one accounted call: replaces each token hold in its admission
-- window by the delta to actual tokens (never below zero), appends each
-- reservation settlement (a replay appends nothing), releases the call's
-- leases when it took any, and prunes the tenant's windows older than
-- p_window_floor and leases lapsed at p_now, which no admission reads.
--
-- p_holds: [{key, delta}]; p_settlements: [{entry_id, entry}].
CREATE FUNCTION wyrd.gateway_settle(
    p_data_tenant_id uuid,
    p_call_id uuid,
    p_window_start timestamptz,
    p_window_floor timestamptz,
    p_now timestamptz,
    p_holds jsonb,
    p_settlements jsonb,
    p_release boolean
) RETURNS void
LANGUAGE plpgsql
VOLATILE
SECURITY INVOKER
SET search_path = ''
AS $$
BEGIN
    IF pg_catalog.current_setting('app.current_tenant', true)
           NOT IN ('', p_data_tenant_id::text) THEN
        RAISE EXCEPTION 'gateway settlement tenant differs from the bound tenant'
            USING ERRCODE = 'invalid_parameter_value';
    END IF;
    PERFORM pg_catalog.set_config('app.current_tenant', p_data_tenant_id::text, true);

    INSERT INTO wyrd.gateway_limit_windows AS w (
        data_tenant_id, limit_key, window_start, requests, tokens
    )
    SELECT p_data_tenant_id, h ->> 'key', p_window_start, 0,
           GREATEST((h ->> 'delta')::bigint, 0)
      FROM pg_catalog.jsonb_array_elements(p_holds) AS h
    ON CONFLICT (data_tenant_id, limit_key, window_start) DO UPDATE SET
        tokens = GREATEST(w.tokens + (
            SELECT (h ->> 'delta')::bigint
              FROM pg_catalog.jsonb_array_elements(p_holds) AS h
             WHERE h ->> 'key' = EXCLUDED.limit_key
             LIMIT 1
        ), 0);
    INSERT INTO wyrd.gateway_accounting_entries (data_tenant_id, entry_id, call_id, kind, entry)
    SELECT p_data_tenant_id, (s ->> 'entry_id')::uuid, p_call_id,
           'budget_reservation_settled', s -> 'entry'
      FROM pg_catalog.jsonb_array_elements(p_settlements) AS s
    ON CONFLICT DO NOTHING;
    IF p_release THEN
        DELETE FROM wyrd.gateway_call_leases
         WHERE data_tenant_id = p_data_tenant_id AND call_id = p_call_id;
    END IF;
    DELETE FROM wyrd.gateway_limit_windows
     WHERE data_tenant_id = p_data_tenant_id AND window_start < p_window_floor;
    DELETE FROM wyrd.gateway_call_leases
     WHERE data_tenant_id = p_data_tenant_id AND expires_at <= p_now;
END;
$$;
