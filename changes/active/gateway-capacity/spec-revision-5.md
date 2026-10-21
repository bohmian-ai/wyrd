---
id: SPEC-gateway-capacity
revision: 5
status: approved
base_revision: 4
---

# Proposed revision 5 — capture loss under saturation

This is a minimal amendment to [approved revision 4](spec.md) and its
[revision-4 amendment](spec-revision-4.md). Unchanged obligations remain in
force. Steven explicitly approved it on 2026-10-21.

## Problem

The first release benchmarks on the completed startup path (TASK-002,
2026-10-21, AMD Ryzen 9 9950X, 32 CPU) show gateway captures lost whenever
closed-loop clients drive calls faster than one tenant's captures can be
written:

| Overhead row | Calls/s | Captures lost | Peak memory |
|---|---|---|---|
| chat, 1/4/16 clients | 4.7k / 16.2k / 34.1k | 0 | ≤1.6 GiB |
| chat, 64 clients | 39.7k | 131,348 | 1.8 GiB |
| chat-stream, 64 clients | 31.4k | 6,599 | — |
| chat-capture (64 KiB in/out), 16 / 64 clients | 10.0k / 8.4k | 179,585 / 135,814 | 8.24 / 7.27 GiB |

Every loss is the outbox shedding at its limit (`outbox is full; the staged
item was lost`). The gateway hands captures to the outbox with unreserved
staging, which may use a quarter of the 65,536-item capacity; the other three
quarters are reserved for producers that refuse instead, such as verification.
The Scribe sink uses the outbox's default single lane, so one tenant's
captures are written one batch at a time. Each call stages two items, its
audit decision and its capture. The profiled 64-client chat run shows the
lane is bound by acknowledgement latency, not CPU: 85 writes averaged
119.7 ms, each sending about four frames that waited about 20 ms apiece for
Scribe's acknowledgement (including a 5.4 ms write-ahead-log fsync). That is
about 7,600 items per write, roughly 16 µs per item, or 65–70k items/s for one
tenant on this host. The 64 KiB case moves about
1.3 GB/s of captured payload at 16 clients. The limit counts items, not bytes,
so large captures held 7–8 GiB before shedding. Each shed item also wrote one
error log line: 521,582 lines in the full matrix.

Revision 3 requires zero loss on every overhead row (REQ-005.3/4). No finite
writer satisfies that against a closed-loop driver and an instant upstream:
the driver raises its rate until something saturates. Admission back-pressure
would satisfy it, but `AGENTS.md` locks the opposite rule for every Scribe
write: loss is counted and never changes the originating operation. The
gateway, like OpenTelemetry's batch processors and standard gateway access
logging, must not refuse or slow a call because evidence capture is behind.

## Proposed change

Keep capture best-effort and make its limits explicit, bounded and measured:

- Shedding at the outbox limit is the only loss a saturated run may show, and
  it is counted separately from every other loss.
- The outbox is bounded by captured bytes as well as items, so saturation
  cannot exhaust memory.
- Benchmarks report saturation shedding as the measured capture ceiling
  instead of an integrity failure; every other loss remains an integrity
  failure.
- Raise one tenant's capture throughput so the measured ceiling moves up.

**Before:** chat at 64 clients reports `FAIL — 131348 outbox items lost`, the
same verdict a dropped write or misattributed row would produce.
**After:** the row reports `CAPTURE-SATURATED — 131348 shed at the outbox
limit; durable capture N items/s, M MB/s`, alongside its independent latency
verdict. A terminal Scribe refusal in the same row would still be `FAIL`.

## Requirements

**REQ-016 — Capture never governs the call; every loss has a reason.** A
gateway call's admission, latency, answer and status never depend on capture
or Scribe progress. The existing `outbox_events_lost_total{outbox}` counter
gains a `reason` label with exactly one fixed value per lost item: `shed`
(refused at a staging limit), `rejected` (terminal Scribe refusal),
`unreachable` (no Scribe reachable), or `shutdown` (staged after the outbox
closed, or still unacknowledged at the shutdown deadline). Sums over
`outbox` alone keep their meaning. `wyrd_gateway_capture_total{outcome=
"staged"}` keeps counting captures handed to the outbox, so REQ-005.4
(staged = arrivals) is unchanged; shedding happens after that handoff and
appears only as `reason="shed"`. Shedding logs at most one record per outbox
and tenant per second, carrying the count shed since the previous record.

**REQ-017 — Bounded by bytes and items.** Each outbox admits staged items only
while both its item count and its accounted bytes are under their limits;
reaching either sheds with reason `shed`, or refuses a reservation as today.
An item's accounted bytes are at least the length of every payload it holds,
such as captured request and response bodies; encoding overhead need not be
counted. The existing reserved/unreserved split applies to bytes the same way
it applies to items. Both limits and their defaults are documented. The
defaults keep REQ-005's memory headroom in every overhead row on the reference
host, including the 64 KiB capture case at 16 and 64 clients.

**REQ-018 — Saturation is a measured ceiling, not an integrity failure.**

- An overhead row whose only loss is `shed`, and which passes every other
  integrity check, is `CAPTURE-SATURATED` when its latency gate passes and
  `FAIL` when it does not. `CAPTURE-SATURATED` is not PASS, never `TARGET`,
  and does not make the command exit nonzero; the report summary lists every
  such row. The row reports items shed, the shed fraction (items shed ÷ items
  staged, over the window and its drain), and durable capture throughput:
  items and accounted bytes the outbox wrote during the traffic window,
  divided by the window's length.
- Any `rejected`, `unreachable` or `shutdown` loss, staged ≠ arrivals, a
  misattributed, duplicated or cross-tenant row, or loss that cannot be
  reconciled remains an integrity FAIL at any load.
- Capacity (open-loop) is unchanged: a rate with any loss is not sustainable.
  During the overload interval `shed` loss is reported and allowed; recovery
  requires zero loss again.

**REQ-019 — Raise per-tenant capture throughput.** The Scribe sink may use
the outbox's existing multi-lane support (`OutboxSink::LANES`, default 1), so
one tenant's captures are written by several concurrent lanes. Every item of
one gateway call shares a lane, as do items the sink requires to observe each
other in arrival order, such as items sharing an idempotency key.
Content-derived batch ids keep retries idempotent across lanes. Different
calls of one tenant may therefore become durable out of arrival order. This is
the expensive-to-reverse decision: before approval of the implementation,
every reader of capture tables and spans is checked to confirm none relies on
cross-call commit order, and the evidence names them.

## Invariants and non-goals

- `AGENTS.md`'s Scribe rule, tenant isolation, verified row attribution,
  batch-id fencing and REQ-015 are unchanged.
- No admission back-pressure, gateway refusal, or opt-in lossless capture
  policy. The gateway keeps unreserved staging; it does not switch to the
  outbox's refusing reservations. Lossless capture is a
  separate product decision for when a tenant needs it.
- No Scribe wire, table schema or migration change. The new metric label is a
  fixed enum and carries no caller-controlled value.
- No change to the ≤500 µs target or the strict <1 ms gate; a saturated row
  never claims the target.

## Acceptance obligations

| ID | Situation and expected observation |
|---|---|
| AC-015 | Each loss path (limit reached, terminal refusal, no Scribe, shutdown) increments exactly its reason; capture failure never changes a call's response. Unit coverage per path; release run shows only `shed` under saturation. |
| AC-016 | Large captures shed on bytes before items; the 64 KiB capture case at 16 and 64 clients keeps memory headroom. Unit boundary tests plus release evidence. |
| AC-017 | The overhead report classifies shed-only rows as `CAPTURE-SATURATED` with ceiling figures, and any other loss as FAIL; report boundary tests and the release matrix. |
| AC-018 | Multi-lane writes keep per-call and idempotency-key ordering, idempotent retry and tenant isolation; outbox tests, the existing two-tenant attribution journeys, and a recorded check of capture/span readers for commit-order reliance. The release matrix records the new ceiling against this revision's baseline. |
| AC-019 | Saturation produces one shed log record per outbox and tenant per second, with counts that sum to `reason="shed"`; unit coverage and the release server log. |

## Resolved decision

**Throughput target for REQ-019:** on the reference host (AMD Ryzen 9 9950X,
32 CPU, Linux; the target does not transfer to other hosts), chat and
chat-stream at 64 clients finish with zero `shed` loss, about 80k items/s for
one tenant against today's ~65–70k. The 64 KiB capture case reports its
ceiling with no target. Recorded at approval.

## History

- 2026-10-21 — Drafted after TASK-002's release benchmarks measured capture
  shedding under closed-loop saturation; Steven agreed to keep capture
  best-effort with counted loss and to revise the zero-loss rule.
- 2026-10-21 — Steven explicitly approved revision 5 with the recommended
  throughput target.
