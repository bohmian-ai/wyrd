# TASK-003 R1 task-review verdict

**FIX_REQUIRED** — only **FIND-003-5** remains.

**FIND-003-1, FIND-003-2, FIND-003-3 and FIND-003-4 are CLOSED.**

Remediation: [TASK-003-R2 — Python provider hold ownership](TASK-003-R2-python-provider-hold-ownership.md).
Route it directly to `$wyrd-implement`; no spec revision is required.

## Immutable subject and acceptance context

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/code-review-workflow`.
- Prior candidate: `384a0def73ed6eac4041dc1e428dc30e981514fe`.
- Current candidate: `f74a576348cb9805ec7a2ef0e1c0bde3dd07928d`.
- Remediation range: seven commits, 13 files, reviewed with
  `git diff 384a0def73ed6eac4041dc1e428dc30e981514fe f74a576348cb9805ec7a2ef0e1c0bde3dd07928d`.
- Original task base: `e6fe5fbdd1905c6627d6a95bb3042a9cfda4d355`;
  cumulative range remains available. Prior candidate is an ancestor; no rebase
  or patch-correspondence issue exists.
- Authority: [spec](../../spec.md), approved baseline 18 plus approved amendments
  including revision 22; [original task](../../tasks/TASK-003-generic-orchestration.md),
  applicable repository/architecture rules; [original verdict](../task-003-384a0def7/verdict.md),
  [original validated ledger](../task-003-384a0def7/findings-validation.md), and
  [R1 remediation/evidence](../task-003-384a0def7/TASK-003-R1-check-admission-and-proof.md).
  Unrelated TASK-007 decisions remain draft and excluded.
- Uncommitted authority/evidence is pinned in [authority-sha256.txt](authority-sha256.txt).
  The current user explicitly approves correcting decision-first ADMIT proof:
  comparison accepted while extra is held, then extra settles admitted after
  release. DISCARD-EARLY still records discard while extra runs. This matches
  unchanged engine behavior, not a new product contract.

This is the first remediation review. It covers prior-finding closure, the
remediation diff and affected callers/consumers/invariants/tests. It does not
restart the original 93-file review. Unaffected source-backed acceptance is
carried from the prior reports as reconciled below. The stale six-commit endpoint
in the R1 evidence introduction does not override the explicit seven-commit
subject or itself create a material defect.

HEAD and implementation source stayed immutable; all ten pinned authority hashes
were checked. Existing dirty skill edits and untracked packets were preserved.
Only current-round reports and the R2 remediation document were written.

## Reviewers, applicability and follow-up

Each role was a separate fresh agent. The orchestrator did not validate its own
finding. [subject.md](subject.md) records source navigation, scope and role
selection before discovery; [findings-validation.md](findings-validation.md)
independently validates selection, carried evidence and final closure.

| Role / report | Scope and outcome |
|---|---|
| [Remediation closure](remediation-review.md) | All four prior findings, 13-file diff, affected lifetimes, new fixture full-workflow reuse and prior acceptance. PASS; original findings closed, no new proposal. |
| [Concurrency domain](domain-review-concurrency.md) | Admission/permits, durable attempts, Busy sibling settlement and cancellation/deadline ownership. PASS; FIND-003-1/2 closed. |
| [Repository standards](standards-review.md) | Changed Rust/Python/TS/MCP rules, ten rustdoc sites, new helpers and deterministic proof. PASS in reported scope; general Python coverage subsequently qualified by focused FIND-003-5. |
| [Focused follow-up](followup-review.md) | Separate validation of the final validator's new Python global-state rule question. RESOLVED / CONFIRMED as an explicit rule violation; no alleged runtime race. |
| [Fresh final validator](findings-validation.md) | Independently checked every closure, role/carry-forward coverage, source, assertions, burden and focused follow-up. FIX_REQUIRED solely FIND-003-5. |

Concurrency review was required by changed production permit/budget/lifetime
boundaries. Standards review was required by the cross-language fixture and
rustdoc changes. Prior security, system, persistence and invariant reports carry
exact grants/tenancy, canonical state/File custody, format/WAL, Scribe isolation,
terminal ACK/retirement and deployment evidence: those owners are unchanged.
Fresh closure/concurrency review reopens the affected invocation producer and
nested-future lifetime rather than waiving those exceptions. Prior whole-engine
maintainer evidence remains valid; new HeldUpstream receives an independent
complete comparison against wiremock/PacedUpstream. No new production
abstraction, provider protocol, UI, planner or performance boundary requires
another full specialist team. The validator found no missing applicable role.

[Claim comparison](claim-comparison.md) records the initial empty discovery union
and subsequent validator-originated precise Python-rule uncertainty. The new
candidate was not self-validated: a separate fresh reviewer traced its full
producer/consumer/lifetime and applicable rule before the validator retained it.
No conflicting material authority or untraced common cause remains. No rejected
or nonblocking observation creates remediation work.

## Prior-finding closure and reconciled acceptance

| Finding / obligation | Independently established source/assertion proof | Result |
|---|---|---|
| FIND-003-1; R1.1a/b | `engine.rs:1507` admits before spend/begin; judge does not reacquire. Waiting-check regression asserts only slow code check called/recorded before interruption; recovery calls judged exactly once, records attempt 1 and returns Passed under one Agent invocation. Common owner serves whole-Verifier and per-member checks. | CLOSED |
| R1.1c/d: genuine interrupted work and permit order | New interrupted Agent-check characterization asserts its real unsettled attempt remains spent, no recovered call, Inconclusive/BUDGET_SPENT. Run-before-shared order, settled/missing fast paths, accepted begin before external dispatch and permit lifetime through settlement remain. | PASS |
| FIND-003-2; R1.2a/b/c | `instance_requirements` reuses existing Busy-aware settled handling. Controlled-time assertion proves refusal waits for running sibling; accepted checkpoint retains sibling/work settlements and no refused attempt. Recovery calls only refused, preserving one-invocation budget and Passed judgments. | CLOSED |
| FIND-003-3; R1.3 | All ten original panic/precondition/attachment locations corrected against actual bodies, with documentation-only changes. New changed Rust items also inspected for attached docs. | CLOSED |
| FIND-003-4; R1.4a/b/c/d | Rust holds comparison until accepted extra success for action-first; decision-first observes held extra request and accepted comparison, then Started/None for ADMIT or Started/Discarded for exclusion. Final admitted settlement proves corrected ADMIT wording. Known false holds bootstrap until comparison accepted and asserts no extra call. | CLOSED |
| R1.4e/f: retained outcomes and mirrors | Original statuses/calls/manifests/retained/read-only-inspection assertions remain. MCP/Python/TS hold replies against public milestones, retaining all six cases without duplicate full timing matrices or server lifecycle controls. | Proof PASS; Python ownership rule fails separately below |
| R1.5: checks and scope | R1 supplies RED/GREEN for the two lifecycle fixes, characterization/static proof for already-correct behavior/docs and focused journey/check results. Fresh review runs pass as listed below. No dependency, schema, generated contract, migration or production fault-control change. | PASS within stated limits |
| FIND-003-5: new Python mutable global | `support.py:60–63` allocates HOLDS globally; test closures mutate it and threaded completion/orchestration reads it. Python AGENTS:127 prohibits global mutable state without a test exception. Current cleanup can be sound while the ownership rule is violated. | FAIL |

Original source-backed matrices remain acceptance context, with the four prior
qualifications replaced by closure above:

| Carried-forward task group | Prior evidence and dependency check |
|---|---|
| O1/O2 declarations, references/cycles, old forms and readiness | Prior behavior matrix field/rule/no-call and successor-order assertions. Planner/contracts/readiness unchanged; validator edit is rustdoc only. |
| O3/O4 identity, scratch/retries, array order/empty/partial/frozen membership | Prior behavior/invariant matrices' actual call/producer/output/replay assertions. Instance controls, aggregation and workspace owners unchanged. |
| O5/O6 ordinary conditions, joins/strict names, repeat/final iteration | Prior behavior/invariant matrices' condition-before-binding, missing-name, not-applicable/inconclusive, previous/index and final-publication assertions. Shared check lifetime expressly revalidated here. |
| O7/OL speculative finality, exclusion and recovery | Prior deterministic engine/recovery/read-authority evidence survives unchanged drive/Scope/control owners. Required public-order qualification closes here. |
| O9/O-L35/65/66 deadlines, active capacity, tenant progress, renewal | Prior concurrency/system actual bounds, unchanged DB deadline and public cancellation/other-tenant progress evidence; revised shared check producer/lifetime proven above. No tenant reservation promised. |
| Exact File/state/format/WAL integrity, security and Scribe terminal delivery | Prior persistence/security/system matrices trace descriptors, accepted prefix, format refusal, tenant/current grants, conflict isolation, forced writer orders and ACK-before-retirement. Runtime/SQL/auth/store owners unchanged; Scribe edit is docs only. |
| OC/OR/O10/OP cancelled wait, generic document and public projections | Prior exact error/status/details, cancellation race, ordered document/citation and HTTP/CLI/MCP/SDK assertions. No client/catalog/wire changes; provider sibling reach has focused supplied results. |
| Non-goals / reuse | No alternate engine/store, nested graph/streams/quorum, quota/reservation, limits acceptance snapshot or non-Rust server fault-control expansion. O9.5 remains retired by approval, not a runtime PASS. |

The current closure and validator reports contain detailed source/assertion
citations for every carried group and explain why changed dependencies do not
invalidate them. Full original-task rediscovery is not required next round.

## Sole remaining validated ledger entry

**FIND-003-5 — CONFIRMED / VIOLATION.** Discovery: final-validator applicability
question, separately source-validated by `followup-review.md` before retention.

`sdks/wyrd-sdk-python/AGENTS.md:127` says “No global mutable state.” The new
`tests/integration/support.py:60–63` registry is allocated once at import;
`test_verifier_orchestration.py:225–236` inserts/removes arrival/release Events.
`conftest.py` routes serving/Handler through completion to orchestration_turn,
which reads the imported registry at 414–417. Every current speculative case
reaches those producers/consumers. Each receiver owns its request records but
the new gate registry has process lifetime instead of provider-fixture ownership.
This directly violates an explicit package constraint; no test exemption or
user-approved waiver exists. Pre-existing Events are not permission for a new
global or a reason for unrelated cleanup.

The current sequential cases, unique fragments, captured Events and `finally`
release are sound. No runtime flakiness, parallel-test failure, data exposure or
server availability defect is claimed. This is a bounded new R1 standards
violation, not a missed original engine defect or an expanded behavioral promise.

**Selected smallest correction:** remove only the new global registry/imports;
put the gate state on the existing Receiver/local-provider fixture and share
that instance through the current callback/test path. Preserve milestones,
ordinary replies, captured Event wake-up and `finally` releases. Receiver already
owns mutable records and synchronization; serving already captures it. Clearing,
locking or renaming the global does not satisfy the rule; deleting gating loses
required deterministic proof. No new class, harness, API, dependency, concurrency
matrix or other-language work is needed. The self-contained R2 task packages
this diagnosis, owner boundary and exact Python selector/checks.

This is the first unsuccessful remediation review, so the two-unsuccessful-round
convergence rule has not been triggered. The next review is bounded to
FIND-003-5 closure and the changed Python provider paths. Carry all four accepted
closures and the unaffected source-backed task evidence forward; do not reopen
them without new source-backed material evidence.

## Verification, limits and residual observations

Fresh orchestrator execution is in [verification.md](verification.md):

- Seven exact engine regression selectors: **7/7 PASS**, including all three new
  check tests and existing budget/permit/Busy/speculation assertions.
- HeldUpstream-backed Rust public speculative selector with real repository
  Postgres: **1/1 PASS** in 14.487s; both migration-readiness checks passed.
- Remediation `git diff --check`, pinned HEAD/source and all ten authority hashes:
  passed.

R1 supplies exact MCP/Python/TS journey runs, MCP sibling journeys, full TS file
and applicable fmt/lints/Python/TS check results. Reviewers inspect assertions;
those runs were not all independently repeated. Historical TDD/diagnostician
transcript limitations remain disclosed, not recreated as source work. Green
runtime tests cannot establish the static no-global-state requirement.

HeldUpstream is justified narrow test-only forwarding/response gating: wiremock's
current-thread responder cannot synchronously wait without blocking the request
needed for release, and PacedUpstream's fixed streaming scripts do not supply
these request-dependent scripts. No new production owner is introduced. Its
Drop aborts serving, but immediate force-close of active held connections is not
certified; successful flows release holds and failed isolated Rust/MCP tests
rely on runtime teardown. A broad Race comment also overstates ADMIT timing while
the detailed code/proof is correct. These are nonblocking evidence/documentation
limits; no cleanup controller or style-only remediation is prescribed.

No implementation, authority, prior verdict, commit, merge, push or deployment
was changed by this review. TASK-003 remains short of its completion gate only
because of FIND-003-5.

Skipped: broad aggregates, fresh execution of every mirrored journey, and full original discovery.
Risk: the new Python hold registry still violates its package ownership rule; no runtime race is alleged.
