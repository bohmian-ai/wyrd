# GitHub Verifier pipeline tasks

These nine tasks derive from **approved spec revision 5 and contracts**, approved
by the user on 2026-11-10. They apply the current repository `wyrd-plan` skill.
Revision 4 applies the approved installation storage amendment. Revision 5 uses
GitHub.com’s documented App flow and removes the Enterprise Server compatibility
decision. Task 002’s remediation packet carries that scope through configuration
and proof. Task 004 targets approved spec revision 6 for the user-directed
storage amendment and is complete following the
[final PASS verdict](../review/004-pinned-pr-preparation-r3/verdict.md);
FIND-004-1 and FIND-004-2 are closed. Other tasks retain their existing statuses
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

## Order and actual dependency outputs

| Task | Deliverable consumed downstream |
|---|---|
| [001](001-isolate-queued-capacity.md) | Separate direct/queued execution/workspace/Python/model admission and original ceiling configuration; queued-only validation extended by 005/007/008 |
| [002](002-github-app-connection.md) | Verified tenant installation/selected repository access, installation-token capability and revocation fence for 003/004/006/009 |
| [003](003-repository-declaration.md) | Ready exact Repository receipt, selected policy/bindings and activation lifecycle for 006/007 |
| [004](004-pinned-pr-preparation.md) | **Complete.** Reviewed pinned/baseline-aware ReviewPipeline revision, ObjectStorage adapter and PostgreSQL SnapshotStore/ContextStore metadata tables/adapters for 005 |
| [005](005-pull-request-extraction.md) | Accepted typed extraction and retained exact source/context for ordinary reviewers and 009 rendering |
| [006](006-pr-admission-and-supersession.md) | Durable queued ExecutionIds with frozen policy/source/generation and cancellation fences for 005/007/008/009 |
| [007](007-deferred-evidence-and-resumption.md) | Durable source-matched evidence, waiting inspection and same-execution park/continuation lifecycle for 008/009 |
| [008](008-approval-steps.md) | Exact pending approval identities, accepted decisions and continuation/progress for 009 |
| [009](009-reports-and-github-delivery.md) | Optional standard report/helper authoring and recoverable Checks/comments with independent delivery inspection |

Numbering preserves the agreed nine behavioral pieces, not a linear execution
order. A valid sequence is 001/002, then 003/004, then 006, then 005/007, then 008,
then 009. Only real prerequisite outputs appear in task metadata. In particular,
005 waits for 006's accepted PR activation. 006 can prove admission with a normal
declared queued Verifier; 005 then proves the actual pinned extraction consumer.

Library work is confined to the approved `wyrd-production` branch. The exact
future reviewed committed/published revision is not yet selected. Do not restore
deleted App branches/commits or fabricate an approved historical dependency pin.
External revision availability is an implementation prerequisite for 004, not a
reason to duplicate the pipeline in Wyrd. No external repo changes occur during
this planning work.

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
| REQ-009–011; AC-007 | 007 evidence/checkpoint/queue ACK crash boundaries; 008 decision ACK boundaries, exact recovery/unavailability and no settled computation replay |
| REQ-012–013; AC-008 | 009: retained successful outputs→zero/one/multiple/invalid report→pinned Check/comment/suggestion independent of authored verdict |
| REQ-005,014; AC-009 | 009: provider remote success/timeout/batching/restart/new head/access loss→readback/reconciled independent delivery without recomputation |
| REQ-015; AC-010 | 001 queued saturation→reserved direct execution; 005/007/008 direct queued-only refusal before any step; 006 supplies burst ingress |
| REQ-016; AC-011 | 002/003/005/006/007/008/009 ship their own typed operations, scopes/errors/inspection/docs and journeys; 009 connects the complete reader journey |
| INV-001 | 002 association, 003 identity, 006 accepted authority, 007 source/type/tenant evidence matching, 008 exact approval permission and 009 per-write access |
| INV-002–004 | 003 readiness selection; 006 canonical input ACK→queue commit; 007/008 evidence/decision/pending ACK→continuation recovery; 009 retained progress→owed provider delivery |
| INV-005 | 005 frozen source/baseline/context and 007/008 same-execution accepted state/budget/deadline recovery |
| INV-006 | 006 source generation and cancellation fence→007/008 no stale release→009 no obsolete current delivery |
| INV-007 | All nine reuse the specified owners; only 002 adds wyrd.github_installations and preserves OIDC trust; 002 redacts secrets, 003 keeps keys out of Cards/TOML, 004 preserves library independence |

At seams the downstream proof consumes actual predecessor receipts/accepted
values through production paths. Private fixture shortcuts may support unit tests,
but cannot replace connect→declare→admit→extract or acceptance→park→resume and
retained report→provider reconciliation journeys. Static already-correct behavior
uses source/contract proof rather than needless production changes/tests.

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
