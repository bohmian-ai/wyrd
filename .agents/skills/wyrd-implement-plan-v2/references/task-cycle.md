# Concurrent task cycle

## Worker delivery

```text
ELIGIBLE -> DISPATCHED -> IMPLEMENTING -> VERIFYING -> CANDIDATE_COMMITTED
                                                            |
                                                    INDEPENDENT_REVIEW
                                                            |
             REMEDIATING <----- RESUME_IMPLEMENTATION <----+
                  |                                         |
                  +------------ CANDIDATE_COMMITTED     APPROVE
                                                               |
                                                        ROOT_INTEGRATES
                                                               |
                                                            ACCEPTED
```

The root assigns one packet to one worker and retains that worker for bounded
remediation. The worker applies `wyrd-implement` or `wyrd-ui`, records its
implementation and exact verification evidence in the task packet, leaves the
status `Ready`, audits its diff, and creates a candidate commit in the task
branch. It cannot update plan-level evidence or mark acceptance.

## Review and remediation

For each candidate commit, spawn a fresh read-only `$wyrd-review` parent in
plan-execution binding mode. Give the parent the approved plan, task, candidate
base and commit, task-worktree delta, completion evidence, verification
evidence, and applicable authorities. The parent performs the primary static
conformance/proof review, creates an immutable review-core
`baseline-code-quality` assignment, and spawns exactly one fresh Terra-medium
`code-reviewer` child. The parent validates and deduplicates the child's
specialist-contract candidates, then returns one consolidated binding verdict.
The root does not receive or route the child result directly.

The root validates the consolidated findings against source. Reversible
corrections return to the same worker. Missing private helpers, fixtures,
adapters, local ownership plumbing, and equivalent non-weaker command fixes are
reversible corrections when the owner, bound, lifecycle, and acceptance result
are already fixed. They do not return to planning.

The worker creates a successor candidate commit, reacquires the required
verification token, and the same parent reviewer performs focused re-review
with a fresh code-quality child. A review finding is material only when every
in-scope correction would change a public or persisted contract, dependency or
feature, security or tenancy behavior, data-loss/recovery behavior,
cross-owner authority, allocation bound/owner, or acceptance outcome. Only
those findings revise canonical authority and invalidate affected capsules.

## Material-authority repair

When an implementor, reviewer, integration conflict, or stale baseline exposes
a material choice, freeze the affected task before that choice is implemented.
Allow independent tasks to continue only after proving that the choice cannot
alter their contracts, owners, write boundaries, dependency order, or
acceptance proof.

Require the blocked worker or reviewer to return a decision brief containing:

- the exact missing or conflicting decision and its material classification;
- repository evidence showing why no in-scope implementation preserves the
  approved contract;
- affected plan and task paths, accepted integration SHA, owners, callers, and
  acceptance criteria;
- alternatives already identified and the authority required to resume; and
- any candidate work that must be retained, discarded, or revalidated.

The root validates the brief against current source and canonical authority
before dispatching another agent. Route the result as follows:

The finding itself is not proof of materiality. Before step 1, the root must
name the exact approved material fact that is missing and verify that every
repository-native correction inside the allowed owner/write set would change
that fact. Missing private DTOs, helpers, adapters, signatures, fixtures,
containers, or local call shapes fail this test and return to implementation.
The root must not strengthen acceptance criteria or introduce a cross-owner
redesign to satisfy a review or rehearsal preference.

1. If the finding is local, reversible, or has an in-scope implementation that
   preserves the approved material contract, clarify the capsule and resume
   the original worker. Do not revise canonical material authority.
2. If the finding is genuinely material but existing user intent, Wyrd
   authority, and repository evidence can determine the answer, dispatch one
   fresh Sol-low `$wyrd-advise` agent. Give it the decision brief, affected
   plan and task references, accepted SHA, and normal repository authorities.
   Require one evidence-grounded recommendation, rejected alternatives,
   material risks, and the evidence that would invalidate the recommendation.
   The adviser remains read-only and does not revise plans, assign work, or
   perform a handoff.
3. The root independently verifies every recommendation fact that controls a
   public or persisted contract, dependency, feature, security, tenancy,
   recovery, ownership, or acceptance boundary. The root accepts, rejects, or
   replaces the recommendation, then records the accepted decision through one
   of the canonical-authority routes below.
4. When the accepted decision changes only one affected task or decision and
   leaves the impact graph, dependency order, task decomposition, other task
   contracts, and acceptance outcomes unchanged, the root may make the
   smallest canonical authority correction directly.
5. When the accepted decision changes a public or persisted contract,
   dependency or feature, security or tenancy behavior, data-loss or recovery
   behavior, cross-owner authority, impact graph, dependency order, task
   decomposition, multiple packets, or acceptance outcome, dispatch one fresh
   Sol-low `$wyrd-plan` agent. Give it the root-approved recommendation and
   evidence as fixed input and require a scoped revision of the affected plan
   and task packets, not an open architecture redesign.
6. Run a fresh `$wyrd-cold-rehearsal` for every milestone whose material
   contract, dependency order, or predecessor contract changed. Revalidate
   affected accepted work and redispatch from the current integration head.

Use at most one advisory pass for one blocker. Do not create an
advise-plan-advise loop, let the adviser and planner approve each other, or
manufacture a separate decision ledger. The canonical plan, task packets, and
their rehearsal evidence remain the execution record.

Return to the user instead of selecting a recommendation when the answer
requires new product intent, a choice between incompatible public semantics,
destructive approval, unavailable external authority, or a preference that
accepted Wyrd authority and repository evidence do not determine. Report the
single smallest decision needed to resume.

## Root integration

On `APPROVE`, confirm the reviewed commit is the candidate tip and its tree is
unchanged by handoff. Cherry-pick its commit(s) into the root branch in
dependency order. Audit the integrated diff, update only the accepted task's
canonical status and evidence, and create the root acceptance checkpoint.

If cherry-pick conflict, stale baseline, or previously undiscovered overlap
changes the task's meaning, do not hand-resolve it as an integration detail.
Return the task to the root decision flow, refresh its base from the accepted
head, and redispatch or serialize it.
