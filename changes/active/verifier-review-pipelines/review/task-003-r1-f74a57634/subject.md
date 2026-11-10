# TASK-003 R1 remediation review subject

Repository `/home/thorrester/Documents/GitHub/wyrd-worktrees/code-review-workflow`.
Prior candidate `384a0def73ed6eac4041dc1e428dc30e981514fe`.
Candidate `f74a576348cb9805ec7a2ef0e1c0bde3dd07928d`.
Original task base `e6fe5fbdd1905c6627d6a95bb3042a9cfda4d355`.
Remediation diff `git diff 384a0def73ed6eac4041dc1e428dc30e981514fe f74a576348cb9805ec7a2ef0e1c0bde3dd07928d` (7 commits, 13 files).
Cumulative context `git diff e6fe5fbdd1905c6627d6a95bb3042a9cfda4d355...f74a576348cb9805ec7a2ef0e1c0bde3dd07928d`.
Ancestry is linear, no rebase/patch correspondence issue.

## Authority and immutable inputs

Read the task-review skill, root/nested AGENTS, spec-driven-development,
maintainer style and relevant testing/architecture owners. CodeGraph precedes
code navigation because `.codegraph/` exists. Commands through mise.
Approved baseline r18 plus approved amendments including r22; unrelated TASK007
policy remains draft. Original task: `../../tasks/TASK-003-generic-orchestration.md`.
Prior verdict/ledger: `../task-003-384a0def7/{verdict,findings-validation}.md`.
Remediation: `../task-003-384a0def7/TASK-003-R1-check-admission-and-proof.md`.
Uncommitted authority/evidence bytes pinned in authority-sha256.txt. Prior
implementation has only pre-existing dirty skill files; packets are untracked.
Do not change source, authorities, prior reports, HEAD or other agents' reports.
New directory holds reports only, no diff/source archives. Check HEAD/source and
hashes before conclusion. A changed candidate/authority must be reported.

The current user explicitly reports approved wording clarification: decision-first
ADMIT proves comparison accepted while extra is held, then extra settles admitted;
admission is not recorded before action settlement by the unchanged engine.
DISCARD-EARLY must still observe discarded while extra is running. Invocations
show `none` until disposition is recorded. Treat this as a correction of the
prior proof instruction, not permission to change engine semantics. Verify
against approved behavior/current source. The remediation execution introduction
still names an earlier six-commit endpoint; the subject commits above and current
user instruction unambiguously pin the actual seven-commit review. Do not report
that stale evidence introduction alone as a material code defect.

## Round scope and carried-forward acceptance

First remediation review after one initial FIX_REQUIRED, not a new full review.
Check closure of FIND-003-1..4 plus all 13 changed files and affected callers,
state/permits, tests/fixtures and mandatory rules. Prior behavior, invariant,
standards, maintainer, system, security, concurrency and persistence reports in
../task-003-384a0def7/ are acceptance context. Carry their source-backed unaffected
rows, checking dependencies against this diff. No broad rediscovery. Reopen only
a specific changed/affected path or missing prior evidence, with stated reason.

| Prior finding/affected boundary | Changed owner/callers | Proof/navigation |
|---|---|---|
| FIND-003-1 check admission vs budget/begin | vala-verify engine requirement/judge/admit and recovery counters | waiting_checks_spend_only_after_admission; interrupted_agent_checks_keep_their_invocation; original budget/permit tests |
| FIND-003-2 nested Busy sibling lifetime | instance_requirements -> settled/Tasks; member/drive consumers | capacity_refusal_lets_dispatched_checks_settle; original outer Busy tests |
| FIND-003-3 mandatory rustdoc | engine helpers, validator controlled, shards retained/slice helpers, Rust journey helpers | inspect all prior ten locations plus new changed Rust items; behavior unchanged in doc commit |
| FIND-003-4 public milestone proof | Rust/MCP orchestration journeys; Python conftest/support/test; TS receiver/server/speculative test | held provider replies + current public invocations; comparison-before-extra vs extra-before-comparison; known false, exclusions/read grants retained |
| New fixture owner/reuse and failure cleanup | wyrd-testing gateway_capture HeldUpstream/Holding/forward/start/hold/release/Drop; existing PacedUpstream, wiremock; TS promised reply and teardown | new test-only axum forwarder reused Rust/MCP, installed dependencies only; inspect owner alternatives/full workflows, no general server lifecycle controls |

The map is a starting point, not complete coverage or conclusions. Expand using
source and affected callers. Verification evidence is in R1, not an intended
verdict. Do not treat implementer summary/green checks alone as proof.

## Specialist selection before discovery

| Boundary/role | Decision and source-backed reason |
|---|---|
| Fresh remediation ponytail closure reviewer | Required. Check all four closures, changed paths, new fixture reuse/burden, adjacent behavior/non-goals; carry original unaffected matrix. Own remediation-review.md. |
| domain-concurrency | Selected. Production check admission now spans budget/begin/settlement and nested Busy drain, materially affecting permit ownership, cancellation and durable attempt accounting. Own domain-review-concurrency.md. |
| repo-rev | Selected. All three languages/MCP plus new shared test forwarder and ten rustdoc locations need cross-tier rules/test integrity coverage. Scope to remediation/affected support, not unrelated 93-file audit. Own standards-review.md. |
| Separate invariant/persistence roles | Carry prior invariant/persistence reports for exact accepted state, format/File custody and WAL integrity. No state format/SQL/checkpoint acceptance implementation changes. Closure/domain review inspect the changed invocation producer ordering and preserved genuinely dispatched budgets; no new recovery mechanism requiring full re-review. |
| Separate security/system roles | Carry prior security/system reports for grants/tenant isolation, lease/cancel notification, Scribe failure isolation and terminal ACK/delivery. Those owners are unchanged; engine correction restores the already-approved check lifetime. Changed local fixture forwarding/cleanup is assessed by closure/repo reviewers; no new deployed topology, credential or server failure boundary. |
| Separate maintainer role | Carry prior whole-engine owner/reuse audit. No new production ownership/abstraction. Closure reviewer explicitly compares test-only HeldUpstream with wiremock/PacedUpstream and checks if existing owner extension suffices; 150-line local-provider support is not a new production transport. A source-proven unmet ownership concern can trigger focused specialist evidence. |
| Independent final ponytail validator | Always required after all selected discovery completes, separate fresh agent. Validate closure, role applicability/carry-forward and every proposal, even empty union. Own findings-validation.md. |

No UI/provider-wire/domain-policy changes. No full first-round team recreated.
Validator independently checks every selection/omission; obtain any missing
required report before verdict. No intended verdict is supplied.

## Verification discipline

Only orchestrator runs tests/Cargo sequentially, avoiding shared-target races.
Reviewers request exact selectors if needed; source-proven paths need no new
experiment. Preserve no-per-tenant quota, no limits acceptance snapshot, no
Python/TS worker/lease/replica control, no new durable format/API/dependency.
Remediation permits minimal local-provider hold/release support; do not mistake
that for server fault controls. Ordinary provider-stall/timeout stimuli remain
legitimate. No finding quotas, broad hardening or extra timing matrices.

## Validation-triggered focused scope addition

The fresh validator raised a new narrow applicability concern: Python's explicit
No-global-mutable-state rule versus the remediation's new `support.py::HOLDS`
registry, threaded provider consumer and test producers. Existing Event globals
are not automatically a waiver. A separate fresh followup-rev owns
`followup-review.md` to trace actual lifetime/reset/fixture ownership and decide
source applicability, materiality and smallest correction burden. This is within
new changed test-support paths, not reopening unrelated old Python debt. It was
not validated by its proposing validator. All unaffected prior closure and
carry-forward remain under review; no implementation change is authorized by
this report workflow.
