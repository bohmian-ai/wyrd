---
id: TASK-002
kind: implementation
status: proposed
spec: SPEC-verification-closeout
spec_revision: 1
requirements: [REQ-004, REQ-005, INV-002, INV-003, INV-004, INV-005, INV-006, AC-003]
depends_on: []
provenance: TASK-017-R3A
---

# R3A: One non-blocking Scribe outbox

## Outcome and Value

Audit, gateway capture, queued Eval and Drift results, and realtime Verifier
results all use the existing `Outbox<S>` producer/consumer pattern to write
batches through one `ScribeSink`, regardless of whether Scribe is local or a
peer. Observation-run scheduling remains a separate outbox.

## Owners, Scope, Consumers, and Prohibited Changes

- `wyrd-runtime::Outbox<S>` remains the only generic non-blocking process
  outbox. Extend it only if an existing contract is demonstrably insufficient.
- The server owns one `ScribeOutbox`; `ScribeSink` owns local/peer routing and
  delegates accepted batches to the existing Scribe APIs.
- Audit, gateway, and verification retain their domain projection,
  attribution, stable batch identity, and validation. They relinquish their
  independent Scribe delivery lifecycle.
- `ObservationRunOutbox` and `ObservationRunSink` remain unchanged in purpose
  and independently drainable.
- Do not add PostgreSQL generic staging, claims, leases, polling, a second
  publisher, new concurrency, one network call per record, or compatibility
  paths.

## Reuse Map

| Capability | Existing owner/symbol | Inspected callers/tests | Missing behavior | Selected extension | New machinery justification |
|---|---|---|---|---|---|
| Non-blocking batching | `wyrd_runtime::Outbox<S>` and `OutboxSink` | `AuditOutbox`, `ObservationRunOutbox`, shutdown tests | Scribe-bound producers do not share one sink | Instantiate `Outbox<ScribeSink>` and retain its batching/retry/drain behavior | One sink is required because no common Scribe sink exists |
| Local/peer delivery | `GatewayCapture` capture route and peer service | gateway capture and verifier runtime tests | Routing is owned by a gateway-specific lifecycle | Move the existing route behind `ScribeSink` | None; reuse the existing transport |
| Audit recording | `AuditOutbox`, canonical append/projection, `AuditPublisher` | audit outbox/publication tests | Audit owns a separate Scribe publisher | Preserve audit preparation; hand its canonical batch to `ScribeOutbox` | None |
| Verification results | runner result projection/publication and direct executor | verification runtime and route tests | Queued results publish directly; realtime results are not recorded | Hand canonical result batches to `ScribeOutbox` | None |
| Observation work | `ObservationRunOutbox` and `ObservationRunSink` | observation enqueue and shutdown tests | No gap | Keep unchanged | None |

## Approach

1. Introduce the shared `ScribeOutbox` composition and a state-owning
   `ScribeSink` over the existing local/peer route.
2. Redirect audit, gateway capture, and queued verification result publication
   to the shared outbox without moving their domain projection or weakening
   stable batch identity.
3. Give realtime result recording the same producer path for Task 3 to consume.
4. Remove superseded producer-specific Scribe retry, deadline, and publication
   ownership while retaining domain preparation and required settlement.
5. Wire startup and shutdown so accepted Scribe work drains independently from
   the unchanged observation-run outbox.

## Required Delivery Contract

The implementation boundary is exactly:

```rust
pub type ScribeOutbox = Outbox<ScribeSink>;
pub type ObservationRunOutbox = Outbox<ObservationRunSink>;
```

```text
Audit ───────────────┐
Gateway capture ─────┤
Eval/Drift result ───┼→ ScribeOutbox → ScribeSink → local or peer Scribe
Verifier result ─────┘

Observation ACK → ObservationRunOutbox → ObservationRunSink → verifier_runs
```

One `ScribeOutbox` item is the complete existing Scribe batch, including its
tenant, destination, stable batch identity, encoded Arrow payload, request
identity, and writer/verifier attribution. It is not one analytical record.
The existing outbox worker performs the continuous pull from its in-memory
queue and supplies a per-tenant slice to `ScribeSink::write`.

`ScribeSink` must reuse the current in-process/peer transport. If that
transport accepts one Arrow batch per call, the sink writes the already-batched
items without inventing another aggregation layer. The task must not introduce
`CLAIM_SIZE`, database claims, consumer leases, `SKIP LOCKED`, a generic
PostgreSQL Scribe table, or parallel publication controls.

Producer closure is explicit:

| Producer | Retained ownership | Removed ownership |
|---|---|---|
| Audit | non-blocking decision intake, sequence/hash integrity, audit projection | direct Scribe publisher/route |
| Gateway | call/span projection and stable batch IDs | deadline-bound Scribe retry/drop loop |
| Queued Eval/Drift | judgment, result projection, run/lease settlement | direct Scribe publication loop |
| Realtime Verifier | inline Judgment and result projection | separate `DirectResultSink` |
| Observation acknowledgement | durable run request construction | none; it remains separate |

Scribe delivery failure is returned to the existing outbox worker so it can
restore the same items to the front. A retry never rebuilds a different batch
identity. Shutdown stops admission, drains producer work using the repository's
existing ordering, and leaves observation scheduling independently drainable.

## Ordered Implementation Scenarios

### Scenario 1 — Every Scribe producer uses one batched sink

**Behavior.** Audit, gateway capture, queued Eval/Drift results, and realtime
Verifier results stage canonical batches on `ScribeOutbox`. `ScribeSink`
receives the existing per-tenant slice, writes it through local or peer Scribe,
and preserves each batch identity and attribution.

**RED.** Add focused owner tests proving each producer reaches one recording
sink and that both local and peer routes receive the same batch metadata. The
current separate audit publisher, gateway delivery, queued-result publication,
and absent realtime write fail this proof.

**GREEN.** Compose one `Outbox<ScribeSink>`, reuse the existing route and batch
types where they fit, and redirect only the delivery seam of each producer.

**REFACTOR.** Delete superseded delivery loops and route ownership after every
producer uses the common sink; keep domain projection with its current owner.

### Scenario 2 — Failure and shutdown use the existing outbox contract

**Behavior.** A failed local or peer Scribe write returns the batch to the
existing outbox retry path. An orderly shutdown drains accepted Scribe work.
Observation-run work drains separately and is never routed to Scribe.

**RED.** Extend the existing outbox/shutdown and Scribe-route tests with one
failed-then-successful batch and independent Scribe/observation drains. The
current disconnected lifecycles fail the integrated assertions.

**GREEN.** Wire the two outbox owners into the existing boot and shutdown
sequence without new scheduling, persistence, claiming, or concurrency code.

**REFACTOR.** Remove dead publisher startup/shutdown branches and keep one
discoverable owner for each outbox.

## Acceptance Criteria

- AC-003 passes for all four producer families and both Scribe routes.
- `ScribeSink::write` receives batches from the existing `Outbox<S>` worker;
  no producer performs one Scribe call per record.
- Retry, deduplication identity, metrics, and orderly drain remain observable.
- No Scribe-bound producer owns an independent publisher or delivery loop.
- Observation-run enqueueing remains behaviorally unchanged.

## Expected Write Set and Consumer Closure

- `crates/shared/wyrd-runtime/src/outbox.rs` only if its existing public
  contract needs no-behavior-change composition support.
- Server boot/state/shutdown, gateway capture routing, audit publication seam,
  verification runner/direct-result seam, and their focused tests.
- Existing Vala audit/result projection owners only where needed to hand a
  canonical batch to the shared outbox.
- Peer Scribe admission only if its current closed destination set rejects a
  canonical audit or result batch.
- `AGENTS.md`, `architecture/agent-rules.md`, `architecture/wyrd-design.md`,
  and `architecture/bifrost-design.md` to replace the obsolete separate-writer
  authority with this approved boundary.

## Verification and Evidence

- Exact focused tests added to the existing runtime outbox, audit publication,
  gateway capture, verification runtime, and server shutdown targets.
- `mise run test:bifrost:integration:server`
- `mise run test:bifrost:integration:redux`
- `mise run test:sql`
- `mise run check:deps`
- `mise run check:tenant-isolation`
- `mise run fmt`
- `mise run lints`

## Material Stop Conditions

- A producer cannot hand its canonical batch to `ScribeOutbox` without
  weakening audit integrity, result identity, tenant attribution, or current
  run settlement.
- The local and peer paths cannot accept the same canonical batch contract
  without a public or cross-service protocol decision absent from the spec.
- Implementation requires generic durable staging, claims, leases, polling, or
  new concurrency rather than the approved existing `Outbox<S>` behavior.

## Authority Links

- `../spec.md` revision 1: REQ-004..005, INV-002..006, AC-003
- `AGENTS.md`; `architecture/agent-rules.md`;
  `architecture/wyrd-design.md`; `architecture/bifrost-design.md`;
  `TESTING.md` (definitive Wyrd guide for test ergonomics,
  understandability, structure, ownership, and lane selection)
