---
name: wyrd-review-and-plan-v3
description: Run the terminal static review of an immutable integrated Wyrd change against its intent, plan, tasks, claims, code, and available evidence. Use after integrated verification as the final merge- or push-readiness review; no execution controller artifacts are required.
---

# Wyrd Review And Plan v3

Review one immutable integrated base-to-target range for aggregate intent and
plan closure, cross-task correctness, architecture and contract drift,
integrated journeys, and merge/push readiness. This is not a per-task candidate
review and never reviews a mutable working tree.

Remain read-only. Do not implement fixes or mutate reviewed source. Runtime
verification normally belongs to the calling execution environment; audit its
available evidence and run only a narrowly necessary check when the caller
explicitly authorizes runtime verification. State static and runtime limits
plainly.

## Required input

Require only:

- repository root;
- immutable base and integrated target SHAs with an unambiguous range; and
- the user intent plus available plan/task authority.

Plan reviews, task reviews, command results, evidence summaries, Change IDs,
Change revision IDs, and artifact digests are useful optional evidence. Missing
controller request IDs, generations, candidate manifests, integration commits,
proof artifacts, leases, or controller state never blocks review.

When a Wyrd `ChangeRevisionId` is supplied, confirm its base/candidate identity
matches the reviewed integrated subject and that it is not stale or superseded.
Review acceptance remains an evidence contribution: it does not transition a
Wyrd `Change`, finalize an `EvidenceManifest`, authorize, merge, or promote it.

Read at the target snapshot:

- `AGENTS.md`, `architecture/agent-rules.md`, and applicable design/doctrine;
- intent, plan, and relevant task packets;
- the complete committed diff;
- enough unchanged owners, callers, consumers, contracts, tests, and generated
  surfaces to evaluate the integrated impact; and
- available verification and prior review evidence.

Follow CodeGraph instructions. Use `git show` or a detached read-only worktree
when the index does not match the target.

## Establish proportionate coverage

Build a compact impact map from the integrated diff: changed behavior owners,
public/durable contracts, state and side-effect transitions, consumers,
generated projections, tests and journeys, deployment surfaces, and applicable
release gates.

Always cover:

1. aggregate user intent and required claim closure;
2. cross-task producer/consumer seams and integration drift;
3. candidate-introduced behavioral regressions;
4. applicable Wyrd architecture, ownership, and hard repository rules;
5. evidence adequacy, staleness, and contradiction; and
6. user/agent journey closure for changed public behavior.

Add focused specialist review only when it materially reduces risk. High-risk
auth, tenancy, audit, destructive persistence, migrations, public contracts,
cross-language boundaries, concurrency/recovery, Vala/Bifrost data-plane, or UI
changes may justify independent lenses. Do not require a fixed reviewer roster,
artifact tree, dispatch schema, or reviewer count for every change. The root
reviewer validates adopted findings against source and owns the final verdict.

Use accepted candidate reviews as evidence rather than mechanically repeating
them. Reopen candidate-local work only when an integrated seam, later change,
stale subject, missing consumer closure, or evidence inconsistency makes the
prior conclusion suspect.

## Review claims and evidence

Map required plan/task claims to the strongest applicable evidence. Preserve
the Wyrd Change trust distinctions when present:

- deterministic evidence, LLM evaluation, and human attestation remain
  orthogonal;
- finalized evidence completeness does not itself prove claim success;
- failed or missing evidence is failed or indeterminate, never success;
- evidence from a different, stale, or superseded subject does not transfer;
  and
- review acceptance does not imply product authorization or merge authority.

Broad integrated checks support, but do not replace, inspection of whether
their assertions prove the relevant behavior. Conversely, do not rerun broad
suites merely to duplicate valid current evidence.

## Finding threshold

Report only confirmed defects that affect integrated behavior, a required
claim, a public/durable contract, cross-task closure, verification credibility,
or an applicable hard repository rule. Reject speculative, preference-only,
duplicate, stale, or unrelated findings.

A finding contains:

- stable ID, severity, and exact location;
- affected claim, task, or hard rule;
- current versus expected integrated behavior;
- reachable scenario and observable consequence;
- bounded required outcome and non-goals; and
- focused verification that would establish closure.

Classify findings as:

- `REVERSIBLE` — intent already determines a bounded code, test, generated,
  documentation, or evidence correction;
- `TASK_REPAIR` — task authority is mechanically contradictory or unexecutable
  without a new material decision; or
- `MATERIAL` — correction needs new product, public/durable contract,
  ownership, security, tenancy, migration, rollout, or acceptance authority.

## Verdict and remediation

Use one verdict:

- `CLEAN` — no confirmed findings; required integrated claims and journeys are
  adequately supported; ready for the caller's merge/push decision;
- `REMEDIATION_REQUIRED` — bounded reversible or task-repair work remains;
- `MATERIAL_DECISION_REQUIRED` — new material authority is required; or
- `REVIEW_BLOCKED` — the immutable target, essential authority, or critical
  evidence cannot be inspected sufficiently.

Lead with the verdict and confirmed findings. Then summarize reviewed identity,
impact coverage, claim/evidence closure, verification/static limits, and
merge/push readiness. Do not require a durable review directory or exact YAML
schema unless the caller requests an automation artifact.

Return reversible findings directly to the caller for bounded
`$wyrd-implement-v3` remediation. A repaired immutable candidate receives
focused `$wyrd-review-v3`, integration, applicable integrated checks, and a
fresh terminal review. Route only `MATERIAL` findings to `$wyrd-plan-v3` or the
user. No repository skill owns scheduling, leases, generations, worktrees,
candidate refs, serial integration, or remediation-controller state.
