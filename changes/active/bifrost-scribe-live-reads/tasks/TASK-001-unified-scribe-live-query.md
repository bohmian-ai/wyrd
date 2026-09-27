---
id: TASK-001
title: Implement the one published-plus-live Bifrost query flow
kind: implementation
status: proposed
spec: SPEC-bifrost-scribe-live-reads
spec_revision: 3
requirements: [REQ-001, REQ-002, REQ-003, REQ-004, REQ-005, REQ-006, REQ-007]
invariants: [INV-001, INV-002, INV-003, INV-004, INV-005]
acceptance: [AC-001, AC-002, AC-003, AC-004, AC-005, AC-006, AC-007, AC-008]
depends_on: []
---

## Outcome and Value

Implement one Bifrost query request and one query service for ordinary and
verification callers. Every query combines its pinned published cut with live
scans on relevant online Scribes. Oracle alone selects Interactive or
Analytical from the one DataFusion physical plan. Callers receive an honest
terminal: success for all selected sources, degraded for known pre-row live
loss, and failure for any required or late-stream fault. This single task owns
all approved spec obligations, including first-class client closure and
retirement of the old 30-second tail-fence path.

Required execution skill: `$wyrd-implement`. Work through the ordered RED,
GREEN, REFACTOR scenarios below; do not implement the entire rewrite before
the first failing proof. Every named focused test command below uses an
existing package and target; new test names are proposed within those targets.
The implementer owns the smallest fixture and test shape that proves each
behavior.

## Owners, Scope, Consumers, and Prohibited Changes

- `wyrd-spec` owns the IO-free request and terminal contract. The server query
  service, Gate, Oracle, and Scribe own durable query behavior. `wyrd-client`
  owns shared SDK transport and decoding; Rust, Python, TypeScript, CLI, HTTP,
  gRPC, and MCP project the same contract. Drift's authenticated
  `ScheduledQueryCaller` enters the ordinary query service and may execute on
  a local or peer-forwarded Oracle. It receives no special source mode.
- Oracle pins published sources, discovers ready Scribe streams for referenced
  tables, selects only relevant reported partition owners, and plans once.
  Scribe scans its own active/immutable/staged authority and streams bounded
  Arrow output over the existing authenticated peer protocol. Published-file
  scans retain Oracle-worker distribution and normal DataFusion residuals.
- Preserve all approved invariants: tenant and permission checks before source
  IO; signed, replay-protected peer authority; admission, memory, backpressure,
  cancellation and deadline ownership; WAL ACK and publication order; pinned
  published files; client acceptance only after a valid terminal.
- Do not add a public visibility/freshness/query-class option, verifier-only
  route, Flight listener, durable current-owner index, replacement tail lease,
  second physical planner, new scheduler, Scribe analytical-stage worker role,
  general aggregate or join pushdown, persistent state, or compatibility mode.
  Do not change write ACK timing or claim exact coverage of all acknowledged
  writes. A publication-overlap omission or duplicate is an approved result
  limit, including for Verifiers.

## Approach

1. Express the one-request, one-terminal contract in a RED contract test;
   then update source contracts and all public projections together.
2. Prove live owner selection with a multi-Scribe journey; move discovery ahead
   of the single physical plan and include selected live work in admission.
3. Prove published-plus-live execution with distributed published workers;
   execute bounded Scribe-local DataFusion scans through the existing peer
   authority and stream them into Oracle's plan.
4. Prove stream ownership and terminal failure behavior before removing the
   leader drain and acquire/page/release tail fence. Keep discovery and its
   authentication.
5. Prove Drift and first-class clients use the same query service and terminal
   contract. Regenerate derived surfaces, align the approved architecture and
   user docs, and run the required gates sequentially.

### Streaming handoff contract

These are the existing owners' required inputs and outputs, not new public
types or a prescribed private API:

| Boundary | Input | Output and ownership |
| --- | --- | --- |
| Discovery to planning | Authorized tenant and resolved tables, pinned published cut, ready-Scribe `ListActiveStreams` responses | Selected `(node incarnation, writer epoch, table, partition)` routes plus known listing losses. Discovery retains no Arrow rows. |
| One DataFusion plan | Pinned published files and selected live routes, with truthful pruning/statistics | One physical root containing existing Oracle-worker file scans and a leader-owned live source. Planning opens no Scribe snapshot and does not drain rows. |
| Oracle to Scribe | Existing signed fragment assignment and `ScribeProviderCut`, admitted query deadline/cancellation and Scribe resource bounds | The authenticated Scribe handler opens its local source and streams schema, bounded batches, and a footer at natural exhaustion. Its open snapshot and admission belong to that fragment stream until completion or drop. |
| Scribe to Oracle live source | Authenticated peer stream for a selected route | Incremental validated `RecordBatch` delivery to DataFusion, with per-fragment progress of zero rows or at least one yielded row. Neither the current `Vec<HotBatch>` resolver nor the dispatcher `AttemptBuffer` may materialize this path. |
| Oracle to query terminal | Published-source completion, known listing losses, selected fragment progress and footer outcomes | Success after the DataFusion plan completes and every still-needed selected stream reaches a valid footer; a child the completed plan intentionally stops is cancelled and released without a footer. Degraded applies only to availability loss before that fragment yields a row; required, late-row, integrity, and unexpected-footer faults fail. The query owner drops/cancels every descendant before settlement. |

The required order is: authenticate and authorize; resolve tables and pin the
published cut; discover and select relevant live routes; build and retain one
physical plan without reading rows; derive Interactive or Analytical from its
root and admit resources; open selected Scribe streams; pass each validated
batch to DataFusion as it arrives; require a footer for streams consumed to
natural exhaustion; after DataFusion completes, cancel and release any opened
child it no longer needs; then emit one terminal and release all owners.
Availability before the first yielded row can mark that live fragment
unavailable. Once a batch containing a row reaches Oracle's DataFusion source,
later loss cannot be downgraded because an aggregate may already have consumed
it. Unexpected EOF or invalid footer while the plan still needs that stream
is a protocol failure. A child intentionally stopped by the completed plan is
ordinary query-owned cleanup and has no footer obligation. Query
cancellation, disconnect, deadline, and ordinary stream drop release the
Scribe snapshot and admission together.

## Ordered Implementation Scenarios

### Scenario 1 — One request and truthful terminal

**Behavior.** A query request contains SQL and optional deadline, with no
visibility, freshness, or class selector. The terminal has no redundant
freshness field. Success means the published cut and the live work required by
the completed plan finished; Degraded includes the known-live-loss warning and
selected source completion; Failed cannot be accepted as a partial result. The deadline
range and stable errors remain intact. Covers REQ-001, INV-003, INV-004,
AC-001.

**RED.** Change the existing contract tests to assert the one request shape and
terminal matrix before changing production types. The current tests require
both policy fields and `Complete`, so the new assertion must fail. Run each
focused test through mise:

```bash
mise exec -- cargo nextest run --locked -p wyrd-spec --lib -E 'test(=vala::api::query_terminal_tests::query_request_requires_policies_and_validation_is_closed)'
mise exec -- cargo nextest run --locked -p wyrd-spec --lib -E 'test(=vala::api::query_terminal_tests::closed_terminal_matrix_validates)'
```

**GREEN.** Make the pure request and terminal types, validation, server
adapters, shared client decoder, and generated-source inputs express one query
behavior. Keep the existing structured rejection and EOS requirements. Rerun
both RED tests. Do not make the server choose Interactive or Analytical from a
client field.

**REFACTOR.** Remove now-redundant mode-specific branches and tests that assert
the removed request matrix while keeping the successful/failed terminal
contract and deadline tests green. Prefer the existing contract owner and
single error path over replacement compatibility types.

### Scenario 2 — Only relevant Scribes execute live work

**Behavior.** For referenced tables, Oracle may list active streams from all
ready Scribes but sends live fragments only to Scribes reporting partitions
that can contribute to the query. With two relevant owners and an irrelevant
third, exactly the two owners execute fragments. An unsafe-to-prune predicate
retains all reported routes for its table. A live-only query receives bounded
planning and admission. A Scribe absent before discovery is not invented from
the historical ACK ledger. Covers REQ-002, INV-002, INV-003, AC-002, AC-005.

**RED.** Add the real-server Oracle journey
`distributed::live_query_routes_only_relevant_scribes` in the existing `oracle`
target. Assert actual fragment destinations and a live-only result; the current
leader-drain path does not execute Scribe fragments and must fail. Run:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E "test(=distributed::live_query_routes_only_relevant_scribes)"'
```

**GREEN.** Discover active streams after tenant/table authority and published
cut pinning, before physical planning. Bind reported node incarnation, writer
epoch, table, and partition; select routes by safe physical pruning, account
selected live work in admission, and dispatch only those routes. Rerun the
focused journey and Scenario 1 contract tests.

**REFACTOR.** Reuse the current authenticated `ListActiveStreams` boundary and
existing table/partition identities. Do not make Postgres batch commits into
a second current-owner inventory or send scan work to every ready Scribe.

### Scenario 3 — One plan combines distributed files with Scribe-local scans

**Behavior.** A filter and aggregate over stable published files on multiple
Oracle workers plus live rows on two Scribes produces the correct result.
Oracle keeps one physical plan and automatically derives Interactive or
Analytical from its root; published work remains distributed. Scribe applies
assigned projection and supported predicates, with DataFusion residuals still
authoritative. General GROUP BY and joins need not move to Scribe. Covers
REQ-003, INV-002, INV-003, AC-003.

**RED.** Add the real-server Oracle journey
`distributed::published_workers_and_live_scribes_share_one_plan` in the
existing `oracle` target. Assert rows and published-worker and Scribe-fragment
destinations; a leader-drained or published-only result must fail. Run:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E "test(=distributed::published_workers_and_live_scribes_share_one_plan)"'
```

**GREEN.** Replace Oracle's local drained-live leaf with a bounded streaming
live source backed by the existing authenticated Scribe fragment handler,
Scribe provider resolver, signed assignments, and Scribe admission. Keep that
source within the existing Oracle leader execution boundary while published
fragments continue across Oracle workers. Decode and validate frames as they
arrive; natural stream exhaustion requires a final footer. Rerun all prior
scenario tests.

**REFACTOR.** Reuse the one planner and existing peer protocol. Eliminate any
whole-fragment attempt buffer on this live path; keep only bounded in-flight
batches and the current DataFusion residual-filter behavior.

### Scenario 4 — Live resources follow the stream, not a second timeout

**Behavior.** More than one Arrow batch streams with bounded backpressure.
Cancellation and client disconnect release Scribe snapshot references and
admission; an ordinary fragment held open beyond 30 seconds remains valid
until query cancellation/deadline or completion. Already-open staged readers
stay readable across publication. Covers REQ-005, REQ-006, INV-001, INV-005,
AC-004, AC-007.

**RED.** Add the real-server Oracle journey
`distributed::live_stream_backpressure_and_query_owned_lifetime`. Use a
controlled pause **at Scribe source production before a later batch exists**.
While that production remains paused, assert that an earlier batch has already
reached Oracle's live DataFusion source; multiple transport batches after a
whole-cohort fetch do not satisfy this assertion. Hold the pause long enough
to cross 30 seconds, observe bounded in-flight retention and backpressure,
then resume and assert success. Also cancel an open stream and separately drop
a public client stream while its Scribe source is open; each must release its
snapshot and admission. Use the existing stream-drop journey pattern. Use no
synthetic host load. The current
tail-fence expiry or whole-cohort materialization must fail this proof. Run:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E "test(=distributed::live_stream_backpressure_and_query_owned_lifetime)"'
```

**GREEN.** Tie Scribe source references, retained bytes/batches, and admitted
resources to the accepted fragment stream and query cancellation tree. Preserve
peer-ticket acceptance expiry as replay protection only. Once no query caller
uses it, remove acquire/page/release, 30-second expiry, leader drain, local
drained binding, and obsolete metrics/hooks; keep authenticated stream listing.
Rerun earlier scenarios.

**REFACTOR.** Let ordinary stream completion/drop own cleanup. Remove legacy
tail-fence branches instead of adding a lease-renewal or watchdog protocol.

### Scenario 5 — LIMIT stops unneeded live work without draining

**Behavior.** Oracle plans a `LIMIT` query over published files and live
Scribe routes in the same DataFusion root. When that plan has enough rows and
successfully completes, an opened live child it no longer needs is cancelled
and releases its snapshot and admission without draining to a footer. This is
a successful query, not a degraded or failed one. A still-needed stream that
ends unexpectedly remains a failure. `ORDER BY ... LIMIT` still returns the
correct top rows across published and live sources; only the completed plan
decides when a child is no longer needed. Covers REQ-001, REQ-003, REQ-004,
REQ-005, INV-003, INV-005, AC-003, AC-004, AC-005.

**RED.** Add the real-server Oracle journey
`distributed::limit_stops_unneeded_live_fragment_without_footer`. Assert an
opened Scribe fragment is stopped by a completed `LIMIT` plan, the query has
one successful terminal, no footer is demanded from that intentionally
stopped child, and its resources release without reading its remaining rows.
Include an ordered-limit result that could be changed by a live row, proving
Oracle did not impose its own early-stop rule over DataFusion. The current
mandatory-footer semantics or leader drain must fail. Run:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E "test(=distributed::limit_stops_unneeded_live_fragment_without_footer)"'
```

**GREEN.** Treat parent-plan completion as the authority to cancel and drop
unneeded children. Require a valid footer when a fragment is naturally
consumed; retain failure for unexpected EOF while still needed. Rerun all
earlier scenarios and the focused LIMIT journey.

**REFACTOR.** Use the existing DataFusion plan and structured stream ownership
to stop work. Do not drain all rows for a footer, add a timeout, or add an
Oracle-specific LIMIT planner or guard.

### Scenario 6 — Known live loss degrades; late and required faults fail

**Behavior.** An absent-before-discovery Scribe can yield Success with the
documented best-effort coverage. A failed listing or selected Scribe lost
before its first row yields Degraded plus `LiveTailUnavailable`. A failure
after a live row while the plan still needs the fragment, or an unexpected
missing/invalid footer, yields Failed. Published-source,
security, tenant, schema, protocol-integrity, resource, cancellation, and
deadline faults fail; no such fault is hidden as a live omission. No client
accepts preceding rows as a complete result after failure. Covers REQ-001,
REQ-002, REQ-004, INV-002, INV-004, AC-005.

**RED.** Add the real-server Oracle journey
`distributed::live_query_terminal_failure_matrix`; drive the early, late,
published, and security cases at the owning boundaries and assert terminal
metadata and client rejection. Current Strict/AllowDegraded handling and
buffered fragment attempts cannot meet this matrix. Run:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E "test(=distributed::live_query_terminal_failure_matrix)"'
```

**GREEN.** Set terminal state from selected-source progress and stable failure
classes. Preserve fatal treatment of published and trust-boundary faults;
degrade only known unavailable live sources before their first row. Validate
the Scribe footer and stream closure. Rerun every earlier scenario test.

**REFACTOR.** Keep one terminal decision path and the existing stable error
catalog. Delete policy branches whose states are no longer reachable from the
one public query contract.

### Scenario 7 — Drift and all client surfaces use the one query service

**Behavior.** Drift mints its narrow SYSTEM read authority and calls the
ordinary `ScheduledQueryCaller`, which enters the same query service and
local-or-peer Oracle dispatch as any caller. It has no source selector and
uses a successful terminal before producing a judgment. A failed query does
not yield a judgment from partial rows. Rust, Python, TypeScript, HTTP, CLI,
gRPC, and MCP expose no visibility/freshness input and agree on the terminal;
the publication-overlap best-effort limit applies to verification too. Covers
REQ-001, REQ-006, REQ-007, INV-002, INV-004, AC-001, AC-006.

**RED.** First update the existing Rust Drift journey
`drift_methods_fit_score_persist_and_dispatch` to assert the same query
service, no source selector, and success-terminal requirement. Run:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-sdk-rust --test drift_verification -P journey --run-ignored=all -E "test(=drift_methods_fit_score_persist_and_dispatch)"'
```

Then update the existing Python query journey
`test_bifrost_query_yields_pyarrow_and_terminal` and TypeScript Oracle journey
`uses the public SDK against an in-process Wyrd server` to use the one request
and terminal shape; both must fail on the old client contract. Run them
individually through the repository-managed Postgres lifecycle:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-python && mise run py:setup && mise exec -- uv run python -m pytest -q -m integration tests/integration/test_bifrost_query.py::test_bifrost_query_yields_pyarrow_and_terminal'
scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-ts/wyrd && mise run ts:build && mise run ts:build:testing && mise exec -- pnpm exec vitest run tests/integration/oracle-query.test.ts -t "uses the public SDK against an in-process Wyrd server"'
```

**GREEN.** Remove old mode inputs from every first-class client and server
projection. Keep Drift on `ScheduledQueryCaller` and the ordinary query
service, including local and peer-forwarded Oracle dispatch. Update its
terminal handling without a verifier-only read path. Regenerate schemas,
stubs, and declarations from source. Close the CLI, HTTP, gRPC, and MCP
consumers and their owning journeys. Rerun all RED commands and earlier
scenario tests.

**REFACTOR.** Remove obsolete policy imports, fixtures, arguments, docs, and
generated artifacts through their source generators. Keep one shared client
transport/decoder and one server query service; do not add per-language or
verification query logic.

## Acceptance Criteria

- All AC-001 through AC-008 in approved spec revision 3 pass. In particular,
  one request shape is used by every caller; class comes only from DataFusion's
  physical root; two relevant Scribes execute live fragments while an
  irrelevant third does not; published work remains distributed; streaming is
  bounded and survives beyond 30 seconds; failure terminals distinguish
  known pre-row live loss from fatal late and required faults; Drift uses the
  ordinary service; and the old tail-fence path is absent.
- The exact published/live handoff is not claimed. Documentation and evidence
  explicitly cover possible omission or duplicate during publication, even
  for verification. No test forces a global exactness promise by adding a
  hidden inventory, handoff fence, or special query mode.
- Every required user journey covers the real client-to-server boundary. Unit
  and integration tests support the journeys but never replace them.

## Expected Write Set and Consumer Closure

Likely owners and consumers, not a file allowlist:

- `crates/wyrd-spec/src/vala/api.rs` and generated schemas/protocol
  projections; `crates/shared/wyrd-client/src/bifrost/` and first-class Rust,
  Python, and TypeScript SDKs and tests.
- `crates/vala/vala-bifrost-redux/src/oracle/` and Scribe live-source/fragment
  owners; `crates/wyrd/wyrd-server/src/oracle/`, query service and adapters,
  MCP and CLI; `crates/wyrd/wyrd-server/src/verification/drift.rs` and shared
  scheduled-query consumer.
- Existing `wyrd-testing` Oracle, Scribe, server, and MCP journeys;
  Rust/Python/TypeScript client journeys; generated and contract checks.
- Approved `architecture/wyrd-design.md`, `architecture/bifrost-design.md`,
  relevant architecture references and runbook, and Bifrost user docs already
  revised on this branch. Ensure final prose matches shipped behavior.

## Verification and Evidence

Run the exact RED commands above one scenario at a time, record the expected
failure, implement the minimum GREEN, rerun its focused command and prior
scenarios, and simplify under REFACTOR while green. A specifically named test
added or renamed during implementation must record and run its exact focused
`mise exec --` command with the actual package, target, features, and selector.
Postgres-backed tests use the repository wrapper and migrations shown above;
do not run overlapping Cargo builds. No synthetic host load or 512 MiB
production-geometry qualification is needed for this query rewrite.

After focused proof, run the owning full journeys and checks sequentially:

```bash
mise run test:bifrost:journey:oracle
mise run test:bifrost:journey:scribe
mise run test:bifrost:journey:drift
mise run test:bifrost:journey:sdk
mise run test:bifrost:journey:python
mise run test:bifrost:journey:typescript
mise run test:bifrost:journey:mcp
mise run test:bifrost:journey:server
mise run fmt
mise run lints
mise run py:format
mise run py:lints
mise run py:typecheck
mise run ts:build
mise run ts:typecheck
mise run ts:napi:check
mise run codegen:check
mise run test:principals:integration
mise run docs:check
mise run check:client-tier
mise run check:pyo3-scope
mise run check:unwrap-audit
mise run verify:bifrost
mise run gate
git diff --check
```

The docs check passed during planning on 2026-09-26; rerun it after
implementation to confirm the source and examples agree with the shipped API.
Static architecture/docs, generated artifacts, and removal of obsolete mode
and tail-fence surfaces need no manufactured RED; inspect their source diff,
run the named generators/checks, and confirm no stale public contract remains.
Record results and final diff evidence under `$wyrd-implement`'s execution
report. All required lanes must be green before the task is reported complete.

## Material Stop Conditions

- A finding that requires a second public query mode, a verifier-only source
  path, caller-selected class, changed ACK/publication semantics, durable
  current-owner inventory, atomic publication handoff, or a stronger guarantee
  than approved best effort requires a new spec revision and human approval.
- A real security, tenant, schema, or published-source failure cannot be
  represented as `Degraded`; stop for specification authority if the approved
  terminal distinction proves impossible within the existing protocol.
- An implementation may choose private streaming and fixture mechanics within
  the approved boundaries. Ordinary code/test/tool failures are not material
  stop conditions; diagnose and fix them under the repository rules.

## Authority Links

- [Approved spec revision 3](../spec.md)
- [AGENTS.md](../../../../AGENTS.md)
- [Agent rules](../../../../architecture/agent-rules.md)
- [Wyrd design](../../../../architecture/wyrd-design.md)
- [Bifrost design](../../../../architecture/bifrost-design.md)
- [Spec-driven development](../../../../architecture/references/languages/spec-driven-development.md)
- [Implementation execution](../../../../architecture/references/languages/implementation-execution.md)
- [Testing workflows](../../../../architecture/references/languages/testing-workflows.md)

## Implementation Evidence

Commits `d1ec13200..abadc452e` (4b617f0f9 through abadc452e) on `vcc/task-005`. Journeys ran through
`scripts/postgres/with-test-postgres.sh` with migrations, one at a time.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| AC-001 one request, three-tier terminal, no visibility/freshness input | `wyrd-spec` `BifrostQueryRequest{sql,deadline_ms}`, `QueryTerminalFrame` (4b617f0f9, 950e73ef4); client, CLI, MCP and SDK projections | `wyrd-spec` `query_terminal_tests::{closed_terminal_matrix_validates, query_request_has_no_source_selectors_and_validation_is_closed}`; `codegen:check`; Python `test_bifrost_query.py` (8 pass); TS `oracle-query.test.ts` (8 pass) asserting no `freshness` | PASS |
| AC-002 only relevant Scribes execute live fragments | Oracle `discover_live_routes` + route selection before the one plan (7a2dab924) | `distributed::live_query_routes_only_relevant_scribes` | PASS |
| AC-003 published work stays distributed beside live leaves | `LiveUnionBoundary`, `LiveScribeExec` (5a4e9188c) | `distributed::published_workers_and_live_scribes_share_one_plan` | PASS |
| AC-004 bounded streaming, >30 s lifetime, release on cancel/drop | `LiveTailBatches::into_stream`, `LiveTailPartition`, query-owned lease (b8b4ec66b) | `distributed::live_stream_backpressure_and_query_owned_lifetime`; redux `tail_rpc` staged-lease tests | PASS |
| AC-005 LIMIT stop without footer; Degraded vs Failed matrix | `LiveFragmentRead::into_stream`; `live_execution_status_error` (Scribe open: `Unavailable`→degradable, `ResourceExhausted`→`Capacity`); test-only `ScribeFragmentFault` hook in `wyrd-server` `oracle/peer_service.rs` (6553f5adc) | `distributed::limit_stops_unneeded_live_fragment_without_footer`; `distributed::live_query_terminal_failure_matrix` (listing loss and pre-row loss Degraded; late loss, missing footer, ticket rejection, capacity refusal and deleted published files Failed and client-rejected; absent-before-discovery Success); `oracle::dispatcher::tests::live_scribe_open_status_separates_availability_from_faults`; `write_read::scribe_undialable_private_peer_degrades_live_coverage` | PASS |
| AC-006 Drift uses the one query; judgment only after a non-failed terminal; docs state both races | `ScheduledQueryCaller` unchanged; Drift journey live step (7143e5e0c, e2207970d); docs (32443a1ee, 9a88a61f1) | `drift_verification::drift_methods_fit_score_persist_and_dispatch` scores unflushed observations; failed terminal → no outcome pinned by `query::scheduled::tests::scheduled_terminal_requires_clean_eof` (unit only: injecting a failure into the server-side scheduled query needs a hook that does not exist); `docs:check` | PASS |
| AC-007 tail-fence acquire/page/release and 30 s lifetime removed; listing kept | 86d133769, 40a644ab6 | Source sweep finds no tail-fence surface (the persisted `tail_fence` audit enum value is kept for historical rows); `test:bifrost:journey:oracle` 33/33 | PASS |
| AC-008 fmt, lints, codegen, docs, `verify:bifrost`, `gate` | — | `fmt`, `lints`, `py:format`, `py:lints`, `py:typecheck`, `ts:build`, `ts:typecheck`, `ts:napi:check`, `codegen:check`, `test:principals:integration` (18/18), `docs:check`, `check:client-tier`, `check:pyo3-scope`, `check:unwrap-audit`, `git diff --check` pass. `verify:bifrost`: 8/9 lanes passed; `unit:rust` failed on a fixture that listed only two source tiers, fixed in a026fd6e3 and rerun at 220/220. `gate`: every dependency except `check:skills-sync` ran one at a time and passed (47/47, including `check`, `test:rust`, `test:bifrost:gate`, `test:gateway:gate`, `test:identity:journey`, `py:test:integration`, `ts:test:integration`, `py:test:unit`, `ts:test:unit`, and every `check:*` and examples lane). `check:skills-sync` passes after the approved `wyrd-task-review` edits were synced and committed (e15c610af) | PASS |

Non-goals stayed excluded: no public mode or class option, verifier-only
route, Flight listener, durable owner index, replacement tail lease, second
planner, scheduler, aggregate or join pushdown, persistent state,
compatibility mode, or ACK/publication change.

Flake fixed: `scribe::write_read::scribe_write_flush_read_user_journey` failed
once because the canonical workload's rows took the server receipt time as
their event time. A run whose two writes to one table straddled an hour
boundary put those rows in two hourly partitions and published two files, so
the cache-off/cache-on layout parity failed. The runner now writes one pinned
`wyrd_event_time` per run (e7f053c7d); the focused journey passes.
