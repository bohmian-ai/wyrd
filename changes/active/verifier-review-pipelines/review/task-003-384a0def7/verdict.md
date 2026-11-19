# TASK-003 task-review verdict

**FIX_REQUIRED** — **FIND-003-1**, **FIND-003-2**, **FIND-003-3**, **FIND-003-4**.

Remediation: [TASK-003-R1 — Check admission and required proof](TASK-003-R1-check-admission-and-proof.md).
Route it directly to `$wyrd-implement`; no spec revision is required.

## Immutable subject and review scope

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/code-review-workflow`.
- Base, excluded: `e6fe5fbdd1905c6627d6a95bb3042a9cfda4d355`.
- Candidate: `384a0def73ed6eac4041dc1e428dc30e981514fe`.
- First review of all **39** TASK-003 commits, using
  `git diff e6fe5fbdd1905c6627d6a95bb3042a9cfda4d355...384a0def73ed6eac4041dc1e428dc30e981514fe`.
  The user independently confirmed this range and that no other task's commit
  falls within it. The Scribe shared-owner fix is included; the final four
  revision-22 commits do not replace the full first-review scope.
- Authority: [spec](../../spec.md) approved baseline revision 18 and subsequent
  approved amendments, including revision 22; [original TASK-003](../../tasks/TASK-003-generic-orchestration.md),
  publication/durability/public contracts and applicable repository authority.
  Unrelated TASK-007 policy remains draft and is excluded from this audit.
- Spec/task packet is uncommitted and pinned by [authority SHA-256 fingerprints](authority-sha256.txt).
  The full subject, initial source navigation and role-selection record are in
  [subject.md](subject.md). Implementation source remained at the candidate;
  pre-existing skill edits and the unrelated untracked packet were not changed.

No prior TASK-003 verdict, findings or acceptance were carried forward. This is
not a remediation round; no convergence assessment is required. Existing
foundation/task-002 context was inspected only where this task consumes it.
The original task's IMPLEMENTED declaration and fresh-checker summaries were
claims to audit, not independent task acceptance.

## Independent review and specialist coverage

The orchestrator routed the immutable subject and wrote this verdict from the
fresh validator's ledger; it did not validate its own findings. Every role was a
separate fresh agent, independent of implementation and other discovery agents.
All eight selected discovery reports were complete before fresh validation.

| Role/report | Required boundary and result |
|---|---|
| [Behavior](task-review-behavior.md) | Complete obligation/non-goal matrix and caller-to-result paths. FAIL; check budget before admission. |
| [Invariants](task-review-invariants.md) | Producer-to-checkpoint-to-recovery identity, membership, iteration/disposition and budget custody. FAIL; same check-budget source. |
| [Repository standards](standards-review.md) | All 93 changed files/languages and applicable nested rules, generated shapes, real SQL capabilities, docs/tests/rustdoc. FAIL; mandatory rustdoc and public ordering proof. |
| [Maintainer](maintainer-review.md) | BASE-to-candidate whole-workflow reuse and owner/readability audit. FAIL; consolidated rustdoc gate. No duplicate engine/runtime/store established. |
| [System resilience](system-review.md) | Shared Scribe/outbox failure propagation, WAL/ACK ownership, cancellation, replacement and unrelated progress. PASS in scope. |
| [Security/tenancy](domain-review-security.md) | Current grants, tenant capabilities, retained/excluded evidence, workspace reads and notification authority. PASS in scope. |
| [Concurrency](domain-review-concurrency.md) | Permit/task ownership, admission/Busy/check accounting and cancellation release. FAIL; waiting-check budget plus nested sibling drop. |
| [Persistent integrity](domain-review-persistence.md) | Control format, exact accepted prefix/row, revision identity, custody, retained contradictions and replay. PASS in scope. Added before validation after applicability audit. |
| [Independent Ponytail validation](findings-validation.md) | Independently checked all role selection/omissions, reports, actual source, materiality and minimum corrections. FIX_REQUIRED; four retained groups. |

Persistence required a separate sensitive-domain review even without a new
store; it was obtained before validation. No UI or provider-wire specialist was
needed because those boundaries do not change. No separate database-performance
review was needed: no new broad scan, query planner or performance contract is
introduced. Persistence, concurrency, security and system reports cover the
changed data/coordination boundaries. The validator independently accepted
these omissions and found no missing required report.

The existing walker/engine/Scope and checkpoint lifecycle were extended for
required readiness, fanout, repeat and speculation. The former stage barrier is
removed rather than retained as a competing owner. The Scribe correction stays
on the existing grouped-write and retained-identity owners. No unearned graph
store, scheduler, checkpoint store or replacement Agent runtime was established.
Task mechanics were checked against stronger authority; they do not justify
new tenant quotas or SDK fault controls.

## Claim comparison and follow-up

[Claim comparison](claim-comparison.md) groups proposals by violated invariant,
not report count or location. BEH-003-1/INV-003-1/CONC-002 form one budget finding;
MAINT-003-01/STD-003-1 form one documentation finding. CONC-001 is a distinct
nested future-lifetime defect. STD-003-2 is retained with a narrower, source-backed
proof correction. Agreement is corroboration only; all proposals were validated
against actual producer/consumer bodies and sibling owners.

No focused discovery follow-up was required: no material authority conflict or
untraced common cause remained. Unique proposals were independently validated,
not rejected or rediscovered merely because only one reviewer proposed them.
The validator discovered no new self-validated blocker. No required specialist
or report is unavailable.

## Validated finding ledger

The full exact source trace, consequences, burden and closure proof for every
finding are in [findings-validation.md](findings-validation.md). This is the
final deduplicated ledger; no rejected extension creates remediation work.

| Finding | Status/classification and source | Violated behavior and minimum correction |
|---|---|---|
| **FIND-003-1** | CONFIRMED / INCORRECT. BEH-003-1, INV-003-1, CONC-002. `engine.rs:1490–1514`, downstream `judge:1660`; recovery `:370–390,460–484,719–731`. | Waiting Agent checks spend and persist attempts before capacity admission. Restart can exhaust a check's only invocation without an Agent call. Admit once before spend/begin at the shared requirement boundary; remove later duplicate acquisition. Preserve real interrupted work accounting and accepted begin before dispatch. No refund protocol or new state field. |
| **FIND-003-2** | CONFIRMED / INCORRECT. CONC-001. `engine.rs:1422–1449`, especially `:1441–1444`; `walk.rs:95–165`; outer owners `engine.rs:970–971,1268–1270`. | Busy in one per-member requirement drops already-dispatched sibling checks. Recovery repeats/charges work whose own admission succeeded. Reuse existing Busy-aware settlement/draining at the inner owner before propagating refusal. No new task registry or retry policy. |
| **FIND-003-3** | CONFIRMED / VIOLATION. MAINT-003-01, STD-003-1. Engine private index helpers, validator `controlled`, Scribe docs `shards.rs:4472–4500`, Rust journey `section_turns`/`mapped`. | Explicit changed-item rustdoc gate remains open; Scribe's slice-pop docs attach to the wrong method. Restore attachment and document actual panic preconditions in the listed helpers. One documentation batch, no defensive code or style-only round. |
| **FIND-003-4** | REVISED / MISSING–VIOLATION. STD-003-2. Rust journey `:89–94,237–273,1103–1251` and mirrored local-provider timing fixtures. | Required public action/guard order is inferred from delays; identical ADMIT assertions can pass in the same order twice. Establish accepted milestones with explicit local-provider hold/release and current public invocation inspection. Preserve mirror case/contract coverage; do not add duplicate complete language timing matrices or server lifecycle controls. Small fixture-only async support may be needed. |

No finding requires changing approved product behavior, persistent format,
permissions, tenancy, coordination ownership or public contracts. Remediation
selects existing owners, not speculative new machinery. A new scope/authority
conflict discovered during implementation must still be reported rather than
silently designed.

## Reconciled acceptance matrix

The [behavior matrix](task-review-behavior.md#acceptance-matrix) supplies the
complete O-ledger and added obligation mapping, with implementation and actual
assertion citations. Invariant/security/persistence/system matrices supply the
cross-owner proof. The reconciliation below overrides broad PASS rows where a
specific validated counterexample or missing mandatory proof exists; unchanged
rows retain source-backed acceptance context for the next bounded review.

| Obligation/constraint | Reconciled implementation and proof assessment | Result |
|---|---|---|
| O1.1–1.6: controls, references/cycles, old declarations | Pure validator/planner, schema/reference and compatibility assertions in behavior matrix. | PASS |
| O2.1–2.4: readiness/any-success/ordinary evidence | Existing engine/ledger release after individual complete settlements; actual ordering/no-call assertions inspected. | PASS |
| O3.1–3.5, O4.1–4.7: fanout identity, scratch, retries, frozen arrays/order/empty/partial states | Engine/Scope and real SDK fixture assertions establish these outcomes, including exact recovered membership. | PASS |
| O5.1–5.5, O-L33/O-L136: conditions, honest joins/names and inspection eligibility | Source/SDK/engine assertions establish refusal/skips/partial collections and read-only excluded inspection. | PASS |
| O6.1–6.8, O-L264: repeat sequencing/final selection/identity | Accepted action/check/repeat state and deterministic assertions establish core sequencing/recovery; nested check admission/Busy lifetime remains qualified by FIND-003-1/2. | Core behavior PASS; check-lifecycle qualifications below |
| O7.1–7.8: speculative behavior/finality and both-order proof | Deterministic engine semantics and retained/read/recovery proofs remain credible. Required public both-order proof and related broad O7.5/O-L274 claims are incomplete under FIND-003-4. | FAIL for required public proof |
| O9.1, O-L35: capacity ceilings, wait permit ownership, Busy preservation | Active shared/execution/fanout ceilings and run-before-shared order pass; inner requirement Busy handling loses dispatched siblings under FIND-003-2. | FAIL for nested Busy preservation |
| O9.2, O-L65, OL: budget/attempt conservation including waiting checks | Actual dispatched/retried work accounting passes its tests. Waiting checks create invalid durable spend under FIND-003-1; outer-drain tests do not cover FIND-003-2. | FAIL |
| O9.3/O9.4/O-L66: original deadlines, cancellation release, tenant progress after release, renewal | Source and inspected lifecycle/admission assertions pass within approved shared capacity, without reserved tenant capacity. Check-budget claim is qualified as above. | PASS within corrected scope |
| O9.5: acceptance-time limits snapshot | Explicitly retired by approved revision 22. Existing enforcement remains required and covered; no new response field is needed. | RETIRED, not runtime PASS |
| O10: generic document workflow | Ordered array/repeat/publication/citation/verdict proof; unflagged skipped escalation is proven in Rust. No claim of that specific unflagged variant on every language. | PASS |
| OL/OL.P/O-L158: exact control-state recovery, format refusal, current permissions | Existing Rust real-server takeover plus checkpoint/permission proofs establish custody and current authority; no language fault-injection controls are required. Does not waive check-budget producer/lifetime findings. | Core recovery PASS; FIND-003-1/2 qualifications |
| OC: cancelled wait, public error/status projection | Shared-client/catalog implementation; exact Rust/Python/TS assertions inspected. Fresh Rust SDK journey passes; stored cancelled/no-result and unknown-code/timeout paths remain. | PASS |
| OR: competing cancellation writers, accepted reports, ACK/retirement and same-lane bystander | Both controlled arrival orders use actual acceptance boundaries. Fresh exact real-Postgres race passes; source confirms retained report, cancelled status/no result and unrelated completion. | PASS |
| OP/O-L274: shipped public controls/status/manifests, strict missing name, cancel, HTTP/CLI/MCP | Existing public contract/case evidence passes; public speculative timing proof remains FIND-003-4. No new per-language recovery or exhaustive timing matrix is introduced. | Contract coverage PASS; timing proof FAIL |
| INV-017–019/027, exact File custody, Scribe/WAL durability and security | Independent domain/system/invariant source traces establish exact accepted state, conflict isolation, rejection/replay, File descriptors, tenant/grant boundaries and ACK before retirement. No new contrary proposal. | PASS with invalid check-producer/lifetime exceptions above |
| Root changed-item rustdoc | Missing/misattributed documentation is explicitly merge-blocking; green lints do not waive it. FIND-003-3. | FAIL |
| Non-goals: no alternate graph/runtime/store, nested graphs, streams/subscriptions, quorum, tenant quota/reservation, acceptance limits snapshot, SDK server fault controls | Complete range and source/reuse audits establish exclusion. Corrections preserve exclusions. | EXCLUDED |

## Verification and evidence limits

[verification.md](verification.md) records commands executed sequentially by the
orchestrator against this candidate:

- `mise exec -- cargo nextest run --locked -p vala-verify -p wyrd-spec --lib`:
  **834 run, 834 passed**.
- Exact real-Postgres cancellation-writer race selector: **1 run, 1 passed**,
  with both repository migration-readiness checks passing.
- Exact real-Postgres Rust SDK conditions/collections/cancelled-wait journey:
  **1 run, 1 passed**, with migration readiness passing.
- Cumulative `git diff --check`, authority fingerprints and pinned source/HEAD:
  passed.

These runs establish only their covered assertions. They do not exercise the
waiting-check or nested Busy counterexamples, repair rustdoc or observe the
required public order pair. Discovery/validation were static source/diff and
assertion audits; nobody claimed those missing cases ran or failed during review.
The focused remediation regressions are planned, not existing proof.

All eleven owner checks, docs generation/command/link checks, and other language,
MCP and recovery journeys have supplied results in the original task. Reviewers
inspected relevant assertions/source and generated projection shapes; the full
SDK/platform/provider/credentialed aggregates were not rerun. Historical RED and
diagnostician transcripts were not independently available. The original task
openly records production-before-tests and projection/TDD exceptions; do not
claim those satisfy historical RED or manufacture source changes to rewrite
that history. These are disclosed limits, not additional unbounded remediation.

No implementation/spec/task source, commit, merge, push or deployment was changed.
Only reports and the single remediation packet were written in this review
directory. TASK-003 does not pass its completion gate until independently
reviewed closure of the four findings.

Skipped: full platform/provider/language aggregates and new runtime reproduction of source-proven gaps.
Remaining risks: early durable check spend, dropped sibling checks, incomplete public order proof, and the mandatory rustdoc gate remain open.
