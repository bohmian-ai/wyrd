---
id: SPEC-gateway-admission-clock
revision: 1
status: draft
---

# Gateway admission reads PostgreSQL time

## Problem

Gateway limits and budgets are shared across every `wyrd-server` replica
through PostgreSQL, but the time that places a call in a limit window, a budget
period, and a concurrency lease is each replica's own clock:

- admission takes `Utc::now()` on the server, truncates it to the UTC minute,
  and binds that minute as the call's limit window;
- the same server time picks the UTC day or month of each budget reservation;
- a concurrency lease expires at server time plus the call's timeout grace;
- admission counts live leases, and settlement prunes lapsed leases and old
  windows, against a server-bound `now`;
- a `GatewayLimitExceeded` refusal reports `retry_after_seconds` from server
  time.

AGENTS.md §15 assigns timestamps used for database coordination, eligibility,
leases, and relative expiry to PostgreSQL: callers bind durations and SQL
derives deadlines from `statement_timestamp()`. The gateway violates that rule.

The violation is observable. Two replicas whose clocks differ by one second
charge calls made in the same real instant to different minute windows, so a
tenant can exceed a per-minute limit by up to one extra window's allowance
around each boundary. A replica whose clock runs ahead lapses other replicas'
leases early and undercounts concurrency. Budget periods split the same way at
midnight UTC.

## Change

**REQ-001 — PostgreSQL places each covered call in time.** For a call that a
limit or budget covers, the admission statement reads `statement_timestamp()`
once and derives from it:

- the call's limit window (the UTC minute containing that instant);
- each budget reservation's UTC day or month period;
- the instant against which live concurrency leases are counted;
- the call's lease expiry, as that instant plus a grace duration the server
  binds.

The server binds no timestamp for any of these.

**REQ-002 — Settlement uses PostgreSQL time.** Settlement prunes lapsed leases
and limit windows older than the previous minute against its own
`statement_timestamp()`. It still charges corrections to the window admission
recorded for the call, not the window current at settlement.

**REQ-003 — Retry advice uses PostgreSQL time.** A limit refusal's
`retry_after_seconds` is the whole seconds from the admission instant to the
next UTC minute boundary, from 1 through 60. Its value range and meaning are
unchanged.

**REQ-004 — Uncovered calls are unchanged.** A call that no limit or budget
covers still touches no admission table and makes no extra database round trip.

## Example

Replica A's clock reads 12:00:59.6, and replica B's reads 12:01:00.3, at the
same real instant. Before this change, two calls under a 10 request-per-minute
limit, admitted on A and B at that instant, are charged to the 12:00 and 12:01
windows. After it, both are charged to the window that PostgreSQL's clock
places them in.

## Invariants

- **INV-001.** Every limit window, budget period, lease expiry and lease
  liveness comparison a gateway admission or settlement makes is derived from
  PostgreSQL's clock.
- **INV-002.** Admission still checks and charges every limit and budget under
  the tenant's admission advisory lock in one statement; nothing is charged
  unless every limit and budget clears.
- **INV-003.** Tenancy and row-level security for gateway admission rows are
  unchanged.

## Scope

In scope: `wyrd.gateway_admit`, `wyrd.gateway_settle`, the gateway ledger that
calls them, and the refusal's retry advice.

Non-goals:

- sliding or token-bucket windows; limits stay fixed UTC calendar minutes;
- changing limit, budget, or refusal wire contracts;
- the clock behind the call's recorded event times (see Open decision);
- any other subsystem's clock use.

## Expensive-to-reverse decisions

- The admission and settlement SQL signatures change: the server binds a grace
  duration instead of `p_window_start`, `p_now` and `p_expires_at`. This is a
  new migration. Statements are internal and have no external callers.
- Budget period selection moves into SQL, so the period of a reservation is
  decided where it is charged.

## Acceptance

- **AC-001.** Postgres integration tests: an admission bound with no timestamp
  is charged to the minute window and budget period containing the statement's
  `statement_timestamp()`. Its lease expires at that instant plus the bound
  grace. A lease whose expiry is at or before a later admission's instant is
  not counted.
- **AC-002.** A Postgres integration test settles a call and observes lapsed
  leases and windows older than the previous minute pruned against settlement
  time. The call's hold correction lands in its admission window.
- **AC-003.** A unit or integration test shows `retry_after_seconds` derived
  from the admission instant and within 1–60.
- **AC-004.** The gateway limit and budget user journeys pass unchanged. The
  limit journey's window-alignment helper may remain, since a fixed calendar
  window still splits two calls that straddle a boundary.
- **AC-005.** `gateway_token_limits_hold_exposure_in_the_admission_window`
  binds no admission time and still proves hold exposure.
- **AC-006.** `mise run check:tenant-isolation` passes for the new migration.

## Open decision

**Which clock records a covered call's `admitted_at`?** Today `admitted_at` is
server time. It is recorded on the call and attempts, and it selects the
pricing version.

- **(Recommended)** Keep server time. It is a domain event time, which §15
  leaves producer-owned, and uncovered calls never reach PostgreSQL. The cost
  is that, within the clock skew of a minute or period boundary, the recorded
  `admitted_at` can fall in a different minute or day than the window or
  period PostgreSQL charged.
- Return the PostgreSQL instant from admission and record it as a covered
  call's `admitted_at`. This keeps records and charges consistent, but covered
  and uncovered calls would then use two different clocks for the same field.

## Authority

- `AGENTS.md` §15, PostgreSQL-owned coordination timestamps.
- `crates/wyrd/wyrd-sql/migrations/20261023000001_gateway_admission_statements.sql`

## Revision history

- Revision 1 (2026-10-22): draft.
