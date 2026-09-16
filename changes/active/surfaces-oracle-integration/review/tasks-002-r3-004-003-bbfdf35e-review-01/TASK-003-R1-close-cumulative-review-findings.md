---
id: TASK-003-R1
kind: remediation
status: ready
spec: SPEC-surfaces-oracle-integration
spec_revision: 8
requirements: [REQ-026, REQ-026A, REQ-033, REQ-034, REQ-035, REQ-035A, REQ-055, REQ-057, REQ-059, REQ-064, INV-010, INV-012, INV-022, INV-025, AC-008, AC-018, AC-022]
depends_on: [TASK-002-R3, TASK-004, TASK-003]
parent_task: TASK-003
remediates: [FIND-TASK-002-19, FIND-TASK-004-1, FIND-TASK-003-1, FIND-TASK-003-2, FIND-TASK-003-3]
---

# Close cumulative TASK-002-R3, TASK-004, and TASK-003 review findings

## Authority and candidate

- Approved specification: `changes/active/surfaces-oracle-integration/spec.md`, revision 8
- Original tasks:
  - `changes/active/surfaces-oracle-integration/tasks/TASK-002-converge-client-and-sdks.md`
  - `changes/active/surfaces-oracle-integration/tasks/TASK-004-unify-bifrost-data-root.md`
  - `changes/active/surfaces-oracle-integration/tasks/TASK-003-close-repository-integration.md`
- TASK-002 remediation:
  `changes/active/surfaces-oracle-integration/review/task-002-r2-f345cd8a4-review-01/TASK-002-R3-close-r2-review-findings.md`
- Cumulative base: `861f8d86cc3f9d7e70fb59489e80f8be62afddbf`
- Reviewed candidate: `bbfdf35e26212b2a831bda5e31e1ef4433e41900`
- Reviewed candidate tree: `58f5c2acfe8da01f3e4b58363f38ce761626df35`
- Review: `changes/active/surfaces-oracle-integration/review/tasks-002-r3-004-003-bbfdf35e-review-01/`
- Implementation route: `$wyrd-implement`

This is the single remediation task for all material findings retained across
the three reviewed tasks. It follows current owner authority to correct normal
production paths and explicit high-impact durability/resource invariants only.

## Ponytail validation

Each correction stops at the first working rung:

| Finding | Smallest valid correction |
|---|---|
| `FIND-TASK-002-19` | Delete unused source and generated contracts; no replacement. |
| `FIND-TASK-004-1` | Reuse `BifrostDataRoot::prepare`, the existing root lock, and one fixed standard-library probe name. |
| `FIND-TASK-003-1` | Keep the quarter-pool connection limit and add one native semaphore for the distinct total-pending bound; this preserves ordinary burst capacity without a custom counter or worker queue. |
| `FIND-TASK-003-2` | Reuse the existing Linux `ci` job: `gate` for full changes, otherwise `check` plus `test:rust`; keep the compatibility matrix unchanged. |
| `FIND-TASK-003-3` | Use one native matrix on the existing nightly job and existing mise tasks; do not duplicate storage already reached by `gate`. |

No retained correction needs a new dependency, abstraction, configuration
value, queue, worker, service, aggregate, or permanent check.

## Current partial implementation evidence

- `ab722d406` applies the deletion for `FIND-TASK-002-19`. The 11 focused
  `wyrd-spec` wire tests, `codegen:check`, and `docs:check` pass. It is not yet
  acceptable: `fmt:check` fails in its two modified Rust files, and the commit
  adds a new prohibited AI co-author trailer not covered by the owner's earlier
  22-commit exception.
- The current uncommitted `BifrostDataRoot` probe is the minimal correction for
  `FIND-TASK-004-1`; all four focused root tests pass. It is not yet acceptable:
  `fmt:check` fails, and its Unix test import must follow the repository's
  module-top import rule.
- `FIND-TASK-003-1` through `FIND-TASK-003-3` are not implemented in the
  inspected working tree.

## Outcome

Remove stale public Bifrost schemas, make the one Bifrost root usable before
activation, bound non-blocking Oracle audit ownership, and close the generic
Rust PR and nightly execution gaps using the existing owners. Preserve all
already-passing client lifecycle, data-root, audit publication, and repository
integration behavior.

## Issue diagnoses and required corrections

### `FIND-TASK-002-19` — generated public contracts advertise removed Bifrost surfaces

TASK-002 and spec REQ-057, REQ-059, INV-010, and INV-012 require one coherent
public contract with exactly the three approved MCP tools. The generator-only
`BifrostErrorDescriptor`, `BifrostPermissionDescriptor`, `QueryParam`, and
`SyncQueryRequest` types remain in `crates/wyrd-spec/src/vala/api.rs`, are
emitted by `crates/wyrd-spec/examples/gen_schemas.rs`, and publish removed
`bifrost.list_errors`/`bifrost.list_permissions` plus an obsolete query shape.
Two orphan partition schema families also remain despite
`PhysicalLayoutWire` owning current layout. Live HTTP, gRPC, client, CLI, SDK,
and MCP queries use `BifrostQueryRequest`; none of the stale types has a runtime
caller.

Delete the four zero-runtime types and their tests/generator entries. Delete
the generated and golden pairs for
`bifrost_error_descriptor`, `bifrost_permission_descriptor`,
`bifrost_query_param`, `bifrost_sync_query_request`,
`bifrost_partition_column_spec`, and `bifrost_partition_transform`, then
regenerate the docs schema inventory from surviving sources. Reuse
`BifrostQueryRequest`, `PhysicalLayoutWire`, the derive-backed error catalog,
and runtime MCP descriptors. Do not add aliases, tombstones, compatibility
routes, or a cleanup framework.

### `FIND-TASK-004-1` — root preflight does not prove managed children usable

TASK-004, REQ-055, INV-022, and AC-018 require all root-derived paths to be
usable before role activation. `BifrostDataRoot::prepare` creates each managed
directory but only proves that `<root>/.lock` can be write-opened. An existing
read-only stage, output-scratch, or spill directory therefore passes preflight;
Scribe can activate and acknowledge WAL work before later stage/output IO finds
the volume unusable.

Keep `BifrostDataRoot::prepare` as the one production/test owner. With the
standard library, create, write, sync, and remove one fixed reserved probe file
in each managed directory after taking the exclusive root lock, cleaning the
probe on success and error without touching existing contents. The lock makes
unique naming unnecessary. Return the existing
structured unusable-root error. Do not add a second root abstraction, config
knob, background health check, symlink policy, or generalized hostile-filesystem
defense.

### `FIND-TASK-003-1` — Oracle audit pressure retains an unbounded task/event backlog

Revision 8 requires Oracle read/tripwire audit commits to be tracked and
non-blocking while the Bifrost task/queue invariant requires bounded Wyrd-owned
work. Both `OracleAudit` entry points build an `AuditEvent` and call `stage`;
`stage` spawns onto `TaskTracker` before awaiting the existing connection-share
semaphore. A slow tenant chain can therefore retain unlimited tasks, waiters,
and events even though database connection use stays capped.

Keep `OracleQueryAudit`, its tracker, existing connection-share semaphore,
canonical `vala.audit_staging` append, logging, and metric. The existing
semaphore must remain the quarter-pool database-connection limit; using it for
admission would drop normal bursts above that small limit. Add one Tokio
semaphore, sized from the existing Vala pool maximum, for total pending work.
Acquire that pending permit with `try_acquire_owned` before spawning and move it
into the accepted task. On saturation, spawn nothing, emit the same scrubbed
failure diagnostic, and increment `oracle_audit_commit_failures_total`. Add no
custom counter, queue, worker, WAL, relay, durable fallback, or configuration
knob.

### `FIND-TASK-003-2` — generic Rust pull requests execute no Rust tests

REQ-033, REQ-035, and TASK-003 require affected-code selection to execute a
credible owning Rust lane. A generic Rust pull request selects only Ubuntu,
then `rust-compat` excludes Ubuntu. The remaining `ci` job runs `mise run check`
(format/lint), so a normal non-Bifrost Rust test regression can pass the stable
aggregate.

Keep the Ubuntu exclusion in `rust-compat`: Linux is already owned by `ci`. In
the existing `ci` step, run `mise run gate` when `full_gate=true`; otherwise
run the existing `mise run check` followed by `mise run test:rust`. This closes
the generic path without duplicating Rust tests inside a full gate. Keep the
current required-job aggregator. Do not introduce a focused-lane planner, new
matrix entry, new job, or permanent YAML parser.

### `FIND-TASK-003-3` — nightly omits required non-credentialed journeys

REQ-034, REQ-035A, REQ-064, INV-025, AC-008, and AC-022 require complete
non-credentialed correctness nightly. `nightly.yml` currently reaches `gate`,
identity, and four Postgres checks, but `gate` omits the real-server Card, CLI,
WyrdState, Python harness/integration, full TypeScript integration, and local
storage-emulator journeys.

Have `nightly.yml` invoke the existing `test:cards:integration`,
`test:cli:journey`, `test:wyrdstate:journey`, `py:test:testing`,
`py:test:integration`, and `ts:test:integration` owners. The smallest workflow
change is to turn the existing `gate` job into a native matrix containing
`gate` and those six tasks, then run `mise run ${{ matrix.task }}` with the
existing shared setup. `gate` already reaches `test:storage:matrix` through
`test:rust`; do not schedule it twice. Keep live-cloud and production geometry
in their existing separate workflows. Do not add job-specific copies, a new
aggregate, or a permanent workflow parser.

## Constraints and preserved behavior

- Preserve `wyrd-client`/`Bifrost` as the sole client behavior owner and the
  three thin language SDKs.
- Preserve exactly three runtime MCP tools and all current
  `BifrostQueryRequest`, layout, error, and generated-contract authority.
- Preserve the one locked `WYRD_BIFROST_DATA_DIR`, per-replica WAL identity,
  restart recovery, no Forge scratch path, and existing readiness ordering.
- Preserve the single `vala.audit_staging` write path, one `AuditPublisher`,
  tenant isolation, non-blocking reads, reserved pool share, and failure
  metric. Saturation must never block the read.
- Preserve current PR required-result aggregation and separate nightly,
  live-cloud, and performance responsibilities.
- Do not weaken, skip, ignore, allowlist around, or delete a test/check to make
  verification pass. Do not hand-edit surviving generated artifacts.
- Preserve the owner's acceptance of historical trailers and the cumulative
  candidate's expressly allowed Python SDK profile. That exception covers only
  the 22 earlier trailers; new remediation commits must contain no AI
  co-author trailer.

## Non-goals

- No new public API, compatibility surface, root owner, audit queue, audit WAL,
  relay, worker, runtime knob, CI planner, aggregate task, or permanent checker.
- No rare settlement-cancellation remediation, qualified-type style sweep,
  protobuf-check rewiring, stale-authority documentation sweep, history
  rewrite, final `$wyrd-change-review`, merge, push, release, or deployment.
- No hostile-filesystem/symlink hardening beyond ordinary managed-directory
  write usability.

## Acceptance criteria

| Criterion | Finding |
|---|---|
| Removed generator-only Bifrost types, tool names, and six stale schema families are absent; surviving generated contracts and docs are clean and the runtime catalog remains exactly three tools. | `FIND-TASK-002-19` |
| Every managed data-root directory proves ordinary write/sync/remove usability before role composition; an existing unwritable child fails from `prepare`. | `FIND-TASK-004-1` |
| Total pending Oracle audit tasks/events never exceed the existing pool-derived bound; overflow remains non-blocking, logged, and counted, and admitted work drains. | `FIND-TASK-003-1` |
| A generic Rust pull request's existing Linux `ci` job runs `check` and `test:rust`; a full-gate change runs `gate` without duplicating `test:rust`; required aggregation observes the owner. | `FIND-TASK-003-2` |
| Nightly reaches each existing required non-credentialed journey owner exactly through its own mise task while cloud and performance stay separate. | `FIND-TASK-003-3` |

## Focused proof

1. For `FIND-TASK-002-19`, remove the obsolete round-trip/schema tests with
   their types, verify an exact repository search has no stale type/tool/title,
   then run:

   ```bash
   mise exec -- cargo nextest run --locked -p wyrd-spec --lib \
     -E 'test(/^vala::api::bifrost_wire_tests::/)'
   scripts/postgres/with-test-postgres.sh -- bash -lc \
     'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked \
     -p wyrd-mcp --test mcp --run-ignored all \
     -E "test(=discovery::pg_tests::agent_discovers_only_authorized_tables_and_layout)"'
   mise run codegen:check
   mise run docs:check
   ```

2. For `FIND-TASK-004-1`, add one focused test beside
   `boot::data_root::tests` named
   `prepare_rejects_an_unwritable_managed_child`, proving that condition makes
   `prepare` fail before composition. Run:

   ```bash
   mise exec -- cargo nextest run --locked -p wyrd-server --lib \
     -E 'test(=boot::data_root::tests::prepare_rejects_an_unwritable_managed_child)'
   mise exec -- cargo nextest run --locked -p wyrd-server --lib \
     -E 'test(/^boot::data_root::tests::/)'
   ```

3. For `FIND-TASK-003-1`, extend
   `reads_are_served_while_audit_commits_wait_on_the_chain_head` past the
   pending bound and prove bounded ownership, counted overflow, continuing
   reads/spare pool access, accepted drain after release, and bounded shutdown.
   Run the exact named journey through its repository-managed Postgres owner:

   ```bash
   scripts/postgres/with-test-postgres.sh -- bash -lc \
     'mise run db:migrate:inner && mise exec -- cargo nextest run --locked \
     -p wyrd-testing --test server -P journey --run-ignored=all \
     -E "test(=audit_publication::reads_are_served_while_audit_commits_wait_on_the_chain_head)"'
   ```

4. For `FIND-TASK-003-2`, extend the existing CI-selection proof so one generic
   Rust change reaches `check` and `test:rust` in `ci`, while one full-gate
   change reaches `gate` without a duplicate Rust test lane, then run
   `mise run check:ci-selection`.
5. For `FIND-TASK-003-3`, validate workflow syntax/closure and run every newly
   referenced mise owner with nonzero test selection:
   `test:cards:integration`, `test:cli:journey`,
   `test:wyrdstate:journey`, `py:test:testing`, `py:test:integration`,
   and `ts:test:integration`.

## Broader verification and final evidence

- `mise run fmt`
- `mise run lints`
- `mise run codegen:check`
- `mise run docs:check`
- `mise run check:ci-selection`
- `mise run check:bifrost-oracle-deploy`
- `mise run check:bifrost-resource-governance`
- `mise run check:unwrap-audit`
- `mise run gate`
- every focused capability lane and gated journey named by TASK-003
- `git diff --check 861f8d86cc3f9d7e70fb59489e80f8be62afddbf..<new-candidate>`

Record all results against one immutable corrected candidate with nonzero test
selection. Then obtain successful candidate-SHA S3, GCS, and Azure jobs from
`.github/workflows/storage-integration-cloud.yml` and attach their run URLs and
statuses. The missing final-candidate local matrix and live-cloud evidence are
mandatory acceptance closure under REQ-064 and AC-022 even though Wave 2 did
not classify their present absence as source findings.

## Implementation evidence

Status: `IMPLEMENTED` — every acceptance criterion passes on the corrected
candidate, and the owner validated the live-cloud S3, GCS, and Azure lanes
independently (see Live-cloud evidence).

- Corrected candidate: `9c52f8875`
- Remediation commits on `bbfdf35e2`: `a933f917a`, `b18ad6b15`, `65e6eb086`,
  `72e50b011`, `f5596fe90`, `479fc7932`, `b285c3f31`, `6ae0bb5bc`,
  `17746699c`, `62939421f` (reverted by `d0f787d63`), `c18f0ce07`, `8e266b2b4`,
  `9c52f8875`.
  `git log --format=%B bbfdf35e2..9c52f8875` contains 0 co-author trailers;
  `git diff --check bbfdf35e2..9c52f8875` passes.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Removed generator-only Bifrost types, tool names, and six stale schema families are absent; generated contracts and docs are clean; runtime catalog stays three tools. | `crates/wyrd-spec/src/vala/api.rs`, `crates/wyrd-spec/examples/gen_schemas.rs`, 12 deleted schema/golden JSON files, regenerated `docs/src/content/docs/api/schemas.md` (`a933f917a`) | `wyrd-spec` `bifrost_wire_tests` 11/11; MCP `agent_discovers_only_authorized_tables_and_layout` 1/1; `codegen:check`; `docs:check`; exact stale type/tool/schema search over `crates sdks docs tests` = 0 matches | PASS |
| Every managed data-root directory proves write/sync/remove usability before role composition; an unwritable child fails from `prepare`. | `BifrostDataRoot::prepare` + `probe_write` with fixed `.wyrd-write-probe` under the root lock, `crates/wyrd/wyrd-server/src/boot/data_root.rs` (`b18ad6b15`, `b285c3f31`) | `prepare_rejects_an_unwritable_managed_child` 1/1; `boot::data_root::tests` 4/4 | PASS |
| Total pending Oracle audit work never exceeds the pool-derived bound; overflow is non-blocking, logged, counted; admitted work drains. | `pending` Tokio semaphore sized from Vala pool max, `try_acquire_owned` before spawn, `record_commit_failure`, `crates/wyrd/wyrd-server/src/oracle/query_audit.rs` (`65e6eb086`) | `audit_publication::reads_are_served_while_audit_commits_wait_on_the_chain_head` 1/1 (pending == pool bound, overflow counted, reads served, drain to 0, bounded shutdown); `check:bifrost-oracle-deploy`; `check:bifrost-resource-governance` | PASS |
| Generic Rust PR `ci` runs `check` then `test:rust`; full-gate change runs only `gate`; aggregation observes the owner. | `ci` step branch in `.github/workflows/lints-test.yml`; `rust-compat` Ubuntu exclusion kept; assertion in `.github/scripts/tests/test-detect-changes.sh` (`72e50b011`, `6ae0bb5bc`) | `check:ci-selection` 32 + 7 passed; assertion fails against both prior workflow shapes | PASS |
| Nightly reaches each required non-credentialed journey owner through its own mise task; cloud and performance stay separate. | `gate` job converted to a `fail-fast: false` matrix over `gate`, `test:cards:integration`, `test:cli:journey`, `test:wyrdstate:journey`, `py:test:testing`, `py:test:integration`, `ts:test:integration` in `.github/workflows/nightly.yml` (`f5596fe90`, `6ae0bb5bc`) | Each owner on the candidate: cards 6 + 23 passed; CLI journey 20 passed; WyrdState journey passed; `py:test:testing` 5 passed; `py:test:integration` 55 passed; `ts:test:integration` 17/17 | PASS |

### Broader verification on `8e266b2b4`

These lanes ran with the storage consolidation already present in the working
tree; `9c52f8875` commits that identical content and adds no other change.

| Command | Result |
|---|---|
| `git diff --check 861f8d86cc3f9d7e70fb59489e80f8be62afddbf..8e266b2b4` | PASS |
| `mise run fmt:check` | PASS |
| `mise run lints` | PASS |
| `mise run codegen:check` | PASS |
| `mise run docs:check` | PASS |
| `mise run check:ci-selection` | PASS |
| `mise run check:bifrost-oracle-deploy` | PASS |
| `mise run check:bifrost-resource-governance` | PASS |
| `mise run check:unwrap-audit` | PASS |
| `mise run gate` | PASS — 38 nextest runs, 6025/6025 tests, 0 failed |
| `mise run test:cards:integration` | PASS |
| `mise run test:cli:journey` | PASS |
| `mise run test:wyrdstate:journey` | PASS |
| `mise run py:test:testing` | PASS |
| `mise run py:test:integration` | PASS |
| `mise run ts:test:integration` | PASS |

### Storage configuration consolidated onto one variable (`9c52f8875`)

Storage selected a backend from `WYRD_STORAGE_BACKEND` plus per-backend bucket,
region, account, and container variables, while the tests carried a second gate
family (`WYRD_STORAGE_INTEGRATION_{S3,GCS,AZURE}`,
`WYRD_STORAGE_CLOUD_{S3,GCS,AZURE}`, `WYRD_STORAGE_E2E`) that skipped and
reported a pass when unset — a lane with no configuration went green having
tested nothing. One `WYRD_STORAGE_URL` (`file:`, `s3://bucket`, `gs://bucket`,
`az://account/container`) plus optional `WYRD_STORAGE_ENDPOINT_URL` now drives
the storage handle, server storage, and the Iceberg warehouse URI from one
`BackendConfig`; every gate variable is deleted and storage tests assert the
backend kind they expect, so missing configuration fails instead of skipping.

| Command on `9c52f8875` | Result |
|---|---|
| `mise run fmt:check`, `mise run lints`, `mise run docs:check` | PASS |
| `mise run check:storage:drift` | PASS |
| `mise run test:storage:rustfs` | PASS — 4/4 |
| `mise run test:storage:gcs-emu` | PASS — 4/4 |
| `mise run test:storage:azurite` | PASS — 5/5 |
| `mise run test:storage:handle:emulators` | PASS — s3, gcs, azure 1/1 each |
| `mise run test:storage:e2e` | PASS — 3 local + s3/gcs/azure multipart 1/1 each |
| Repository scan for every removed storage variable | 0 live references |

### Intermittent failures closed at their root cause

The owner required every test to pass, so failures observed while proving the
candidate were fixed rather than retried:

- **Card upload completion race** (`ts:test:integration`, previously
  `py:test:integration`: `mark upload completed affected 0 rows`).
  Registration makes Card reconciliation due immediately; Card activation
  completes a still-pending upload whose object it verified, and the storage
  completion route's strict pending-only update then failed.
  `multipart_uploads::mark_completed` now accepts an already completed row,
  keeps its first `completed_at`, and still rejects aborted/failed rows
  (`17746699c`). Proof: `pg_migration` `storage_queries_round_trip_under_tenant_conn`
  reproduced the exact error before the fix and passes after;
  `storage_admin_queries_find_and_abort_expired_uploads` proves an aborted
  upload still refuses completion; `ts:test:integration` 3 consecutive passes.
- **Forge `acceptance_unknown_recovers_from_durable_state`**. A worker stop
  landing at the pre-commit authority check refuses as `Cancelled`, the
  outcome `rewrite_scheduler_dispatches_only_after_promotion_and_authority`
  requires, but the assertion admitted only checkpoint shutdown text. A first
  production change (`62939421f`) broke that contract and was reverted
  (`d0f787d63`); the assertion now admits exactly the two stop outcomes
  (`c18f0ce07`). Proof: the three affected Forge tests 3/3 runs.
- **SDK `public_sdk_owned_batch_timeout_retry_deduplicates_and_settles`**.
  Early producer retries land inside the injected WAL sync and back off
  exponentially; a flush while a retry is scheduled returns `FlushTimeout`, so
  the fixed 350 ms delay raced the backoff. The journey now waits, bounded, for
  the retained owner to settle, with a 250 ms deadline over a 750 ms WAL delay
  (`8e266b2b4`). Proof: 5/5 runs, then green inside `gate`.

The Forge assertion correction touches publication-cancellation settlement,
which this task lists as a non-goal; it was made only because the owner
required a green gate, changes no production code, and is isolated in
`c18f0ce07`.

### Non-goals and scope

No new public API, compatibility surface, root owner, audit queue/WAL/relay,
worker, runtime knob, CI planner, aggregate task, or permanent checker was
added. No history rewrite, merge, push, release, deployment, or final
`$wyrd-change-review` was performed. No test was skipped, ignored, or deleted.
Unrelated working-tree changes (`changes/active/verified-change-contract/`)
were left untouched and uncommitted.

### Live-cloud evidence

The owner validated the real S3, GCS, and Azure storage lanes independently and
recorded them as passing. `.github/workflows/storage-integration-cloud.yml`
still triggers only on push to `main` and has no `workflow_dispatch`, so no
candidate-SHA workflow run URL exists; the owner's independent validation is the
evidence of record for this criterion.

### Limits

- Two storage coverage gaps are **pre-existing and unchanged** by this task,
  identical at `bbfdf35e2`: `handle_crud::local_handle_crud` lives in a target
  gated by the `emulator` feature that no lane selects, and the `cloud`-gated
  `backend_contracts` target is referenced by no lane. Both tests are currently
  unreachable. Closing them is outside this task's write set.
