---
id: SPEC-gateway-capacity
revision: 7
status: approved
base_revision: 6
---

# Revision 7 — group source order comes from the Verifier

The maintainer approved this amendment on 2026-10-21 and directed that it ship
in this change. Every obligation in [the specification](spec.md) and revisions
4–6 remains in force.

## Problem

A `group_end` Verifier receives each source slot as a list. Today the list is
ordered by Bifrost's `wyrd_ingested_at`, then by execution ID. Scribe stamps
that time once per committed batch, so executions staged together tie, and the
execution-ID tiebreak is arbitrary: a direct `verify` gets a time-ordered ID,
but a keyed `submit` gets an ID hashed from its idempotency key. A six-turn
support conversation was judged as turns `[1, 5, 2, 3, 4, 6]`. Concurrent
Scribe lanes (REQ-019) can also commit later items first.

Arrival order isn't a guarantee Bifrost or Scribe should make. The order that
matters belongs to the domain, so the Verifier declares it.

## Change

**REQ-020 — Verifier-declared source order.** A Verifier with source slots
may declare `order_by` in one of two forms:

- `recorded_at`: the system key. Orders by the time the server recorded each
  source execution's inputs (`execution_inputs.recorded_at`, stamped per call
  at `verify` start or `submit` acceptance), then execution ID.
- `inputs.<slot>[.<field>|[n]]...`: an input key. It names one of the
  Verifier's own source slots that reads a source input
  (`verifiers.<name>.inputs.<input>`), plus an optional path into that input's
  value.

Example:

```yaml
inputs:
  turns: verifiers.turn-check.inputs.turn
  answered: verifiers.turn-check.requirements.answered.outcome
order_by: inputs.turns.turn
```

Registration refuses `order_by` when:

- the Verifier declares no source slot;
- it is neither `recorded_at` nor an `inputs.` reference;
- it names an undeclared slot;
- it names a slot that isn't a source input;
- it contains a `[*]` wildcard.

A `group_end` binding pins the declaration with its other group settings.
Later Verifier registrations never change a pinned binding.

When the group snapshot is pinned with an input key, every slot reading the
named slot's source binding is ordered by that key:

- numbers first, in ascending numeric order;
- then strings, in ascending order;
- then any other or missing value.

Ties order by `recorded_at`, then execution ID. The snapshot's execution list
follows the same order, so the snapshot digest covers it.

Without `order_by`, lists order by `recorded_at`, then execution ID. Bifrost's
`wyrd_ingested_at` no longer orders lists: Scribe stamps it once per batch.
`recorded_at` is a server clock reading per call. Overlapping calls, retries,
and calls sent through different replicas can be recorded out of call order,
so documentation directs authors who need an exact order to an input key.

**REQ-021 — No commit-order promise.** Scribe and the server outbox make no
cross-batch ordering promise to group readers. Documentation that implies one
is corrected. This satisfies the reader inventory REQ-019/AC-018 requires for
group verification.

## Acceptance

**AC-020.** Spec unit tests prove `order_by`:

- parses and round-trips in both forms;
- validates;
- is refused for each invalid form, under rule `order_by` at path `order_by`.

Server unit tests prove:

- default lists follow `recorded_at`, not ingest time;
- a declared input key orders numbers before strings before missing values,
  with ties stable.

The Rust, Python and TypeScript grouped-support journeys declare
`order_by: inputs.turns.turn` and observe turns 1–6 in order.

**AC-021.** User documentation of group lists describes the declared order and
the `recorded_at` default. The outbox lane rustdoc no longer claims arrival
order.

## Constraints

- This is an additive optional Verifier field and an additive optional field
  of the pinned `group_end` settings. Existing bindings without it decode
  unchanged.
- No new dependency.
- No Bifrost or Scribe ordering mechanism.
- No change to tenancy, authorization or audit.
