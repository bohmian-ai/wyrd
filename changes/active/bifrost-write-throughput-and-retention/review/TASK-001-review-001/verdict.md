# TASK-001 verdict: SPEC_REVISION_REQUIRED

TASK-001 cannot pass. R1 changes an observable recovery/visibility contract that
still conflicts with the owning architecture. Two independently validated defects
also require correction within already-settled receipt and availability behavior.

## Immutable subject and authority

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/github-verifier-pipelines-task-004`.
- Base: `66f44aee193077ef3565aa76140a942d8eb593f9`.
- Candidate: `bf438a8d7985aadd29db3beb73ed0269c2b2f7ca`, 19 commits.
- Diff: `git diff 66f44aee1...bf438a8d7`; R1 increment:
  `git diff 31af0fe55 bf438a8d7`.
- Specification: [SPEC-bifrost-write-throughput-and-retention revision 1](../../spec.md).
- Original task: [TASK-001-fence-lifecycle.md](../../tasks/TASK-001-fence-lifecycle.md).
- Maintainer remediation: [TASK-001-R1](../TASK-001-maintainer-001/TASK-001-R1-bifrost-only-readers.md).

This is the first independent task review. R1 has no prior independent verdict or
finding IDs to close; it is reviewed as part of the cumulative implementation.
The user's supplied range pins the subject. No source, test, task, specification,
or architecture files were changed during review; only this new review directory
was written. Candidate and source cleanliness were checked throughout review.

Applicable authority: root AGENTS; nested `crates/wyrd/wyrd-sql/AGENTS.md`;
`changes/spec-driven-development.md`; `agent_docs/{index,rust,maintainer-style}.md`;
`TESTING.md`; `benchmarks/AGENTS.md`; relevant tenant/operator rules in
`architecture/wyrd-security-posture.md`; `architecture/bifrost-design.md` acceptance,
recovery and receipt handoff/cleanup; and `architecture/wyrd-design.md` execution
recording/recovery. The reports identify the precise governing sections.

## Independent coverage and assignments

| Reviewer | Boundary and report |
|---|---|
| `behavior_rev` | Primary REQ-001–003, four domain consumers, R1, claim history, adjacent capacity/query-budget behavior, general rules, waits and performance evidence: [task-review-behavior.md](task-review-behavior.md). |
| `invariant_rev` | Stateful WAL ownership, accepted-only replay, whole-batch custody, durable frontier, torn tail and stage retirement, REQ-004–006: [task-review-invariant.md](task-review-invariant.md). |
| `domain_rev` | Persistent SQL/tenancy, cross-pod mirror authority, deletion, sweep/health/admission and recovery readiness, REQ-002/006–009: [task-review-domain.md](task-review-domain.md). |
| `ponytail_rev` | Fresh independent validation of every proposal, coverage/proof strength, authority, specialist applicability, and correction bounds: [findings-validation.md](findings-validation.md). |

The domain reviewer covers failure propagation/startup normally assigned to
`system-rev`; the invariant reviewer covers complex lifecycle ownership. No
additional system reviewer is needed for the same traced paths. No broad cross-tier
public projection, SDK contract, UI, or substantial independent abstraction change
requires a separate repo/maintainer reviewer; the primary retains those applicable
rule checks. This is boundary assignment, not a vote count.

The single reconciled requirement/scenario coverage matrix is incorporated by
reference from [findings-validation.md](findings-validation.md). It maps the existing
REQ-001–009/scenarios to source, assertions, execution evidence and assessments;
it creates no additional obligations. Recovery/frontier acceptance evidence remains
credible; receipt mirror and cleanup-readiness paths fail; domain miss semantics
remain unresolved against authority.

## Contract disposition

R1's latest-readable query and fresh/older-state recovery implement the amended
REQ-003. However, `architecture/bifrost-design.md:733–735` requires unavailable,
never absent, for accepted-but-invisible records. `architecture/wyrd-design.md:2065–2067`
and `:2117–2123` require exact accepted recovery before computation and the fenced
prefix rather than treating an invisible accepted revision as absent.

Fixed batch identities preventing overwrite do not themselves settle that observable
contract difference. R1 still records pending approval, and derivative acceptance
wording retains the prior behavior. The current user summary explains the change
but does not unambiguously approve this architectural boundary change. A focused
clarification was requested; no answer was available when this verdict was written.

Required decision: explicitly settle whether consumers may observe missing accepted
rows and resume from an older readable revision. Then reconcile the approved contract,
owning architecture, task acceptance wording and the stale receipt-bridging module
description in `verification/evidence.rs`. Review does not choose the semantics,
restore receipt reads, or alter architecture to legitimize implementation. This
contract issue takes precedence in the returned verdict over the settled defects below.

## Validated corrections

Findings are recorded once, with source paths, supported triggers, evidence and
validation reasons, in [findings-validation.md](findings-validation.md):

- **FIND-TASK-001-1:** another running pod can acknowledge from its stale mirror
  after the owning pod deletes the backing receipt. Violates REQ-002.
- **FIND-TASK-001-2:** a pending initial cleanup sweep can block recovered server
  boot/readiness. Violates the separation of initial admission health from reads,
  recovery and process readiness in REQ-008–009.

These independently require `FIX_REQUIRED` work within settled behavior. One cohesive
[TASK-001-R2 remediation](TASK-001-R2-mirror-expiry-and-cleanup-readiness.md) addresses
them without deciding the R1 contract. No implementation was launched.

## Verification, performance and process limits

Reviewers inspected actual assertions and committed execution records. R1 records
139 server integration tests, 15 lifecycle journeys, 35 verification-runtime journeys,
41 queue SQL tests and 12 unit tests; earlier task records cover the broader receipt,
recovery and cleaner owners. Review did not rerun Cargo/Postgres, format/lints, or
benchmarks. `git diff --check 66f44aee1...bf438a8d7` passed during orchestration.
The two defects are deterministic source-traced failures independently validated,
not failures reproduced by a new execution in this review.

Added/moved wait coverage is recorded in the primary and specialist reports.
Consumer receipt probes are removed; existing Bifrost reads/ACK waits stay under
their owning contracts. The initial sweep's unsanctioned readiness coupling is
FIND-TASK-001-2; background deletion must remain off append/ACK.

Supported report inspection found VALID diagnostic subset records at `31af0fe55`
with a dirty-tree digest. These are progress evidence, not a clean full benchmark
qualification of `bf438a8d7`; final R1 source correspondence is not established by
those records. The user explicitly reports the hot-read regression remains open
by maintainer decision; it is preserved as a verification limit rather than
manufactured into a newly discovered defect. Full qualification and the 80 input
MiB/server CPU-second target remain the CPU/final-change proof, subject to the
recorded hot-path benchmark preference. This verdict claims no performance PASS.

Behavioral verification and TDD are separate. The task records post-implementation
connected recovery journeys, cleaner health/PG tests and cleanup journey; preserve
those TDD violations. R1 records behavioral RED before its changed reader assertions.
Later mutation tests cannot establish historical RED. No request to redo history or
churn production code is part of remediation.

There have not been two independently unsuccessful remediation rounds; no convergence
escalation applies. TASK-001 remains unaccepted. Review neither commits, merges,
pushes, deploys nor starts TASK-002.
