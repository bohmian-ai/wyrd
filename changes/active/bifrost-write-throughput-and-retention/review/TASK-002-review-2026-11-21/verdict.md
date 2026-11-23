# TASK-002 review: FIX_REQUIRED

Behavioral verification: **FIX_REQUIRED**. TDD compliance: **PASS as recorded**;
no demonstrated violation, with historical-order limits below. Performance
qualification remains **UNRESOLVED and waived as a review blocker** by the current
maintainer instruction. This verdict does not certify the 80 MiB/CPU-second target.

## Immutable subject and authority

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/github-verifier-pipelines-task-004`.
- Base: `5a057524f07d23a515d505caa4955539926d7fee`.
- Candidate: `c40e7245c3ada4a35177e3a8c2d37c42d058c619` (seven commits).
- Approved contract: [spec revision 3](../../spec.md).
- Original task: [TASK-002](../../tasks/TASK-002-cpu-throughput.md).
- Source review used `git diff 5a057524f...c40e7245c`. HEAD and tracked source
  remained unchanged; only new review reports/remediation were written.

Current maintainer instructions expressly approve the Arrow fork change and
exclude deferred benchmarks as blockers. They take precedence over the packet's
benchmark gates and the shared-memory general benchmark preference for this
review. Other safety and verification obligations remain applicable.

Governing references: root `AGENTS.md`,
`changes/spec-driven-development.md`, `TESTING.md`,
`agent_docs/index.md`, `agent_docs/rust.md`,
`agent_docs/maintainer-style.md`, and `architecture/bifrost-design.md`
durability/visibility, Scribe ownership/append/recovery, Oracle planning, and
resource/failure invariants. No nested AGENTS applies in the changed Vala/shared
trees. The existing CPU design and task evidence were inputs, not new authority.

## Independent assignments

| Reviewer | Boundary | Report |
|---|---|---|
| behavior-rev | Assigned requirements, checked projection, shard publication, read pruning and authorization/planning, dependencies, remaining repository rules and TDD | [Behavior](task-review-behavior.md) |
| invariant-rev | Encoder worker isolation, retained allocations/root accounting, inspection and terminal lifecycle/concurrency | [Invariants](task-review-invariants.md) |
| domain-rev | Ordered IPC ownership, WAL framing/digests/windows, IO failure/rollback, lease lifetime and accepted recovery | [Persistence](task-review-domain.md) |
| Fresh ponytail-rev | Independent source validation of proposals and complete coverage/proof reconciliation | [Validated findings](findings-validation.md) |

The calling agent orchestrated and ran checks; it did not discover or validate
its own findings. A separate system reviewer would duplicate lifecycle/recovery
coverage. No new SQL schema, public cross-tier contract, SDK surface, or scheduler
requires a broad repo or additional domain reviewer. The primary reviewer covers
maintainability; no substantial replacement owner/orchestration justified an
additional maintainer reviewer. All selected independent reports are present.

Navigation covered OTLP preflight/decode → flattened counts/projector → shard
commands; preprocessing → ordered owned IPC buffers → WAL framing/IO → accepted
replay; ingress pool → encoder slots/root leases → terminal shutdown/inspection;
capture → recorded pruning → projected statistics/identity pass → Oracle's
authorized optimized plan → registered physical planner and actual consumers.

## Reconciled matrix

| Requirement / scenario | Implementation and assertion/execution evidence | Result |
|---|---|---|
| REQ-001 / scenarios 1–2 | Shard control and pre-ACK publications retain existing order; chunk representation adds no durability cycle. New mailbox publication assertion, existing lease/ACK/retry/recovery assertions, recorded final 993 redux and 75 Scribe/OTLP tests. | PASS |
| REQ-004 / scenario 2 | WAL reader and replay still require canonical accepted authority; chunk framing and logical identity remain unchanged. Actual many-chunk file round trip plus existing ambiguous retry, restart and source-boundary journeys. | PASS |
| REQ-010 / scenario 1 | `LogColumns::with_capacity` / `RowEnvelopes::with_capacity` reuse flattened checked counts; unchanged preflight admission, merge/fallback and rejection path. Exact-capacity and control-only publication assertions; recorded regressions. | PASS |
| REQ-011 / scenario 2 | `PreparedWalAppend::chunked`, checked ordered lengths/CRC/digest, 64-piece windows, common short-write/rollback owner and IO leases. Independent writer parity with nested/null/dictionary/offset data, 200-chunk file test and scripted writes. | PASS |
| REQ-012 / scenario 3 | Worker-marked context slots use actual capacity/native accounting, root Metadata leases, bounded retention, independent stream state and race-safe closure. Reuse/parity/refusal/error assertions pass as recorded. Terminal abort/cancel bypass closure: FIND-TASK-002-1. | FAIL |
| REQ-013 / read-side scenario | `RecordedBounds` conservatively maps unchanged source columns; unknown casts/missing stats and identity second pass remain. Oracle directly uses registered physical planner after logical optimization and payload authorization. Owner assertions plus focused Oracle execution below. | PASS for functional read behavior |
| REQ-013 / throughput, write matrix, read tails, competing load | No benchmarks executed or throughput/tail claim established. Explicit current maintainer waiver applies. | UNRESOLVED, nonblocking |
| Mandatory rustdoc | `ReadableSource` description displaced onto `RecordedBounds`: FIND-TASK-002-2. | FAIL |
| Formatting and dependency gates | Fresh `fmt:check` and `check:deps`, including security-inline gate. | PASS |
| Mandatory lint gate | Workspace union passes, shipped-server graph rejects unconditional unused constructor: FIND-TASK-002-3. | FAIL |

Retained finding details and independent source evidence appear once in
[findings-validation.md](findings-validation.md). They are implemented through
one bounded [remediation task](TASK-002-R1-terminal-encoder-cleanup.md).
Retained IDs: **FIND-TASK-002-1**, **FIND-TASK-002-2**,
**FIND-TASK-002-3**. The final lint finding was independently diagnosed by the
persistence reviewer and independently validated after the actual gate failed.

No new or relocated PostgreSQL, Scribe, object-store, or Vault wait was found.
Windowed writes stay inside the existing sanctioned WAL append; fsync and the
batched acceptance transaction retain their boundaries. Query storage/source
waits stay within the existing planner/deadline boundary. No codec, compression,
WAL format, schema, resource concurrency, query routing, or durable-owner redesign
was accepted through this review.

## Verification

Fresh central execution on the candidate:

- `mise run fmt:check`: PASS (read-only equivalent of formatting).
- `mise run check:deps`: PASS, including its one
  `wyrd-spec::security_inline_gate` test (nextest run
  `09ba3e87-a46b-4cf0-b7af-99d85a4a9fea`).
- `mise run lints`: FAIL. Workspace all-features/all-targets Clippy passes;
  its separate shipped-server Clippy command fails on unused
  `PreparedWalAppend::new` at `scribe/wal.rs:511` with `-D dead-code`.
- `mise exec -- git diff --check 5a057524f...c40e7245c`: PASS.
- Focused Oracle journeys: PASS, 2/2 (nextest run
  `b3dd9e28-cfd6-4663-8303-608d4402ecf8`, 66 skipped). Exact command:

  ```bash
  mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E "test(=published::source_transitions_preserve_exact_rows) | test(=published::variant_sql_registry_covers_every_session)"'
  ```

  The migration/idempotency fixture passed first (1/1, nextest run
  `cdea5f91-31fc-4fe4-92b5-6541042340bb`). The two real-server journeys exercise
  exact rows through live/published transitions and Interactive/Analytical
  registered planning, including payload-column refusal before remote work.

Recorded implementation execution, independently checked against assertions:
993 redux library tests, 75 Scribe/OTLP journeys and crate Clippy; earlier redux
integration checkpoint; fork lifecycle/parity tests. These suites were not
rerun wholesale. Earlier scenario-2 text saying journeys were pending is
superseded by the final scenario-3 75-test record. The earlier integration run
predates final retention; final library/context tests and Scribe/OTLP journeys
provide later boundary proof. The shutdown finding is source-proven; a warmed
abort/cancellation regression is required for closure, not claimed as freshly
executed here.

TDD is reported separately: the task records concrete RED/GREEN failures and
claims PASS. Bundled commits do not independently reconstruct test-first order;
the context-reuse RED with `restore` stubbed could be test-first scaffolding or a
later mutation. No contrary evidence establishes a violation. Review does not
demand fabricated historical RED or production churn to repair history.

This is the first TASK-002 review: no prior finding closures or unsuccessful
remediation rounds apply. Review does not implement, commit, merge, or publish.
Broad integrated journey suites and benchmark experiments were skipped;
throughput benefit and CPU/read-tail qualification remain unmeasured.
