---
id: TASK-002
kind: implementation
status: proposed
spec: SPEC-verification-closeout
spec_revision: 2
requirements: [REQ-004, REQ-005, INV-002, INV-003, INV-004, INV-005, INV-006, INV-007, INV-008, AC-003]
depends_on: []
provenance: TASK-017-R3A
---

# R3A: One in-memory Scribe outbox

## Outcome and Value

Audit records, gateway captures, queued Eval and Drift results, and realtime
Verifier results all use one non-blocking producer/consumer path. The existing
outbox worker drains logical writes from memory, `ScribeSink` batches them, and
the existing local or peer Scribe path admits and persists them. Observation-run
scheduling remains a separate PostgreSQL-backed outbox.

## Owners, Scope, Consumers, and Prohibited Changes

- `wyrd-runtime::Outbox<S>` remains the generic non-blocking, per-tenant
  producer/consumer owner. Change only the retry boundary required to replay an
  identical failed slice without merging later arrivals into it.
- The server owns one `ScribeOutbox`; its closed `ScribeWrite` input represents
  audit, gateway-capture, and verification-result domain writes.
- `ScribeSink` owns destination grouping, Arrow batch encoding, deterministic
  batch identity, and the existing in-process/peer route.
- Existing Scribe ingress remains the only owner of capacity admission, byte
  accounting, memory leases, WAL persistence, and acknowledgement.
- `ObservationRunOutbox` and `ObservationRunSink` remain unchanged in purpose
  and independently drainable.
- Remove the superseded audit and verification-result PostgreSQL staging and
  publication paths. Already retained Scribe history is not rewritten.
- Do not add a PostgreSQL Scribe queue, another consumer, polling, claims,
  leases, `SKIP LOCKED`, a claim size, new publication concurrency, a capacity
  setting, or another memory governor.

## Reuse Map

| Capability | Existing owner/symbol | Inspected callers/tests | Missing behavior | Selected extension | New machinery justification |
|---|---|---|---|---|---|
| Non-blocking producer handoff | `wyrd_runtime::Outbox<S>` and `OutboxSink` | `AuditOutbox`, `ObservationRunOutbox`, and outbox retry/shutdown tests | Scribe-bound producers do not share one sink; a failed slice is currently merged with later arrivals before retry | Specialize it as `Outbox<ScribeSink>` and preserve a failed write as the next identical slice | The small retry-boundary correction is required for stable content-derived Scribe batch identities |
| Local and peer Scribe delivery | `GatewayCapture`, `CaptureRoute`, and `CaptureBatch` | gateway capture route tests and peer capture service tests | Routing is gateway-specific | Reuse the route behind `ScribeSink` | None |
| Scribe admission and durability | `AdmissionController`, `ScribeResources::try_reserve_ingress`, and `ScribeIngressFrame` | Scribe admission/resource tests and local/peer ingest tests | No gap | Reuse unchanged after `ScribeSink` submits a batch | None; duplicate admission is prohibited |
| Audit recording | `AuditOutbox`, `AuditSink`, `AuditPublisher`, and `AuditLogTable` | audit outbox/publication tests and server tests reading `vala.audit_staging` | Audit has a separate PostgreSQL staging and publisher path | Stage logical audit events on `ScribeOutbox`, batch them in `ScribeSink`, and retain only the decision-content columns in `audit_log` | None |
| Gateway capture | `GatewayCapture::publish` | gateway capture and invocation tests | Delivery is tied to the call deadline | Stage the validated logical capture on `ScribeOutbox` | None |
| Queued verification results | runner `store_result`/`publish` flow and `GatewayCapture::write_result` | verification runtime and lease tests | Encoded results are staged in `wyrd.verifier_run_results` and published by the runner | Stage the logical result, then settle the run | None |
| Realtime verification results | direct verification executor | direct-route tests | The verdict is returned but not recorded | Stage the same logical result shape on `ScribeOutbox` | None |
| Observation work | `ObservationRunOutbox` and `ObservationRunSink` | observation enqueue and shutdown tests | No gap | Keep unchanged | None |

## Approach

1. Add the closed logical `ScribeWrite` input, compose one
   `Outbox<ScribeSink>`, and reuse the current local/peer route.
2. Have each closed variant project its fixed destination set, then let
   `ScribeSink` group the tenant slice by destination, encode complete Arrow
   batches, derive stable identities from the identical logical slice, and
   submit those batches to existing Scribe ingress.
3. Redirect audit, gateway capture, queued Eval/Drift results, and realtime
   Verifier results to that outbox; keep all request and verdict completion
   independent of Scribe acknowledgement.
4. Remove the audit staging/publisher/hash-chain path and the queued-result
   staging/publication path. Settle a queued run after its result is staged in
   memory.
5. Keep retryable failures on the existing outbox backoff, consume and count
   terminal Scribe rejections, and drain accepted work to the existing orderly
   shutdown deadline.
6. Update the architecture authorities and focused tests to describe and prove
   the single best-effort path.

## Required Delivery Contract

```rust
pub type ScribeOutbox = Outbox<ScribeSink>;
pub type ObservationRunOutbox = Outbox<ObservationRunSink>;
```

```text
Audit ───────────────┐
Gateway capture ─────┤
Eval/Drift result ───┼→ ScribeOutbox → ScribeSink → local or peer Scribe
Verifier result ─────┘                         │
                                               └→ existing Scribe admission/WAL

Observation ACK → ObservationRunOutbox → ObservationRunSink → verifier_runs
```

`stage()` performs only the in-memory handoff. The existing outbox background
worker continuously pulls pending values and supplies one tenant slice to
`ScribeSink::write`. `ScribeSink`, not each producer, converts that slice into
destination-specific Arrow batches. Scribe is pushed those batches; it does not
poll the outbox.

There is no PostgreSQL intermediary for Scribe-bound writes. Abrupt process
death and an expired shutdown deadline may lose writes that Scribe has not
acknowledged. Observable rejection and shutdown loss are logged and counted;
abrupt loss is not observable by the failed process. No loss changes the
originating permission decision, gateway call, run settlement, or realtime
verdict. The existing outbox queue remains unbounded, so a prolonged retryable
Scribe outage can grow producer-process memory; this task adds no capacity
setting.

## Ordered Implementation Scenarios

### Scenario 1 — Every Scribe producer uses one batching sink

**Behavior.** Audit, gateway capture, queued Eval/Drift results, and realtime
Verifier results stage logical values on `ScribeOutbox`. For each tenant slice,
`ScribeSink` emits destination-specific Arrow batches rather than one Scribe
call per analytical row, and sends them through the existing local or peer
route with the required attribution.

**RED.** Add focused owner tests that stage multiple same-destination writes
from each producer family and assert that the recording route receives batched
frames with correct tenant, table, principal/Card attribution, and stable batch
identity. The current separate audit publisher, gateway delivery, queued-result
publisher, and absent realtime write fail this proof.

**GREEN.** Compose `Outbox<ScribeSink>`, reuse the existing projection and
route owners, and redirect only the producer-to-delivery seam.

**REFACTOR.** Delete superseded producer-specific delivery loops after all
four producer families use the shared sink; retain validation and domain-value
construction with their existing owners.

### Scenario 2 — Retry, rejection, and shutdown preserve the best-effort contract

**Behavior.** A retryable local or peer failure replays the identical tenant
slice and therefore the identical destination batch IDs; writes staged later
are not merged into that retry. A terminal Scribe rejection is logged, counted,
and consumed so later tenant writes continue. Orderly shutdown drains until its
deadline; abrupt or deadline loss never changes the originating operation.

**RED.** Extend the outbox, route, and shutdown tests with a failed write plus
later arrival, a terminal rejection followed by a valid write, and a shutdown
deadline. Current retry coalescing and producer-specific failure behavior fail
these assertions.

**GREEN.** Make the smallest outbox retry-boundary correction, map only
retryable delivery failures back to that retry path, and use the existing
outbox loss metrics and shutdown lifecycle.

**REFACTOR.** Remove obsolete deadline loops and publisher lifecycle branches;
do not add a scheduler, admission owner, or configuration setting.

### Scenario 3 — PostgreSQL retains work scheduling only

**Behavior.** Audit decisions and queued verification results no longer enter
PostgreSQL before Scribe. The audit staging, chain-head/publication progress,
publisher, and `wyrd.verifier_run_results` paths are removed. A queued run
settles after in-memory staging. Observation acknowledgement still writes
`wyrd.verifier_runs` through `ObservationRunOutbox`.

**RED.** Update focused audit and verification tests to assert retained rows
through Scribe and the absence of the retired staging lifecycle, while keeping
the observation-run enqueue/claim journey green. Existing staging assertions
and runner publication behavior fail this proof.

**GREEN.** Remove the retired owners and tables, project audit decision content
without the gapless chain fields, and stage verification results before run
settlement.

**REFACTOR.** Remove dead migrations/query modules/test fixtures and update the
architecture text; do not preserve unused compatibility paths.

## Acceptance Criteria

- AC-003 passes for all four producer families and both Scribe routes.
- Multiple logical writes for one destination become an Arrow batch; producers
  do not invoke Scribe once per analytical row.
- Retrying an unchanged sink write produces the same batch identity, and a
  terminally rejected write cannot block later writes for its tenant.
- No code before existing Scribe ingress reserves Scribe memory or duplicates
  its admission rules.
- `vala.audit_staging`, its chain/publication state, `AuditPublisher`, and
  `wyrd.verifier_run_results` have no remaining runtime owner.
- Retained audit history contains the authorization-decision content without
  the retired chain-only fields; already retained rows are not rewritten.
- Queued runs settle after staging their result; observation-run enqueueing and
  claiming remain behaviorally unchanged.
- Abrupt-process loss is documented; observable rejection and
  shutdown-deadline loss are logged and counted.

## Expected Write Set and Consumer Closure

- `crates/shared/wyrd-runtime/src/outbox.rs` and its in-module tests for the
  identical-slice retry invariant.
- Server state/boot/shutdown and the audit, gateway capture, verification
  runner/direct-result, peer capture, and focused integration tests.
- Vala audit table/projection and SQL migrations/query modules that own the
  retired staging tables and chain-only fields.
- Existing Bifrost/Scribe tests only where the revised built-in audit schema or
  local/peer route requires consumer updates; Scribe admission itself remains
  unchanged.
- `AGENTS.md`, `architecture/agent-rules.md`, `architecture/wyrd-design.md`,
  and `architecture/bifrost-design.md` to replace the obsolete staging and
  publisher authority.

## Verification and Evidence

- Exact focused tests added to the existing runtime outbox, gateway capture,
  audit, verification runtime, peer route, and shutdown owners; record their
  exact `mise exec -- cargo nextest run` selectors in the implementation
  evidence once named.
- `mise run test:bifrost:integration:server`
- `mise run test:bifrost:integration:redux`
- `mise run test:sql`
- `mise run check:deps`
- `mise run check:tenant-isolation`
- `mise run fmt`
- `mise run lints`

## Material Stop Conditions

- Existing local and peer Scribe ingress cannot accept the same encoded frame
  without a new public or peer-wire contract.
- Removing the chain-only audit fields requires rewriting already retained
  Scribe history rather than changing only the current built-in table contract.
- Observation-run scheduling cannot remain independently durable after the
  result-publication staging path is removed.
- Implementation requires a new durable queue, polling consumer, capacity
  setting, or Scribe admission mechanism rather than the approved in-memory
  `Outbox<S>` path.

## Authority Links

- `../spec.md` revision 2: REQ-004..005, INV-002..008, AC-003
- `AGENTS.md`; `architecture/agent-rules.md`;
  `architecture/wyrd-design.md`; `architecture/bifrost-design.md`;
  `TESTING.md` (definitive Wyrd guide for test ergonomics,
  understandability, structure, ownership, and lane selection)

## Implementation Evidence

Commits `ce56636ba`..`7d3eb319a` on `wyrd-verification-closeout-outbox`.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| AC-003 for all four producer families and both Scribe routes | `crates/wyrd/wyrd-server/src/scribe_outbox.rs` (`ScribeWrite::{Audit,Capture,Result}`, `ScribeSink` local/peer routes); audit via `AuditStage for Outbox<S>`; capture in `components/gateway/capture.rs`; queued and direct results in `verification/runner.rs` | `wyrd-server` lib `scribe_outbox::tests::{writes_group_by_destination_under_their_attribution, audit_decisions_frame_under_the_platform_audit_principal, an_unbound_route_waits_and_an_unreachable_one_drops, scribe_refusals_and_peer_codes_classify_alike}`; `wyrd-testing` `audit_retention::{retained_history_carries_both_credential_shapes, system_owner_security_rejections_retain_once}`; `test:bifrost:integration:server` 87/87 | PASS |
| Multiple logical writes for one destination become one Arrow batch | `ScribeSink::write` groups a tenant slice by destination and encodes one frame each | `scribe_outbox::tests::writes_group_by_destination_under_their_attribution` | PASS |
| Retrying an unchanged write keeps its batch id; a terminal rejection never blocks the tenant | `crates/shared/wyrd-runtime/src/outbox.rs` identical-slice retry; content-derived v7 batch ids | `wyrd-runtime` `outbox::tests::{a_retry_writes_the_identical_failed_slice_without_later_items, a_failed_write_is_retried_once_in_order_ahead_of_later_items, a_failing_tenant_does_not_delay_another_tenant}` (9/9); `scribe_outbox::tests::{a_retry_resubmits_identical_frames, a_terminal_rejection_never_blocks_later_writes}`; `pg_verification_runtime::lost_result_ack_replays_the_identical_sealed_batch_and_scribe_deduplicates` | PASS |
| No pre-ingress Scribe memory reservation or duplicate admission | The sink submits frames to the existing local/peer Scribe ingress unchanged | Source review of `scribe_outbox.rs`; `test:bifrost:integration:redux` 904/904 | PASS |
| `vala.audit_staging`, chain/publication state, `AuditPublisher`, `wyrd.verifier_run_results` have no runtime owner | Additive drops `vala-sql/migrations/20261008000000_drop_audit_staging.sql`, `wyrd-sql/migrations/20261008000000_drop_verifier_run_results.sql`; publisher and staging modules deleted | `git grep` finds no runtime reference outside migrations and the migration test; `vala-sql` `pg_migration` asserts the relations are gone; `test:sql` | PASS |
| Retained audit carries decision content without chain-only fields; retained rows not rewritten | `vala.system.audit_log` built-in contract without `seq`/hash fields; no history rewrite | `audit_retention::retained_history_carries_both_credential_shapes` (reading `seq` fails) | PASS |
| Queued runs settle with their result; observation enqueue/claim unchanged | `runner.rs` builds the result, settles under the lease fence, then stages | `pg_verification_runtime` and `pg_verification_routes` 40/40, including `expired_lease_is_reclaimed_and_the_stale_holder_is_fenced` and `every_tenant_is_claimed_in_the_first_round` | PASS |
| Abrupt-process loss documented; rejection and shutdown-deadline loss logged and counted | `AGENTS.md` §2, `architecture/wyrd-design.md`, `architecture/bifrost-design.md`; `Outbox::shutdown` counts `outbox_events_lost_total` | `outbox::tests::shutdown_flushes_items_that_recover_before_the_deadline`; `scribe_outbox::tests::a_terminal_rejection_never_blocks_later_writes` | PASS |

Lanes: `fmt`, `lints`, `codegen:check`, `check:deps`, `check:tenant-isolation`,
`git diff --check`, `test:bifrost:integration:server` (87/87),
`test:bifrost:integration:redux` (904/904), `test:sql`,
`test:identity:journey` (all green), and the wyrd family suite (2397/2397).

Exact selectors: `mise exec -- cargo nextest run --locked -p wyrd-runtime --lib -E 'test(/outbox::tests::/)'`;
`mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(/scribe_outbox::tests::/)'`;
`mise exec -- cargo nextest run --locked -p wyrd-testing --test server --run-ignored all -E 'test(/audit_retention::/)'`
(Postgres wrapper); `mise exec -- cargo nextest run --locked -p wyrd-server --test integration -E 'test(/pg_verification_(runtime|routes)::/)'` (Postgres wrapper).

### Deviations and material limits

- The queued result is staged after its lease-fenced settlement reports
  `completed`, not before. Staging first let an expired-lease holder publish a
  second, orphaned result (`expired_lease_is_reclaimed_and_the_stale_holder_is_fenced`).
- Each gateway request stages its own capture frame; `Phase::Publication` is
  removed; a direct result's `result_id` is its `execution_id`.
- The audit decision time is the process clock at staging.
- The built-in audit schema changed without a history upgrade path (precedent
  `b7185d0ee`); retained rows are not rewritten.
- Decisions staged by an Oracle drain after the outbox shutdown fence are lost
  and counted.
- Tests that injected audit-write failures were deleted: staging cannot fail,
  and the `Outbox` unit tests own loss accounting.
- System-owner audit is not readable through Oracle; it is proved through
  `vala.scribe_batch_commits`. Three `platform_admin_e2e` journeys now assert
  only that system-owner batches increased (tenant-slug resource and the
  absence of credential material are not re-asserted).
- The `audit_publication` harness option was removed from the Rust, Python,
  and TypeScript test servers.
- Test harness: every start path composes and binds on the shared Wyrd
  runtime its `Drop` drives, and in-process teardown drains the Scribe outbox
  and Bifrost like the serve task (`Bifrost::mark_unsupervised_for_test`).
  The fault-injection result outbox (`PublicationFault::outbox`) also starts
  on the shared runtime; started on a test's current-thread runtime, its
  in-flight Scribe write held Bifrost's teardown abort forever under load.
  `tenant_connection_rotation_journey` reads each replica's audit before that
  replica stops, because a stopped replica's rows reach others only through
  Forge publication.
- The stub-scanning test reads the generated `native.d.cts` and lists
  `grpc_url`/`bootstrap_user` and `bootstrapService`/`bootstrapUser` as setup.
- Host: psql 14 lacks `\getenv`, so Postgres lanes ran with
  `/opt/homebrew/opt/libpq/bin` first on PATH. macOS bash 3.2 lacks `mapfile`,
  so the wyrd family suite ran as the same `cargo nextest` command
  `scripts/run-family-tests.sh` builds, not through the script.
