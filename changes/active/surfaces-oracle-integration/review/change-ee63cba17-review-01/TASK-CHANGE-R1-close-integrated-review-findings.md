---
id: TASK-CHANGE-R1
kind: remediation
status: ready
spec: SPEC-surfaces-oracle-integration
spec_revision: 9
requirements: [AC-005]
depends_on: [TASK-001, TASK-002, TASK-003, TASK-004, TASK-005, TASK-006]
parent_task: integrated-change
remediates: [CHANGE-002]
---

# Close integrated change-review findings

Implementation route: `$wyrd-implement`.

## Authority and immutable subject

- Approved specification: `changes/active/surfaces-oracle-integration/spec.md`, revision 9.
- Original tasks: `changes/active/surfaces-oracle-integration/tasks/TASK-001-*.md` through `TASK-006-*.md`; TASK-001/005/006 approval uses the committed `review/human-task-approval-2026-09-16.md` record. Earlier independent verdicts remain historical.
- Integrated review: this directory's `change-review.md`.
- Cumulative base: `861f8d86cc3f9d7e70fb59489e80f8be62afddbf`.
- Reviewed target: `ee63cba1798a9e4fa6a21afbcdf1eda6c2f5b1f6`, tree `04193bd3fc315ab671f5d7fb33d3bff22081392c`.
- Last tested implementation source: `d6de890d83f269eab834fa323f3ebe31f1adc573`, tree `d8a727b5846f24bac7b55bbc786bcb391c55cb03`. Later commits add task-review and human-approval documentation only.

## Outcome

Satisfy the remaining real-server proof obligation without changing production query, audit-publisher, SDK, storage, or Forge behavior. This finding does not establish a new production-code failure. The owner explicitly waived literal Surfaces-parent ancestry; no ancestry merge is required.

### `CHANGE-002` — real-server frozen-range race proof is incomplete

AC-005 requires existing real-server retained-history evidence to cover a growing staging tail with competing publishers, crash replay without duplication, and an idle staging table drained to zero. The SQL `pg_audit_staging` test covers tail growth and competing freeze at the SQL seam. The existing real-server `frozen_audit_range_replays_once_while_its_tail_waits` journey in `crates/wyrd/wyrd-testing/tests/bifrost/server/audit_publication.rs` freezes and replays a range, but appends the tail only after replay settles (current lines 302–305). Its green result therefore does not exercise the specified cross-boundary interleaving; the comments overstate what the executable sequence proves.

Extend that existing real-server journey, using its current Postgres/server harness and publication owner, so a new tail row commits above a still-frozen range before settlement and a competing publication cycle observes the same bound while the tail exists. Abort/replay the in-flight cycle, then require the original retained events exactly once, eventual publication of the tail, and zero staged rows when idle. Preserve the test's existing crash/settlement fence and existing assertions; do not add a new harness or test file, and do not change production publisher behavior merely to make the scenario pass. The SQL seam test remains supporting proof, not a substitute for this required journey.

Proof: run the exact existing test selector with nonzero selection under repository-managed Postgres, then `mise run test:bifrost:journey:server`; record the observed ordering and counts. The exact command is:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E "test(=audit_publication::frozen_audit_range_replays_once_while_its_tail_waits)"'
```

## Constraints and final verification

- Preserve the approved human task approvals and historical verdicts. Do not rerun a task-review cycle solely for this remediation.
- Preserve tenant isolation, audit-before-decision, one frozen bound and batch identity, Scribe deduplication, atomic watermark/GC, non-blocking Oracle read audit, and the protected query deadline/terminal contract.
- Do not merge to `main`, push, release, deploy, rewrite either source branch, introduce a compatibility path, add a second audit owner, or alter public contracts.
- After the correction, run `mise run fmt`, `mise run lints`, the exact focused journey and owning server lane, `git diff --check` from the cumulative base, and final-tree required verification. The user accepts an exact same-tree map of all gate children in place of one aggregate invocation; no lane may be skipped, zero-selected, weakened, or counted after a killed run. Preserve local `mise.local.toml` S3/GCS/Azure proof for the final implementation tree.
- Record the final implementation commit/tree and any later evidence-only commit separately. A fresh `$wyrd-change-review` reassesses the complete base-to-target range; this task does not perform completion.

| Acceptance criterion | Finding |
|---|---|
| The existing real-server journey executes tail growth and competing publication before frozen-range settlement, then proves exact replay and idle drain. | `CHANGE-002` |

## Implementation evidence

- Implementation commit `ecbac3bd54be7504d2c6eacf71ac5c3c5fe2cd99`, tree
  `a08375474672fa330748bb2918b4a89e169c1008`; one file,
  `crates/wyrd/wyrd-testing/tests/bifrost/server/audit_publication.rs`.
  This record is a later evidence-only commit.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| The existing real-server journey executes tail growth and competing publication before frozen-range settlement, then proves exact replay and idle drain. | `frozen_audit_range_replays_once_while_its_tail_waits`: staged rows fenced; freeze and tail append commit in one transaction (tail seq asserted above the bound); a competing `freeze_publication_range` queued behind that transaction (observed via `pg_blocking_pids`) reuses the identical bound while the tail is staged; a spawned `publish_tenant` cycle queues next; frozen decisions retained 3 and tail retained 0 while fenced; cycle aborted, fence released, replay; frozen retained exactly 3, tail retained 1, staging drained to zero. New helpers `backend_pid`, `await_blocked_behind`. | Exact focused command from this task: 1 run, 11 skipped, PASS; repeated 5 more times, 5/5 PASS (~12.3 s each). `test:bifrost:journey:server` 12/12 PASS. | PASS |

Observed ordering: fence → freeze+tail commit (competitor already queued) →
competitor returns `Some(range)` → publication durable (frozen 3, tail 0) →
abort → fence release → replay → frozen 3, tail 1, staged 0.

Final-tree verification on tree `a08375474` (same-tree map of every `gate`
child; each `mise run <task>` exit 0):

- `fmt`, `lints`, `git diff --check 861f8d86cc3f9d7e70fb59489e80f8be62afddbf`
  and working tree: exit 0.
- `test:bifrost`: `unit:rust`, `unit:python`, `unit:typescript`,
  `integration:redux`, `integration:sql`, `integration:server`, and journeys
  `sdk`, `forge`, `scribe`, `oracle`, `otlp`, `mcp`, `python`, `typescript`
  PASS inside the `gate` run. Journey `server` failed in that run on the
  unchanged `a_stalled_tenant_does_not_block_another_tenants_history`
  ("idle tenant kept 1 staged row(s) through 90s") under full gate load; the
  test uses its own database and is untouched by this change. That failed run
  is not counted: `test:bifrost:journey:server` then passed 12/12 in three
  separate same-tree reruns.
- `test:rust` families: `test:wyrd` 1907, `test:skald` 410, `test:vala` 1272,
  `test:shared` 649, `test:sql` 102+4+113+2, `test:tonic` 44 PASS;
  `test:storage:matrix` all invocations nonzero, PASS.
- `check`, `check:skills-sync`, `codegen:check`, `cardkind:check`,
  `check:client-tier`, `check:registry-client-tier`, `check:cli-client-tier`,
  `check:sdk-client-tier`, `check:sdk-pyo3-scope`,
  `check:registry-no-server-routes`, `check:pyo3-scope`, `check:mocks-scope`,
  `check:unwrap-audit`, `check:audit-script`, `check:clippy-allow-audit`,
  `check:clippy-allow-audit:self`, `check:tenant-isolation:self`,
  `check:test-coverage:self`, `check:ci-selection`, `check:storage:drift`,
  `check:registry-tx-coupling`, `check:registry-immutable-spec-hash`,
  `check:registry-single-table`, `check:object-store-pin`,
  `check:error-coverage`, `check:tenant-isolation`, `check:test-contracts`,
  `check:tokens`, `py:setup`, `py:format:check`, `py:lints`, `py:typecheck`,
  `py:test:unit`, `ts:napi:check`, `ts:typecheck`, `ts:test:unit`,
  `examples:python:datacard`, `check:examples`, `check:docs`,
  `check:no-legacy-server-vocab`, `check:no-tonic-outside-wyrd-tonic`,
  `check:from-pools-allowlist`, `check:fixtures-no-server`,
  `check:no-testing-in-prod-deps`, `check:bifrost-oracle-deploy`,
  `check:bifrost-resource-governance`, `check:test-coverage`: PASS.
- Local real cloud (`mise.local.toml`): `storage:s3:dev`, `storage:gcs:dev`,
  `storage:azure:dev` each handle CRUD 1 + multipart e2e 1 PASS.

Environment note: host disk exhaustion, low-memory termination, and external
deletion of `target/` artifacts ended earlier `test:rust` and `check:examples`
attempts before any test result; none is counted, and each lane above is
from a later complete run on the same tree.

Non-goals held: no production query, audit-publisher, SDK, storage, or Forge
change; no new harness, test file, dependency, alias, or `#[allow]`; no
skipped or weakened test; no push, merge, release, or deploy;
`mise.local.toml` untracked; unrelated `changes/active/verified-change-contract/*`
edits and untracked review directories untouched.

Status: `IMPLEMENTED`.
