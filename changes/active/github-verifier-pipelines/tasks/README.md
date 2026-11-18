# GitHub Verifier pipeline tasks

The original nine tasks derive from **approved spec revision 5 and contracts**, approved
by the user on 2026-11-10. They apply the current repository `wyrd-plan` skill.
Revision 4 applies the approved installation storage amendment. Revision 5 uses
GitHub.com’s documented App flow and removes the Enterprise Server compatibility
decision. Task 002’s remediation packet carries that scope through configuration
and proof. Task 003 is complete following its PASS verdict; FIND-003-1 through
FIND-003-8 are closed. Task 004 targets approved spec revision 6 for the user-directed
storage amendment and is complete following the
[final PASS verdict](../review/004-pinned-pr-preparation-r3/verdict.md);
FIND-004-1 and FIND-004-2 are closed. Task 006 and its remediation received the
[PASS verdict](../review/006-pr-admission-r2/verdict.md); FIND-006-1 through
FIND-006-7 remain historically closed, but maintainer acceptance is reopened
pending Task 010. Other tasks retain their existing statuses
and revision 5 basis. Verification evidence lives in the owning tasks and reviews.

The selected path is existing connect/authorization→Card declaration saga→Scribe
and SubmissionSink admission→fair verification queue→same engine and canonical
Bifrost checkpoints→independent provider delivery. Extend complete existing
workflows. Task 002 may add `wyrd.github_installations`; Task 004 additionally
owns the required library snapshot/context metadata tables and PostgreSQL
`SnapshotStore`/`ContextStore` adapters, plus `ObjectStorage` over the configured
object-store backend. No other new PostgreSQL tables or database service,
queue, event bus, general wait framework,
result mirror, UI, private enterprise path or source modification is authorized.
Wyrd owns App keys/signing/callbacks/tenancy; the independent library owns generic
acquisition/comparison/index/context and remains standalone.

Task 011 targets approved spec revision 9 and adds the single docsite tutorial
for the existing REQ-016 / AC-011 journey. It consumes task 009's runnable public
standard-report example, topic docs and connected surface proof. Task 009 retains
those obligations; 011 owns the step-by-step teaching page and its discovery.

The maintainer agreed [shared workload management](../design/shared-workload-management.md)
on 2026-11-16 and requested TASK-012-R1-allocation before TASK-013. The matching
[revision-16 amendment](../spec-revisions/16-shared-workload-management.md) was
explicitly approved on 2026-11-16. The maintainer declared the task complete
on 2026-11-18; scenario 7 records the AC-018 qualification gaps left open. It replaces speculative
verification envelopes/copy estimates, preserves Bifrost's intentional workspace
reservations, and connects shared pressure and non-preemptive CPU admission.

## Order and actual dependency outputs

| Task | Deliverable consumed downstream |
|---|---|
| [001](001-isolate-queued-capacity.md) | **Complete.** Separate direct/queued execution/workspace/Python/model admission and original ceiling configuration; queued-only validation extended by 005/007/008 |
| [002](002-github-app-connection.md) | Verified tenant installation/selected repository access, installation-token capability and revocation fence for 003/004/006/009 |
| [003](003-repository-declaration.md) | **Complete.** Ready exact Repository receipt, selected policy/bindings and activation lifecycle for 006/007 |
| [004](004-pinned-pr-preparation.md) | **Complete.** Reviewed pinned/baseline-aware ReviewPipeline revision, ObjectStorage adapter and PostgreSQL SnapshotStore/ContextStore metadata tables/adapters for 005 |
| [005](005-pull-request-extraction.md) | **Requires 010.** Validated live typed extraction for ordinary reviewers; accepted revisions retain exact source/context for recovery and 009 rendering |
| [006](006-pr-admission-and-supersession.md) | **Historical PASS; acceptance reopened pending 010.** Durable queued ExecutionIds with frozen policy/source/generation and cancellation fences for 005/007/008/009 |
| [007](007-deferred-evidence-and-resumption.md) | **Complete (maintainer approval).** Durable source-matched evidence, waiting inspection and same-execution park/continuation lifecycle (compute released at park; background ACK before retirement) for 008/009 |
| [008](008-approval-steps.md) | **Requires 010.** Exact pending approval identities, accepted decisions and continuation/progress for 009 |
| [009](009-reports-and-github-delivery.md) | **Requires 010.** Optional standard report authoring and recoverable Checks/comments with independent provider retry that never recomputes verification; no new SQL delivery fields |
| [010](010-nonblocking-verification-execution.md) | **Complete.** Approved runtime spec revision 7; shared memory-owned engine, preserved preparation, bounded background terminal retention/lease/group-commit settlement, claim backpressure and resource-scaling proof; keeps Bifrost unchanged and PR/GitHub/Task 004 metadata |
| [010-R1](010-R1-restore-direct-verification-baseline.md) | **Complete remediation.** Maintainer-agreed direct resource-scaling restoration, destination batching and consumer closure; queued-only resumable state on the same engine; full L1 verification/gateway/Bifrost with no regression. L2 measurement is excluded. |
| [011](011-code-review-pipeline-tutorial.md) | **Proposed; requires 009.** Single docsite journey from GitHub connection and repository declaration through review authoring, approval, Check/comment and waiting/delivery diagnosis; reuses 009's public standard-report example |
| [012](012-integrate-code-mode.md) | **Proposed.** Staged port of pinned code mode into B's dynamic pipelines; preserves behavior, applies revision 12's decided nested exclusions and crates.io Monty, carries open obligations and supplies the composed, functionally verified tree for 013 and 014; runs no benchmark |
| [012-R1](012-R1-allocation.md) | **Complete (maintainer approval); requires 012; approved spec revision 16.** AC-018 qualification gaps recorded open in scenario 7. Consumption-based workload manager over gateway/verification/Bifrost and managed workers, preserved Bifrost workspace policy, connected fairness/custody regressions and full clean L1 qualification; required before 013 |
| [012-R2](012-R2-hot-read-optimization.md) | **Proposed; requires 012-R1's existing integrated owners and a specification scope amendment before implementation.** Freeze original UI SQL from a local immutable revision; add L1 `ui-sql` and record its clean baseline before optimizing shared hot execution. UI query rewriting and frontend changes are outside scope. Historical settlement-remediation TASK-012-R2 records remain unchanged. |
| [013](013-mixed-verification-benchmark.md) | **Proposed; requires 012 and 012-R1; approved spec revision 16.** Deterministic 100-tenant Postgres fairness/completion/drain/ledger regression; same-rate direct-only control for existing L2 `competing` on one `combined` server, shared tables and full clean L2 baseline for 014; consumes 012-R1's qualified allocation implementation and L1 records |
| [014](014-direct-verification-throughput.md) | **Proposed; requires 013 (after 012 and 012-R1); approved spec revision 16.** Direct-throughput optimization toward 20,000–30,000 verifications/s and Rust round trips under 2–3 ms; retains c64 overhead target, six-population c1/c64 and matched-rate qualification, scheduler/isolation regressions and inherited diagnosis/ledger rows; compares L1/L2 baselines and re-runs competing/control; consumes 013's 100-tenant test and existing replica-claim proof, without sustained 100-tenant or two-process qualification |

Task 010 targets approved runtime spec revision 7 and must be completed before
renewed 006 acceptance or 005/007–009 execution. Tasks 005 and 007–009 list 010 in
`depends_on` and carry the corrected runtime handoffs: live extraction, background
park/ACK custody and independent provider retry.

The maintainer subsequently agreed the
[benchmark remediation amendment](../design/task-010-benchmark-remediation.md)
and requested 010-R1, with L1 measurement only across all three engines.
010-R1 supersedes conflicting direct-state/inspection and L2 qualification text
in 010. It uses 010's existing implementation rather than requiring 010 to pass
first. Reconcile the agreed amendment in spec/contracts and owning architecture
before affected production edits. This records the user's decision, not an
invented approval of another spec revision or a runtime PASS.

Numbering preserves the original nine behavioral pieces, not a linear execution
order. The original sequence was 001/002, then 003/004, then 006, then 005/007,
then 008/009; 010 now precedes 005/007–009 and renewed 006 acceptance. Only real prerequisite outputs appear in task metadata. In particular,
011 follows 009 and consumes its runnable example and topic docs.
005 waits for 006's accepted PR activation. 006 can prove admission with a normal
declared queued Verifier; 005 then proves the actual pinned extraction consumer.

Library work is confined to the approved `wyrd-production` branch. The exact
future reviewed committed/published revision is not yet selected. Do not restore
deleted App branches/commits or fabricate an approved historical dependency pin.
External revision availability is an implementation prerequisite for 004, not a
reason to duplicate the pipeline in Wyrd. No external repo changes occur during
this planning work.

## Supplemental performance investigations

[TASK-012-R2](012-R2-hot-read-optimization.md) is a separate implementation
proposal, not another profile-only investigation below. Its approved base is
revision 16, but REQ-024's integration-only scope needs an explicit approved
extension before implementation. It records the maintainer-selected independent
engine/benchmark scope and baseline-first checkpoint; it does not claim that
amendment is already approved or reorder 013/014.

These tasks use the integrated shared workload manager on the current branch.
Each can run independently and returns profile evidence and a recommendation
to the maintainer. Run their load windows serially on a shared host. Use the
saved inputs under `benchmarks/history/` and capture new diagnostics on the
current branch. Verify that the requested profiles exist under
`target/benchmarks/` before drawing conclusions.

The requested work is profiling and diagnosis. TASK-012-R1 still owns
REQ-024 and AC-018 qualification and supplies the reviewed input for TASK-013.

| Task | Deliverable |
|---|---|
| [015](015-profile-direct-verification.md) | Locate where canonical Direct Verify latency rises and attribute the Rust and Python c64 tails. |
| [016](016-profile-gateway.md) | Attribute Gateway overhead, payload admission waits, and retained memory. |
| [017](017-profile-bifrost-reads.md) | Attribute the Bifrost long-tail memtable slowdown and Parquet read tails. |

## Obligation and connected proof map

The approved spec remains the obligation ledger; these rows assign its proof,
without expanding it into another checklist. Shared invariants apply wherever
listed in task metadata; their strongest crash/authority proof is identified here.

| Approved obligation | Task and observable proof |
|---|---|
| REQ-001; AC-001 | 002: real verified connect/selection/disconnect, spoof/replay/foreign tenant and revocation |
| REQ-002–003; AC-002 | 003: connection→declare→ready exact graph/selected policy, offline/pins/races/failure/rollback/empty list |
| REQ-004–005; AC-003 | 006: declared binding→41 durably accepted PRs, replay/partial fanout/out-of-order and queued/running supersession; 007/008 prove parked fencing |
| REQ-006–007; AC-004 | 004 public pipeline full/active pinned comparisons→005 actual admitted extraction→same Prompt/workspace values and retained source after recovery/cache expiry |
| REQ-008–009; AC-005 | 007: active binding subscription→real upload/publication→existing/new subscriber scenarios, source/type/auth matching and exact replay/conflict |
| REQ-010–011; AC-006 | 007 independent work/all blocked→resource-free park→evidence continuation; 008 actual pending occurrence→authorized/rejected/expired decision→same execution |
| REQ-009–011; AC-007 | 007 restart while waiting for evidence; 008 restart while waiting for a decision, exact recovery/unavailability and no settled computation replay |
| REQ-012–013; AC-008 | 009: retained successful outputs→zero/one/multiple/invalid report→pinned Check/comment/suggestion independent of authored verdict |
| REQ-005,014; AC-009 | 009: provider remote success/timeout/batching/restart/new head/access loss→readback/reconciled independent delivery without recomputation |
| REQ-015; AC-010 | 001 queued saturation→reserved direct execution; 005/007/008 direct queued-only refusal before any step; 006 supplies burst ingress |
| REQ-016; AC-011 | 002/003/005/006/007/008/009 ship their own typed operations, scopes/errors/inspection/docs and journeys; 009 owns the runnable standard-report example, topic docs and connected runtime journey; 011 adds the single end-to-end tutorial, discovery links and rendered-page reader proof |
| INV-001 | 002 association, 003 identity, 006 accepted authority, 007 source/type/tenant evidence matching, 008 exact approval permission and 009 per-write access |
| INV-002–004 | 003 readiness selection; 006 canonical input ACK→queue commit; 007/008 evidence/decision/pending ACK→continuation recovery; 009 retained progress→independent provider delivery |
| INV-005 | 005 frozen source/baseline/context and 007/008 same-execution accepted state/budget/deadline recovery |
| INV-006 | 006 source generation and cancellation fence→007/008 no stale release→009 no obsolete current delivery |
| INV-007 | All nine reuse the specified owners; only 002 adds wyrd.github_installations and preserves OIDC trust; 002 redacts secrets, 003 keeps keys out of Cards/TOML, 004 preserves library independence |

At seams the downstream proof consumes actual predecessor receipts/accepted
values through production paths. Private fixture shortcuts may support unit tests,
but cannot replace connect→declare→admit→extract or acceptance→park→resume and
retained report→provider reconciliation journeys. Static already-correct behavior
uses source/contract proof rather than needless production changes/tests.

Runtime revision 7 adds REQ-017–018 and AC-012–014. Task 010 owns shared memory
execution, bounded background custody/lease/group settlement, source-preserving
recovery, matched resource-scaling proof and unreleased-schema cleanup. Its
O1–O7 table maps these obligations; 005 and 007–009 consume the corrected runtime.

## Existing verification owners and runners

Each introducing task owns its shipped surface projection and tier-1 story,
including negative flow. Use repository checked-in Card fixtures and public
clients with WyrdTestServer and managed Postgres; mock provider/model transport,
not the Wyrd queue, custody or engine. Test Python/Node lifetimes in those
languages. Read nested AGENTS.md when implementing in SDK/SQL/docs trees.

Existing manifests/runners inspected for this packet:

- Rust SDK package `wyrd-ai`: `mise run test:rust-sdk:journey` manages Postgres,
  builds server and runs its existing journey targets.
- Python/TypeScript integration trees: `mise run py:test:integration` and
  `mise run ts:test:integration` build/sync the respective SDK and server and
  provision Postgres. Use the public thin SDKs; Python exports/stubs and N-API/
  TypeScript projections are part of the introducing task.
- CLI real-server stories extend `wyrd-cli`'s existing `cli` target
  (`tests/cli.rs`); `mise run test:cli:journey` provisions Postgres/migrations
  and invokes that target. Exercise each introduced CLI operation through this
  runner alongside the SDK/MCP stories.
- `wyrd-testing`'s existing `server` target includes `verification_runtime`;
  `mise run test:bifrost:journey:server` owns its managed lifecycle.
- `wyrd-mcp`'s existing `mcp` target includes `verification` and `discovery`;
  `mise run test:bifrost:journey:mcp` owns its managed lifecycle. Assert typed
  operation/catalog/error and domain permission plus MCP write scopes.
- SQL/server `integration` targets retain real-dependency owner tests;
  `mise run test:sql`, `mise run test:cards:integration` and affected family lanes
  own setup. Test membership follows existing targets; no extra test binary is
  implied by a new story/module.

No existing individual test is prescribed by name. Implementers add focused tests
at these owners, confirm actual names/selectors and record their exact focused
commands with the managed environment before RED. A broad lane is not a substitute
for a named focused test. Task-specific owner slices run during implementation;
full shipped-surface lanes and the required broad aggregate belong to integrated
change review, without repeating an aggregate's component lanes.

For changed surfaces, include the needed `mise run codegen:check` after normal
regeneration, `mise run py:format`, `mise run py:lints`, `mise run py:typecheck`,
`mise run ts:format:check`, `mise run ts:lints`, `mise run ts:typecheck` and
`mise run ts:napi:check` as applicable. OpenAPI is runtime: relevant server
integration contract proof uses `mise run test:principals:integration`; MCP
catalog proof uses its owner runner, not codegen. Rust changes run
`mise run fmt`, `mise run lints`; owner/dependency/tenant changes add
`mise run check:deps` and `mise run check:tenant-isolation`. Guides use
`mise run docs:check`. Finish with `git diff --check` and focused diff inspection.
Use these existing owner checks without introducing another verification lane.

The tasks carry approved authority amendments at the implementing owner. They do
not select private helpers, SQL columns/indexes, control-row layout or fixtures.
An actual inability to satisfy the approved custody/authority/no-new-table boundary
must be reported with evidence, not hidden behind fabricated IDs or a new service.

## Comprehension handoff

One independent bounded read-only reader explained the complete flow, every
dependency output, its rationale and connected proof from the current spec,
contracts, relevant design and tasks. The reader found no material contradiction
or unsupported obligation and requested two wording clarifications: distinguish
publisher arguments from internal execution matching, and distinguish explicit
whole-output binding from implicit context injection. Both are repaired in 007
and 005 respectively; the same reader rechecked those paragraphs and confirmed
both gaps resolved, including CLI alongside SDK/MCP proof. This is comprehension
evidence only; runtime feasibility,
external library availability and implementation correctness remain unverified.
No separate review artifact or readiness gate was created.

Revision 5 and TASK-002-R1 received a bounded independent comprehension check.
The reader traced the original finding dispositions and correction/proof
boundaries; subsequent [Ponytail calibration](../review/002-github-app-connection-r1/scope-resolution.md)
reduces retry and historical-evidence work. Its requested clarifications to revision references, packet
requirements and the substituted-code negative proof are incorporated. This
checks the authoring handoff; implementation closure remains pending.

Task 011 and its dependency/traceability additions received a bounded independent
read-only comprehension pass against the current spec, contracts and task packet.
The reader found no material gap or unsupported scope and confirmed that 009
retains the runnable example, topic docs and connected runtime proof. This checks
the tutorial handoff only; the tutorial and its execution proof remain task work.
