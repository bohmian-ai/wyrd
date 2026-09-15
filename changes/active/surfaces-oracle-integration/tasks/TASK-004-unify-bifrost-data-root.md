---
id: TASK-004
kind: implementation
status: implemented
spec: SPEC-surfaces-oracle-integration
spec_revision: 5
requirements: [REQ-055, REQ-055A, INV-022, AC-018]
depends_on: [TASK-001]
parent_task:
remediates: []
---

## Objective

After the core Redux integration, make `WYRD_BIFROST_DATA_DIR` the one server
configuration root for every Bifrost-managed local path and prove boot,
readiness, restart, and replica ownership behavior. This outcome blocks final
integration closeout.

## Constraints

- Derive all managed Scribe and Oracle local paths from one root, defaulting
  locally to `.wyrd/bifrost`.
- Remove `WYRD_SCRIBE_WAL_DIR` without aliases or compatibility fallback.
- Create required managed paths before role activation and fail before
  readiness when the resolved root is unusable.
- Durable replicas must not share one writable WAL identity. Tenant, node,
  writer epoch, ownership, replay, and fencing semantics cannot weaken.
- Forge has no spill or scratch child path; do not invent one to make the root
  appear exhaustive.
- This task depends on the integrated server, Scribe, Oracle, and deployment
  owners from TASK-001 and completes before TASK-003 closeout.
- There is no such thing as a pre-existing failure anymore. All failures must be explicitly handled within the current execution context.

## Relevant Surface

- Bifrost server configuration and boot/readiness owners
- Redux Scribe WAL and Oracle spill path ownership
- Deployment configuration, examples, and operations documentation
- Bifrost server, Scribe, Oracle, restart, and multi-replica journeys

Paths are ownership guidance, not a private implementation allowlist.

## Approach

1. Resolve the single root once at the server boundary and pass derived owned
   paths into the integrated Scribe and Oracle owners.
2. Remove legacy independent settings and update deployment configuration and
   documentation to expose only `WYRD_BIFROST_DATA_DIR`.
3. Create and validate managed directories before activating any selected
   Bifrost role; connect readiness to usable and healthy local ownership.
4. Preserve per-replica WAL identity, restart recovery, fencing, and shutdown
   behavior for default and overridden roots.
5. Add the smallest production-shaped configuration, boot, restart, and
   multi-replica evidence needed to prove the outcome.

## Acceptance Criteria

- With no override, every Bifrost-managed local path is created below
  `.wyrd/bifrost`; with an override, every path is created below the resolved
  `WYRD_BIFROST_DATA_DIR`.
- Scribe and Oracle activate only after their derived paths are usable. An
  invalid, unwritable, contradictory, or unavailable root prevents readiness
  with a structured failure and no partial role activation.
- The removed Scribe and Oracle settings are rejected or ignored as absent
  legacy configuration according to the current configuration parser; no
  alias, fallback, or second root survives.
- Restart from the same root recovers acknowledged Scribe state without loss, duplication beyond approved replay semantics,
  or authority widening.
- Multiple durable replicas use distinct writable WAL identities even when
  configured beneath the same deployment-level storage location; no process
  opens another node's local WAL path.
- Forge remains free of a spill/scratch child path and all deployment targets
  preserve their approved memory and readiness behavior.

## Verification

- `mise run fmt`
- `mise run lints`
- `mise run check:bifrost-oracle-deploy`
- `mise run check:bifrost-resource-governance`
- `mise run check:unwrap-audit`
- `git diff --check`

Run and record exact focused `mise exec -- cargo nextest run` commands for the
configuration, boot, readiness, restart, and multi-replica scenarios added or
changed here. Do not run a Bifrost aggregate in this task.

## Implementation Evidence

Commits: `e3ddd2ec8` (single locked data root), `89ea8c325` (Scribe settlement
assertions ignore the retained-audit publisher's own buckets), `ea280654b`
(Oracle follower task-cache cycle that leaked each graph's spill directory,
exposed once spill paths moved under the locked root), `208d1452e` (keeps the
data-root lock binding ahead of `compaction_runtime` so
`app::tests::forge_runtime_lives_until_worker_supervision_drains` holds).

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Every managed path below `.wyrd/bifrost` or `WYRD_BIFROST_DATA_DIR` | `wyrd-server/src/boot/data_root.rs`, `config.rs` | `boot::data_root::tests::prepare_creates_every_managed_path_below_the_root`; `config::tests::bifrost_data_dir_defaults_and_follows_environment` | PASS |
| Invalid, unwritable, or contended root blocks readiness with no partial activation | `BifrostDataRoot::prepare` runs before role activation in `boot/mod.rs` | `prepare_rejects_an_uncreatable_root`; `prepare_refuses_a_root_owned_by_another_replica` | PASS |
| Removed Scribe/Oracle settings rejected; no alias or second root | `config.rs` | `config::tests::scribe_wal_dir_env_is_rejected`; `check:bifrost-oracle-deploy` | PASS |
| Restart from the same root recovers acknowledged Scribe state | `wyrd-testing` cluster/server harness reuse the node root | `bifrost::cluster::tests::cluster_restart_rederives_same_plan_from_retained_snapshot`; scribe journey | PASS |
| Distinct WAL identity per replica under one storage location | per-node roots in `cluster.rs`, `process_cluster.rs`, deploy manifests | root flock test above; `process_pool_lifecycle_isolated_across_restart`; `role_separated_readiness_matches_configured_roles` | PASS |
| Forge has no spill path; deployment memory/readiness preserved | `data_root.rs`, deploy manifests | `check:bifrost-resource-governance`; `check:bifrost-oracle-deploy`; oracle journey 28/28 | PASS |

Commands (all exit 0 on the committed tree):

- `mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(/^boot::data_root::tests::/)'`
- `mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=config::tests::bifrost_data_dir_defaults_and_follows_environment) | test(=config::tests::scribe_wal_dir_env_is_rejected)'`
- `scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise run test:bifrost:unit:rust:inner'` (wyrd-testing 64/64, wyrd-server 118/118, wyrd-client 39/39)
- `scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test <scribe|oracle|server|mcp> -P journey --run-ignored=all'`
- `scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cargo nextest run --locked -p wyrd-server --features test-support --test pg_grpc_ingest_smoke --run-ignored=all'`
- `mise run fmt`, `mise run lints`, `mise run check:bifrost-oracle-deploy`,
  `mise run check:bifrost-resource-governance`, `mise run check:unwrap-audit`,
  `git diff --check`

Non-goals held: no Bifrost aggregate was required; unrelated working-tree
Forge orphan-cleanup edits were not committed. Running
`forge::production_routes::coordinator_and_worker_delete_only_exact_never_published_generation`
with those uncommitted edits times out; the committed tree passes it.
