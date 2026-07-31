-- Durable, tenant-aware Forge scheduling and attempt state.

CREATE TABLE vala.forge_tasks (
    task_id uuid PRIMARY KEY,
    data_tenant_id uuid NOT NULL,
    catalog_name text NOT NULL CHECK (catalog_name = 'wyrd-redux'),
    namespace_name text NOT NULL CHECK (namespace_name IN ('vala.system','vala.bifrost','vala.traces','vala.metrics','vala.logs','vala.genai','vala.eval','vala.drift','vala.dev','vala.datasets')),
    table_name text NOT NULL CHECK (table_name <> '' AND length(table_name) <= 63 AND table_name ~ '^[A-Za-z0-9_.-]+$' AND table_name NOT LIKE '%..%' AND table_name !~ '^\\.|\\.$'),
    strategy text NOT NULL CHECK (strategy IN ('staging_fold','small_files','full_identity','manifest_rewrite','snapshot_expiry','expired_cleanup','orphan_cleanup')),
    lane text NOT NULL CHECK (lane IN ('ordinary','large_singleton')),
    base_snapshot_id bigint NOT NULL,
    plan jsonb NOT NULL,
    plan_hash bytea NOT NULL CHECK (octet_length(plan_hash) = 32),
    estimated_files bigint NOT NULL CHECK (estimated_files > 0),
    estimated_bytes bigint NOT NULL CHECK (estimated_bytes > 0),
    estimated_parallelism integer NOT NULL CHECK (estimated_parallelism > 0),
    estimated_memory_bytes bigint NOT NULL CHECK (estimated_memory_bytes > 0),
    estimated_spill_bytes bigint NOT NULL CHECK (estimated_spill_bytes > 0),
    large_task_ceiling_bytes bigint NOT NULL CHECK (large_task_ceiling_bytes > 0),
    state text NOT NULL CHECK (state IN ('ready','claimed','running','prepared','succeeded','retryable','unschedulable','failed','cancelled')),
    attempt_id uuid,
    claimed_by uuid,
    claim_expires_at timestamptz,
    watermark_snapshot_id bigint,
    watermark_timestamp_ms bigint CHECK (watermark_timestamp_ms >= 0),
    evidence jsonb,
    ready_at timestamptz NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CHECK ((watermark_snapshot_id IS NULL) = (watermark_timestamp_ms IS NULL)),
    CHECK ((state IN ('claimed','running','prepared')) = (attempt_id IS NOT NULL AND claimed_by IS NOT NULL AND claim_expires_at IS NOT NULL)),
    CHECK (
      (state = 'claimed' AND watermark_snapshot_id IS NULL)
      OR (state IN ('running','prepared') AND watermark_snapshot_id IS NOT NULL)
      OR (state NOT IN ('claimed','running','prepared') AND watermark_snapshot_id IS NULL)
    ),
    CHECK (state <> 'prepared' OR evidence IS NOT NULL)
);

ALTER TABLE vala.forge_tasks ENABLE ROW LEVEL SECURITY;
ALTER TABLE vala.forge_tasks FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON vala.forge_tasks
    USING (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());

CREATE UNIQUE INDEX forge_tasks_idempotency ON vala.forge_tasks
 (data_tenant_id, catalog_name, namespace_name, table_name, strategy, base_snapshot_id, plan_hash);
CREATE UNIQUE INDEX forge_tasks_publication_active ON vala.forge_tasks
 (data_tenant_id, catalog_name, namespace_name, table_name)
 WHERE state IN ('claimed','running','prepared');
CREATE INDEX forge_tasks_ready ON vala.forge_tasks
 (data_tenant_id, ready_at, task_id)
 WHERE state IN ('ready','retryable');
CREATE INDEX forge_tasks_attempt ON vala.forge_tasks (attempt_id) WHERE attempt_id IS NOT NULL;
CREATE INDEX forge_tasks_watermark ON vala.forge_tasks
 (data_tenant_id, catalog_name, namespace_name, table_name, watermark_timestamp_ms)
 WHERE state IN ('running','prepared');
CREATE INDEX forge_tasks_terminal ON vala.forge_tasks
 (data_tenant_id, updated_at, task_id)
 WHERE state IN ('succeeded','unschedulable','failed','cancelled');
CREATE INDEX forge_tasks_status ON vala.forge_tasks
 (data_tenant_id, updated_at DESC, task_id);

CREATE TABLE vala.forge_scheduler_state (
    singleton boolean PRIMARY KEY DEFAULT true CHECK (singleton),
    owner uuid,
    fencing_token bigint NOT NULL DEFAULT 0 CHECK (fencing_token >= 0),
    expires_at timestamptz,
    last_tenant_id uuid,
    updated_at timestamptz NOT NULL DEFAULT now()
);
INSERT INTO vala.forge_scheduler_state (singleton) VALUES (true);

CREATE TABLE vala.forge_large_lane_lease (
    singleton boolean PRIMARY KEY DEFAULT true CHECK (singleton),
    task_id uuid,
    owner uuid,
    attempt_id uuid,
    fencing_token bigint NOT NULL DEFAULT 0 CHECK (fencing_token >= 0),
    expires_at timestamptz,
    updated_at timestamptz NOT NULL DEFAULT now(),
    CHECK ((task_id IS NULL) = (owner IS NULL)),
    CHECK ((task_id IS NULL) = (attempt_id IS NULL)),
    CHECK ((task_id IS NULL) = (expires_at IS NULL))
);
INSERT INTO vala.forge_large_lane_lease (singleton) VALUES (true);

CREATE FUNCTION vala.release_forge_large_lane(p_task_id uuid, p_attempt_id uuid, p_owner uuid)
RETURNS boolean
LANGUAGE sql
SECURITY DEFINER
SET search_path = pg_catalog
AS $$
  WITH released AS (
    UPDATE vala.forge_large_lane_lease
       SET task_id = NULL, owner = NULL, attempt_id = NULL,
           expires_at = NULL, updated_at = statement_timestamp()
     WHERE singleton AND task_id = p_task_id
       AND attempt_id = p_attempt_id AND owner = p_owner
     RETURNING singleton
  )
  SELECT EXISTS (SELECT 1 FROM released)
$$;
REVOKE ALL ON FUNCTION vala.release_forge_large_lane(uuid, uuid, uuid) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION vala.release_forge_large_lane(uuid, uuid, uuid) TO wyrd_app, wyrd_platform_admin;

GRANT SELECT, INSERT, UPDATE, DELETE ON vala.forge_tasks TO wyrd_platform_admin;
GRANT SELECT, UPDATE, DELETE ON vala.forge_tasks TO wyrd_app;
GRANT SELECT, UPDATE ON vala.forge_scheduler_state, vala.forge_large_lane_lease TO wyrd_platform_admin;
