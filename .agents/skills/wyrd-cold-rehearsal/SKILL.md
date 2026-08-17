---
name: wyrd-cold-rehearsal
description: Adversarially prove that a Wyrd task or cohesive milestone has its material contracts, allocation bounds and owners, dependency order, setup, and acceptance proof fixed before implementation. Use during planning readiness or after an implementor exposes a genuine material decision; do not use as a second implementation controller for private mechanics. Supports optional Luna-high specialist fanout for unresolved cross-owner evidence.
---

# Wyrd Cold Rehearsal

Treat the task packet as source code for an implementation assignment. Prove
that a fresh implementor can construct the first production object, complete
the workflow, and run its proof without redesign. A polished description is
not evidence of executability.

A rehearsal is an executability gate, not a private-design gate. It must not
require a particular private DTO, helper, adapter, signature, container,
iterator, fixture, or local call shape when the approved owner, material bound,
lifecycle, dependency direction, and observable acceptance outcome are fixed.
Absence of a private mechanism is not evidence that a material fact is unknown.

## Establish the exact rehearsal target

Require these caller-supplied absolute inputs:

- `REPO_ROOT`;
- `PLAN_PATH`;
- `TASK_PATH` for a single-task material-authority rerun, or the complete
  caller-supplied milestone task-path set for milestone mode;
- accepted source revision;
- accepted predecessor commit or contract revisions.

In milestone mode, resolve and read every packet belonging to the milestone.
Read the target packet set, its parent plan, repository `AGENTS.md`, normal
architecture authorities, applicable implementation skill, manifests, and
verification owners. If `.codegraph/` exists, use CodeGraph before grep or
manual search.
Ignore earlier rehearsal conclusions. Do not accept a rehearsal record embedded
in the task as proof of its own correctness.

Record SHA-256 digests of the task or milestone packets and plan plus the
resolved source and predecessor commits. A later material contract or
predecessor change makes the result stale. Private names, paths, fixtures,
helpers, and equivalent non-weaker command corrections do not invalidate a
milestone rehearsal by themselves.

Distinguish the plan's `Repository revision` (the immutable source snapshot on
which planning began) from the caller-supplied accepted execution revision (the
current serial integration head). An accepted execution revision may advance
after predecessor integration. That is not stale when the original revision is
its ancestor and the supplied predecessor commits/evidence explain the advance;
bind source inspection to the accepted execution revision and report both.

## Keep the rehearsal independent

The rehearsal parent owns the verdict and remains read-only. Do not edit the
task, plan, source, tests, or commands. Report corrections to the planner.

Keep the gate bounded. At the start, record the wall-clock start time with a
read-only clock command. Immediately after binding target identity, send the
checkpoint before beginning the source sweep; do not postpone it until a tool
call or specialist returns. Re-check elapsed time after every tool result and
specialist message. Use these hard phases:

- by minute 2: identity, authorities, first production constructor, and
  predecessor handoff are located;
- by minute 5: every required track has completed one bounded sweep and all
  concrete blockers found so far are recorded; finding one blocker does not
  end an independent track;
- by minute 7: adjacent-impact sweep, verification track, and specialist
  reconciliation stop;
- before minute 8: issue the strict verdict.

Never start a new search, source sweep, or specialist wait after minute 7.
Never wait more than two minutes on one specialist update. If the required
evidence is incomplete at minute 7, return `FAIL` naming the unresolved
construction path instead of continuing. Speed never permits a conditional
pass. The caller should interrupt a parent that has not returned by minute 8;
that interrupted run is `FAIL`, not rehearsal evidence.

Do not return immediately after the first blocker. Record it, continue every
orthogonal track, and use the remaining bounded time to find other defects that
share its owner, constructor, allocation phase, error boundary, or verification
lane. If one blocker makes a later success path unreachable, mark that segment
`UNREACHABLE FROM BLOCKER`, then continue failure, dependency, caller, and
verification traces that do not require inventing its fix.

Use one fresh Luna agent at `high` reasoning as the rehearsal parent. The
parent may spawn up to three Luna-high specialists only when one bounded
unresolved question crosses multiple owners and parallel evidence retrieval is
likely to finish inside the timebox. Do not fan out merely because a task
mentions memory, concurrency, persistence, or multiple crates. Give each
specialist only the task, repository, normal authorities, and one exact track:

1. allocation and lifecycle provenance;
2. compile-shaped interface, caller, and dependency tracing;
3. verification, fixture, environment, and failure-path tracing.

One parent simulation is the default and is sufficient when it can trace all
tracks directly. Specialists provide evidence, not verdicts. The rehearsal
parent reconciles contradictions against source and issues the sole result.

## Simulate implementation, not document review

Walk the task in dependency order and produce all of the following:

1. Resolve the accepted base and every predecessor output. For a task whose
   predecessors are scheduled earlier in the same plan, bind those outputs to
   the predecessor packets and their current PASS rehearsal digests; do not
   require their future symbols to exist at the pre-integration source base.
   For a predecessor already integrated into the accepted source, instead bind
   its resolvable accepted commit, `Complete` task/plan evidence, and current
   source symbols. Classify those seams `BASE-PRESENT`; do not require or invent
   a new cold-rehearsal artifact for completed work. Fail only when the accepted
   commit/evidence is unavailable or the current source contradicts the claimed
   integrated contract.
   If the shared worktree is dirty or its files differ from the accepted
   revision, read accepted source through `git show <accepted>:<path>` (or a
   separate clean read-only worktree). Never use preserved working-tree edits
   as evidence that an accepted symbol is present, absent, or stale.
2. Locate every named owner, caller, consumer, dispatch hop, feature, fixture,
   target, and command.
3. Write the compile-shaped constructor or method call for the first
   production object and each cross-owner handoff. Account for generic bounds,
   visibility, lifetimes, feature gates, and dependency direction.
4. Trace one successful operation from entry to terminal release.
   For every streaming, paged, chunked, batched, retryable, or cursor-driven
   workflow, trace at least two consecutive nonempty iterations plus terminal
   exhaustion. Prove that a capability consumed by iteration one is either
   intentionally terminal or is recreated from an explicit reusable owner
   without reacquisition, copying, growth, or an extra live allocation. A
   one-shot constructor presented as a repeatable stream handoff is `FAIL`.
5. Trace refusal, cancellation, panic/poison, retry, and partial-progress paths.
6. Walk the first test addition and every verification command, including
   repository-managed services, migrations, ignored tests, and CLI forwarding.
7. Identify every remaining choice and classify it. Private helper/type names,
   module organization, local adapters, fixed stores, fixtures, mechanical
   callers, and equivalent non-weaker command corrections are implementation
   mechanics. Public or persisted behavior, dependencies/features,
   security/tenancy, data-loss/recovery, cross-owner authority, acceptance
   outcomes, and genuinely unknown material bounds/owners are material.

Before classifying a finding as material, identify the exact approved fact that
is missing and prove that every repository-native implementation within the
allowed owner/write set would require changing that fact. If one in-scope
private implementation can preserve the owner, bound, lifecycle, dependency
direction, and acceptance result, classify the finding as implementation work.
Never prescribe or infer a cross-owner redesign merely to make a compile-shaped
example easier to state.

## Sweep the blocker neighborhood

Before issuing `FAIL`, cluster findings by root cause rather than stopping at
the first bad line. For each blocker, inspect the bounded neighborhood below:

1. the complete constructor and destructor of the affected owner;
2. sibling allocations, fields, collections, and phase guards created by that
   constructor;
3. every direct production caller and cross-crate handoff named by the packet;
4. exhaustive error variants, public/language projections, and poison paths;
5. retry, cancellation, partial-progress, shutdown, and restart behavior; and
6. dependent packet contracts plus the focused command intended to prove the
   behavior.

Report a compact track matrix with `COMPLETE`, `BLOCKED`, or `UNREACHABLE`
for allocation/lifecycle, interface/callers, failure/recovery, dependencies,
and verification. Required corrections must close the root-cause cluster and
its concrete adjacent manifestations, not merely replace the first token that
failed to compile. Do not invent a hypothetical fix to continue the sweep;
trace only facts and contracts that remain independently observable.

For a prerequisite/foundation task, distinguish its own executable output from
a later dependent task's adoption. A downstream adapter need not exist at the
accepted source and must not be added to the prerequisite's write set. It is
ready when the prerequisite exposes a complete capability contract and the
dependent packet defines a compile-shaped adapter, permitted dependency edge,
owner transfer, release behavior, and proof. Read both packets and rehearse the
handoff packet-to-packet. Fail only when that future adapter is absent,
inconsistent, or requires a material choice; do not fail merely because the
dependent implementation has not run yet.

Apply the same rule in the downstream direction. When rehearsing a task that
depends on not-yet-integrated predecessors:

- read every predecessor packet and its current PASS rehearsal evidence;
- treat the predecessor's normative types, signatures, ownership transfers,
  errors, and tests as the future integration contract;
- use current source to locate the replacement seam, dependency direction,
  callers, and impact graph, but do not report a future predecessor symbol as
  missing merely because execution has not reached that predecessor;
- mark such a symbol `DEPENDENCY-PROVIDED / REVERIFY AFTER INTEGRATION`;
- fail when the predecessor contract or PASS evidence is missing, stale,
  contradictory, not compile-shaped, or insufficient for the downstream call;
  and
- fail when the downstream packet still has to invent a material cross-owner
  argument, production allocation authority/bound, public error, feature edge,
  durable transfer, or acceptance outcome. Local arguments, private guard
  plumbing, helpers, and adapters are implementation work.

Do not apply that future-predecessor rule to work already integrated at the
accepted base. An integrated predecessor is established by a resolvable commit
contained in the accepted revision plus `Complete` plan/task evidence. Read its
current implementation directly and bind it as `BASE-PRESENT`; historical
rehearsal evidence may corroborate acceptance but is not a prerequisite.

The rehearsal report must separate `BASE-PRESENT` seams from
`DEPENDENCY-PROVIDED` seams. Before implementation-plan dispatch, the
controller resolves the material predecessor contract against the newly
integrated commit. Private signature, path, or helper drift is a bounded
implementation adaptation when the owner, invariant, lifecycle, and observable
result are unchanged. Only a material mismatch is `FAIL` and returns to
planning.

Before reporting a signature, arity, type, visibility, or call-shape mismatch,
quote the exact numbered definition and caller, enumerate each parameter and
argument once, and perform a second read of those lines. A finding that cannot
show the mismatch syntactically is invalid. Do not infer duplicate parameters
from adjacent line numbers or similarly named values.

A `FAIL` is evidence for the root controller, not automatic authority to stop
or revise a plan. Each material finding must state the exact missing decision,
why it changes a material contract, and why no permitted private helper or
adapter can close it. Findings lacking that proof are invalid.

Read [readiness-contract.md](references/readiness-contract.md) completely when
the task allocates memory, owns a queue/pool/buffer, crosses a crate boundary,
or claims bounded/fail-safe behavior.

## Enforce allocation provenance

Never accept “caller-provided,” “bounded,” “fixed,” “reserved,” “workspace,”
“pool-owned,” or a named wrapper as proof by itself. For every production
allocation or live collection that can scale materially with user data,
workload, concurrency, retries, or elapsed work, require evidence of:

- facts or a validated configured ceiling available before allocation;
- checked public-API sizing where bytes are derivable;
- cardinality/concurrency and complete simultaneous-live-set bounds;
- the authoritative budget that admits work before material allocation;
- owner and cross-owner transfers while live;
- enforcement that prevents growth beyond the admitted envelope;
- release or explicit durable retention on success, refusal, cancellation,
  panic, retry, shutdown, and restart as applicable; and
- materialized-capacity observation and governing proof.

The plan fixes these material invariants, not the private guard/token type,
constructor signature, collection representation, or local split/transfer
plumbing. An estimate used as authority, hidden workload-scaled copy,
unbounded production collection, or material workspace without a known owner
and ceiling is `FAIL`. Bounded control metadata, telemetry sample storage,
qualification DTOs, paths, report slots, fixtures, and process handles do not
require server-style allocation formulas; verify their cardinality and cleanup
only when relevant to the acceptance outcome. Do not guess memory bounds.

Allocator headers, size-class rounding, control blocks, stacks,
fragmentation, and runtime/library internals that cannot be governed through a
public allocation API may be classified as measured runtime residual only when
a later approved task physically reconciles them under the cgroup and they are
never used as admission credit. Do not demand compiler-private layout formulas
or per-string capabilities as a substitute for that reconciliation.

## Issue a strict verdict

Use exactly one verdict:

- `PASS`: implementation can begin without a material choice; material owners,
  bounds, dependencies, lifecycle, and acceptance proof are fixed. Private
  mechanics may be completed by the implementor.
- `FAIL`: one or more material decisions, ownership/bound gaps, dependency
  conflicts, public/persisted ambiguities, or unproved managed allocations
  remain.
- `STALE`: task, plan, source, or predecessor identity changed.

There is no conditional pass for material authority. Unknown or unverified
material ownership is `FAIL`; source-determined private mechanics are not.

Return a compact report with target identity, verdict, first executable steps,
allocation table when applicable, the track matrix, verified commands,
root-cause-clustered findings with exact evidence, and required packet
corrections. Do not propose implementation code beyond the compile-shaped calls
needed to prove executability.

## Integrate with planning and execution

`$wyrd-plan` runs this skill once for a cohesive milestone after its material
contracts and dependency order are drafted. Rerun only after a material
revision or a genuine implementation authority conflict. Do not require a
separate fresh rehearsal for every private-mechanics-only packet revision.

`$wyrd-implement-plan-v2` consumes the milestone result before dispatch. It
does not rerun rehearsal after implementation starts. A material `FAIL`
returns the affected milestone to planning; bounded private discoveries route
through `$wyrd-implement` and `$wyrd-review`.

Readiness and qualification are surface-scoped. Rust-only evidence cannot
establish Python, TypeScript, Node.js, PyO3, N-API, or another omitted runtime;
an intentionally omitted surface remains unproved rather than blocking the
named milestone.

`plan-readiness-reviewer` validates the current rehearsal evidence and then
performs its independent architecture/readiness review. It does not substitute
the prior prose-oriented rehearsal logic for this gate.
