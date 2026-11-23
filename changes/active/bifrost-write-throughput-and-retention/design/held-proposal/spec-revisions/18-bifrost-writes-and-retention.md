---
id: SPEC-github-verifier-pipelines
revision: 18
status: draft
execution_hold: true
amends: ../../../../github-verifier-pipelines/spec.md
---

# Supplemental Bifrost write throughput and retention

> HELD HISTORICAL PROPOSAL. Moved from the GitHub Verifier pipelines packet at
> the maintainer's direction. This proposal remains on hold and includes
> superseded scope. It is not the selected design or an implementation input.
> The current design is `changes/active/bifrost-write-throughput-and-retention/design/notes.md`.


The maintainer requested this supplement on 2026-11-20 after selecting the
write-throughput and retention recommendation. This records that proposal for
approval; approved revision 17 remains unchanged. It is not an implementation
approval or a claim that the draft is incorporated into the specification.

Scribe currently completes one group before processing the next, leaving all
write surfaces subject to the same serialized durable work. Its acceptance
fences accumulate permanently. Deleting them at publication would instead lose
retry protection and hide accepted verification and GitHub records from readers
that currently use those fences as permanent acceptance history.

The selected solution extends Scribe's shared group-commit owner and Forge's
existing maintenance protocols. It adds no separate write path, payload store,
retirement table, receipt archive, or per-batch registration. The cost is bounded
overlap in Scribe, migration of acceptance readers, and background deletion and
vacuum work. Batch deduplication becomes finite; evidence retention remains a
separate, explicitly configured policy.

## Proposed obligations

**REQ-025 — Shared write throughput.** Native Arrow, OTLP traces/logs/metrics,
and server outbox writes use the same improved Scribe lifecycle. Overlap group
assembly/append with preceding durable completion while preserving durable ACK,
tenant isolation, duplicate/conflict behavior, fair admission and shutdown or
restart recovery. Preserve asynchronous producer recording and all existing
explicit durable handoffs. No new caller-visible durability wait is introduced.

**REQ-026 — Finite batch acceptance fences.** Batch-id deduplication retains a
one-hour window from PostgreSQL admission time. After that window, an identical
transport batch may be accepted again. Forge removes an aged fence only after
its shared WAL cohort has safely retired, durable publication has settled, and
no unresolved recovery or commit uncertainty requires it. Existing publication
proof remains available until the dependent fences are removed. Fence absence
outside the window is not evidence that a batch was never accepted.

Approvals, deferred evidence, execution revisions, and GitHub delivery recover
accepted state from authoritative retained Bifrost records using their domain
identities, with live fences bridging accepted records not yet query-visible.
Expiry must neither reopen a decided approval nor replay settled computation or
provider delivery. Unavailable recovery remains unavailability. Domain-level
conflict and replay guarantees survive transport deduplication expiry.

Fence cleanup bounds steady-state history to the retry window plus retained
recovery/publication work; it is not a fixed row-count or disk-size guarantee.
Unsettled work retains its safety roots and existing bounded admission/custody
backpressure rather than discarding acknowledged data. Deletion and ordinary
PostgreSQL vacuum must keep up with normal insert churn; deletion alone does
not immediately shrink physical storage.

**REQ-027 — Recoverable independent maintenance.** Forge discovers due work
from existing durable table authority, supersessions, and task evidence after
leadership changes, including tables receiving no new commits. Compaction debt
does not indefinitely prevent snapshot expiration, expired-object cleanup or
never-published orphan cleanup. Preserve exclusive table maintenance authority,
tenant fairness, bounded passes, and uncertain-effect recovery.

Keep snapshot retention at the existing six-hour default and never below the
maximum Oracle query deadline. Keep the existing 24-hour minimum age for
never-published attempt objects. Retained snapshots/references, live or uncertain
operations, unpromoted hot objects, staging, definition roots and publication
evidence remain protected. Neither age nor an object listing alone authorizes
physical deletion. Do not add per-query SQL registrations.

**REQ-028 — Explicit evidence-row retention.** A table without an explicit
retention duration retains its current data indefinitely. An explicitly
configured table policy uses its authoritative time column and one fixed
PostgreSQL-derived cutoff per pass. Rows strictly older than the cutoff are
eligible; rows at or after it survive. Whole expired files may be removed;
files spanning the cutoff must preserve surviving rows. Late-arriving expired
rows become eligible on the next pass that sees them. Records still required
by active workflows or recovery remain protected.

Publish the row-removal table mutation before reclaiming objects, using existing
snapshot retention and reachability safety. Snapshot expiration alone is not
row retention. No universal destructive default is selected for telemetry,
verification, or audit history. Rust server owners implement policy; any
necessary typed configuration, wire/schema and SDK projections stay consistent.

## Scope amendment and fixed constraints

On approval, this amendment extends REQ-024 beyond revision 17's hot-read scope
for the obligations above and reconciles REQ-017's unchanged-fence wording.
INV-001–007, accepted domain-state recovery and other pipeline behavior remain.
Update the owning Bifrost/security/operations contracts before affected
production changes; do not retain contradictory permanent-fence semantics.

The selected implementation approach is in
[TASK-018](../tasks/018-bifrost-write-throughput-and-retention.md). Preserve
existing grouping/fairness and byte limits, WAL compression, 16 MiB/4 MiB
windows, export timer, default shard count and synchronous fence durability.
No new PostgreSQL table, per-fence UPDATE, retry epoch, partitioned deduplication
key, service, dependency, or diagnostic/profiling phase. Necessary unreleased
migration edits are in place; no new migration file, released-history rewrite,
or database reset is authorized. The typed row-policy projection and necessary
existing-schema/index changes are limited to delivering this supplement.

## Acceptance

Task 018's ordered scenarios are the connected proof of REQ-025–028, together
with the applicable existing invariants. They cover shared ingress, restart and
retry, retained domain acceptance after fence cleanup, maintenance takeover,
reader-safe reclamation, and explicit row retention. Existing L1 comparison
qualifies throughput across the supported write populations without weakening
REQ-018's no-regression standard. No numerical speedup is promised and no
diagnostic detour precedes implementation.
