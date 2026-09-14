# Persistent Data, Audit Publication, Forge, and Oracle Coordination Review

## Immutable subject

- Repository: `/tmp/wyrd-task-002-r2-review-f345cd8a4`
- Base: `861f8d86cc3f9d7e70fb59489e80f8be62afddbf`
- Candidate: `f345cd8a4fdb5566ae6828ffb4f29d64f7f17599`
- Approved specification: `changes/active/surfaces-oracle-integration/spec.md`, revision 7
- Original task: `changes/active/surfaces-oracle-integration/tasks/TASK-002-converge-client-and-sdks.md`
- Prior reviewed candidate: `4d9d74b34803b3f27d55f740a5b40ffbe968b306`
- Remediation: `changes/active/surfaces-oracle-integration/review/task-002-r1-4d9d74b34-review-01/TASK-002-R2-close-r1-review-findings.md`

The detached candidate remained clean and fixed at `f345cd8a4` throughout this
review. The repository has no `.codegraph/` index, so inspection used Git and
direct source/caller tracing.

## Boundary and authority coverage

| Boundary | Governing authority | Source and history inspected | Result |
|---|---|---|---|
| Task and remediation scope | TASK-002 objective/constraints/non-goals; TASK-002-R2 outcome, constraints, non-goals, and mapped findings; `architecture/references/languages/spec-driven-development.md` | Complete `861f8d86..f345cd8a4` history and `4d9d74b34..f345cd8a4` delta | FAIL: two unrelated domains entered the remediation candidate |
| Durable system-tenant identity and migrations | Spec REQ-013, REQ-026B, REQ-027B, INV-007, INV-015; `AGENTS.md` tenancy/persistent-state rules; `architecture/bifrost-design.md`; SQL and migration rules | `crates/wyrd-spec/src/ids.rs`; Wyrd and Vala migrations; tenant-directory decoding; Oracle epoch constraints; all `SYSTEM_OWNER`/nil guards | FAIL: material identity and schema behavior changed outside TASK-002-R2 |
| Scribe WAL and retained audit publication | Spec REQ-010, REQ-014, REQ-026B, REQ-027 through REQ-029, INV-008 through INV-008D; `architecture/bifrost-design.md`; `architecture/references/domain/analytical-operations-reliability.md`; `architecture/operations/reliability-and-recovery.md` | `scribe/wal.rs`, replay caller in `scribe/replay.rs`, audit-log admission owner and focused decoder tests | FAIL as task scope; no additional correctness defect found in the added guard |
| Forge worker and maintenance coordination | Spec REQ-051, REQ-052, INV-021, INV-022; Bifrost and analytical-reliability authorities | `forge/worker.rs`, the sole new API caller in `wyrd-server/tests/pg_router_smoke.rs`, commit `3e8b6efb3` | FAIL as task scope; change is confined to `test-support` |
| Oracle coordination and reader authority | Spec REQ-050 and INV-018; Oracle epoch migration and system-owner callers | `vala-sql` Oracle migrations, Oracle follower and reader-owner uses, peer-audit staging test | FAIL only through the system-identity drift below; no independent Oracle coordination regression found |

The peer-audit assertion update in `c0d4dc924` merely reads the already-current
`permission` and `outcome` staging columns, and the audit-publication test's use
of `DataTenantId::SYSTEM_OWNER` follows the shared constant. Those test-only
realignments introduce no separate durable behavior and are not material
findings.

## Findings

### `PERSIST-R2-01` — DRIFT: TASK-002-R2 changes the durable system tenant and Scribe recovery contract

- **Violated obligation:** TASK-002-R2 is limited to five client error,
  OpenAPI, rustdoc/import, and SDK-documentation findings. Its explicit
  non-goals exclude persistent-state, server audit, and lifecycle changes.
  TASK-002 itself owns client/SDK convergence, not a new stable tenant identity
  or migration decision. The spec-driven workflow requires material tenancy
  and persistence choices to be owned by an approved task rather than entering
  an unrelated remediation candidate.
- **Exact location:** `crates/wyrd-spec/src/ids.rs:142-149`;
  `crates/wyrd/wyrd-sql/migrations/20260601000015_seed_system_tenant.sql:13-38`;
  `crates/vala/vala-sql/migrations/20260910000013_oracle_coordination.sql:10-13`;
  `crates/vala/vala-sql/migrations/20260910000025_oracle_reader_authority.sql:70-76`;
  `crates/vala/vala-bifrost-redux/src/scribe/wal.rs:954-1026`; associated
  system-owner checks and projections in `catalog/logical_table_identity.rs`,
  `catalog/tenant_table.rs`, `scribe/{hot_stage,ingress}.rs`,
  `tables/audit/{audit_log,projection}.rs`, and `oracle/follower.rs`.
- **Evidence:** commits `7fc756f09` and `5cfe7b6b9` landed after the prior
  reviewed candidate and before the four declared TASK-002-R2 implementation
  commits. They replace the durable system tenant from the nil UUID with
  `00000000-0000-7000-8000-000000000000`, edit the greenfield tenant seed and
  Oracle epoch/cluster schema in place, change every corresponding physical
  identity comparison, and add Scribe WAL recovery admission for system-owned
  audit-log slices. Neither commit is mapped to a TASK-002-R2 finding or named
  in its implementation evidence; the remediation record instead says no
  persistent-state change occurred.
- **Reachability:** the seed migration creates this row on every fresh database;
  Oracle reader epochs reference it; peer-audit and tail-audit writers acquire
  its tenant transaction; the audit publisher writes its identity into Scribe
  WAL records; `ReplayAccumulator::append` calls `decode_slice_payload` and
  checks the decoded tenant against the WAL header before recovering the rows.
- **Observable consequence:** the candidate changes the primary/FK identity of
  every system-owned Postgres row and the tenant bytes accepted in durable WAL
  replay. A nil-system WAL/database created by the preceding candidate is no
  longer accepted, while the new fixed UUIDv7 becomes ordinary decodable tenant
  input. The repository is pre-release, so lack of compatibility may be valid
  for the owning change, but it does not make this a client-remediation change.
- **Required testable correction:** remove the system-tenant, migration, and WAL
  recovery changes from the TASK-002-R2 candidate. If the broader integration
  needs them, route them through their own approved implementation/remediation
  task with the durable identity decision and focused SQL, WAL-restart,
  audit-publication, tenant-isolation, and Oracle-reader evidence. Do not add a
  compatibility migration or decoder to make the mixed candidate pass.

### `PERSIST-R2-02` — DRIFT: TASK-002-R2 adds a Forge worker synchronization API for an unrelated server test

- **Violated obligation:** TASK-002-R2 explicitly excludes lifecycle
  concurrency changes and cleanup outside its validated client/server-contract
  modules and two documentation snippets. TASK-002 does not authorize a new
  Forge test-support contract.
- **Exact location:**
  `crates/vala/vala-bifrost-redux/src/forge/worker.rs:809-815,1254-1273,1634-1665`;
  sole caller `crates/wyrd/wyrd-server/tests/pg_router_smoke.rs:2569-2574`.
- **Evidence:** commit `3e8b6efb3` adds `attempt_hold_table`, the public
  feature-gated `hold_after_next_table_attempt_for_test` method, its arming
  path, and a lifecycle-event scan that changes which completed attempt parks
  the supervised worker. The change exists only to stabilize
  `coordinator_object_store_failure_clears_readiness`; it is not part of any
  retained TASK-002 finding or R2 acceptance criterion.
- **Reachability:** builds with `test-support` expose the method, and the real
  server journey calls it before Forge promotion so audit-log tasks no longer
  consume the one-shot barrier. The filtered worker path executes after each
  returned attempt while the barrier remains armed.
- **Observable consequence:** the cumulative TASK-002 candidate changes the
  Forge test-support API and worker scheduling seam, and changes the causal
  behavior of a server/Forge readiness test unrelated to client convergence.
  Default production builds exclude it, but the task's required no-unrelated-
  change boundary applies to test-support code and tests as well.
- **Required testable correction:** remove this Forge/test change from the
  TASK-002-R2 candidate and review it as the narrow owner-test correction it is.
  Its own proof is the exact `coordinator_object_store_failure_clears_readiness`
  server test under `test-support`; no new worker abstraction or production
  behavior is required.

## Verification limits

- Review was source- and history-based; no implementation file was changed and
  no test command was rerun.
- The supplied R2 evidence covers the two named client/OpenAPI nextest proofs,
  formatting/lints, Python and TypeScript lanes, codegen, docs, and client/PyO3
  boundaries. It does not report the focused Scribe WAL decoder/restart tests,
  `mise run test:sql`, tenant-isolation checks, Oracle reader-authority tests,
  the Forge readiness test, or a Bifrost owner lane after these intervening
  changes.
- Existing focused source tests support the intended local behavior, but green
  tests would not make the two changes part of TASK-002-R2's approved scope.

## Overall result

**FAIL** — the reviewed client remediation may close its five intended
findings, but the immutable cumulative candidate also contains two independently
reachable, unmapped changes to durable system identity/Scribe recovery and the
Forge test-support lifecycle. The domain finding ledger is
`PERSIST-R2-01`, `PERSIST-R2-02`.
