# TASK-001 review subject and coverage

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/code-review-workflow`.
- Base: `0e8a31b14d336fadf62e1d8525a7fa030ab6c4eb` (`a9f0b6261^`).
- Candidate: `cd905f4cd929785bda25696326d58b4a4ae88dee`.
- Range: `git diff 0e8a31b14d336fadf62e1d8525a7fa030ab6c4eb...cd905f4cd929785bda25696326d58b4a4ae88dee`.
- Complete first acceptance audit of the replacement Bifrost-state TASK-001 under approved spec revision 18, durability supplement revision 2, including TASK-001 Revision 1. All 60 changed files are in scope; the final commit alone is not the subject.
- Source must remain immutable. HEAD equals candidate; no working source changes at establishment. Architecture amendments and the active packet are intentionally uncommitted inputs. Do not change source, authority, tasks, evidence, or other reviewer reports. Write only the assigned report in this directory.
- Approved spec: `changes/active/verifier-review-pipelines/spec.md`.
- Task: `changes/active/verifier-review-pipelines/tasks/TASK-001-bifrost-execution-state.md`, original scenarios 1–8 plus Revision 1 scenarios R1.1–R1.3.
- Supplement: `changes/active/verifier-review-pipelines/execution-state-and-durability.md`.
- Report directory: `changes/active/verifier-review-pipelines/review/task-001-bifrost-cd905f4cd/`.

The older `review/task-001-r1-365bfc94b/` concerns spec revision 16 and another candidate. Preserve it as historical context. This audit carries forward no acceptance rows from it: the replacement contract, migration and state owners materially changed. Original invariant preservation remains task scope; this is not an automatic re-review of every historical issue. Historical stable IDs FIND-TASK-001-1 through FIND-TASK-001-15 are reserved. Validator must preserve an old ID only for the same cause; assign new findings starting at 16. Revision 1 is human-directed scope, not an independently discovered finding.

## Authority and navigation map

Read applicable AGENTS, `changes/spec-driven-development.md`, `agent_docs/maintainer-style.md`, `agent_docs/rust.md`, `TESTING.md`, and the narrow owner sections. Nested rules for touched files: `crates/wyrd/wyrd-sql/AGENTS.md`, `docs/AGENTS.md`, `sdks/wyrd-sdk-ts/AGENTS.md`. Root `AGENTS.md` governs remaining touched directories. Generated sources require generator parity, not hand edits.

| Contract / owner | Producers, consumers and starting source | Relevant proof / authority |
| --- | --- | --- |
| Engine memory, invocation starts/settlements and recovery | `vala-verify` engine/ledger/checkpoint interface → server `verification/state.rs::RunState`, `Held`, `Pending`, `StateScribe` → runner and hosted blocks/evidence/publications/workspace | Task 1/4/5, R1.1–R1.3; supplement DUR-001–005/008; `pg_durable_verifier_work`, engine checkpoint test |
| Bounded checkpoint delivery, cancellation and receipts | `RunState::change/durable`, shared pending future → `scribe_outbox.rs::ScribeWrite/ScribeSink::lane/frames/tally/write` → local and peer Scribe; shared `wyrd-runtime::Outbox` remains dependency owner | R1; reserved capacity, byte accounting, per-frame receipt, exact coverage; state/outbox unit tests |
| Durable writer identity, next revision, rejection and replay | `ScribeStateAdvance`/peer proto → Scribe ingress/preprocess/shards/replay → `vala-sql::scribe_batch_commits/scribe_state_heads`; new state-head migration | Task 2/3, DUR-006/009/010; Bifrost durability/acceptance authority; `pg_scribe_batch_commits`, Scribe lifecycle recovery/takeover tests |
| Complete state format and exact recovery | `wyrd-spec::verification::state::ExecutionState` → result builder checkpoint row → `InputResolver::state`, `RunState::claim/read/accepted` | Task 1/2/8; spec REQ-052–058, INV-017–019; exact recovery / compatible format proofs |
| Direct, queued and terminal delivery | verification service direct entry, runner execute/dispatch/result/deliver, SQL queue claim/renew/complete, WORKER_FORMAT and SQL payload removal migration | Task 1/4/6/7/8; Wyrd design verification execution section; operations restart/lifecycle; SDK verify/submit/read journey and runtime tests |
| Public inspection and File custody | service inspection/auth → accepted state → page cuts/metadata/publication selected content → exact File descriptor storage reads; deleted SQL page/publication owners | Task 5/6; security authorization/tenant binding/audit; publication, paging, download and cross-tenant supported callers |
| Cross-tier contracts and standards | new stable StateRefused error, proto state fence, generated TS error-code/docs projections, SQL capability signatures, added/changed helper/test rustdoc, Cargo dependency change | Root/nested AGENTS, errors/agent-surfaces guides, codegen generators, SQL RLS/transaction rules |
| Sibling workflows | groups/bindings/operators/submissions, source deletion, admission idempotency, cancellation and original deadlines | Task 7/8; unchanged dependencies need targeted consumer tracing when source owners changed |

This map is a starting point, not complete coverage or an intended conclusion. Follow CodeGraph before source searches, expand from actual diff and callers, and compare whole workflows with existing owners rather than only leaf calls.

## Specialist applicability and assigned reports

| Role | Material contract / invariant and source-backed selection | Report |
| --- | --- | --- |
| behavior-rev | Required first complete task acceptance audit; service/runner/state/inspection/SDK paths and all original plus R1 obligations | `task-review-behavior.md` |
| invariant-rev | Shared live/accepted/pending state, ticket coverage, invocation/publication/final lifecycles span multiple consumers (`state.rs`, runner, hosted adapters) | `task-review-invariants.md` |
| repo-rev | 60 files across Rust/spec/server/SQL/proto/docs/TS generation, mandatory rustdoc and SQL capability rules | `standards-review.md` |
| maintainer-rev | SQL execution state removal, new RunState and StateScribe ownership, acceptance/fence extensions, host adapter replacement; test whole-workflow reuse | `maintainer-review.md` |
| system-rev | Direct now has durable checkpoint boundaries, worker-format migration, outage/timeouts/cancellation/retained-volume restart, shared outbox failure propagation | `system-review.md` |
| domain-concurrency | Shared futures, caller-driven checkpoint progress, lease-confirm/freezing gap, cancellation/takeover and per-frame receipts change supported concurrency | `domain-review-concurrency.md` |
| domain-data | New SQL state heads/RLS and payload-removal migration; WAL/batch-fence transaction atomicity, replay, exact accepted-state recovery and page stability | `domain-review-data.md` |
| domain-security | Rewritten execution reads, internal Bifrost capability, exact File metadata, changed direct/queued state path: permissions, revocation, tenant isolation and audit must survive | `domain-review-security.md` |

All applicable specialties above are selected. No separate provider/OIDC/UI/Python reviewer: no provider or token-profile behavior, UI workflow, or Python native lifetime changes occur in this range. Repo reviewer checks generated TS parity; behavior/security reviewers still trace affected public projections and existing journey evidence. Domain separation is by boundary, not duplicate comprehensive passes.

## Evidence and review instructions

Available verification is recorded in the task's historical and Revision 1 implementation-evidence sections, with commands/selectors and explicit gaps. Treat claims as claims; inspect tests and results actually available. Do not consume the caller's prose completion summary as proof. The orchestrator may independently run narrow tests sequentially and record results in `verification.md`; do not run competing Cargo commands. Request a focused run if needed. Do not write a test or modify production to prove a finding.

Every discovery report needs assigned coverage, source-backed acceptance/rule matrix as applicable, material proposed findings with local IDs, reachable producer-to-consumer scenario, governing obligation, consequence, smallest justified correction and proof, and PASS/FAIL/BLOCKED. No optional redesigns, preferences, speculative hardening, or quotas. Mandatory changed-item rustdoc remains a gate. Audit private task mechanics against stronger authority before prescribing machinery. Review unchanged dependencies affected by the changed owner. Record any specific missing source/evidence.

Reports are independent: do not read other reports. The orchestrator compares claims before a fresh independent validator reads all reports. Only independently confirmed/revised findings may enter the final verdict. Follow-up is reserved for material conflicts or unresolved failure paths. Review does not implement, merge, push or deploy.

## Discovery-isolation correction

The initial concurrency/security reviewers disclosed a direct conclusion exchange
after independently tracing their proposals. Their reports are retained for
provenance but will not serve as selected independent discovery evidence or
corroboration. Fresh replacements, given only the original subject and scope,
write `domain-review-concurrency-independent.md` and
`domain-review-security-independent.md`. These replace the two report paths in
the selection table above. The validator must use the replacements; the earlier
reports create no additional independent votes or findings. All other selected
reports remain independent. No source or authority changed.

## Focused follow-up selection

A fresh `followup-rev` is selected for the material authority/correction-boundary
uncertainty raised by STD-003 and SYS-2: whether the no-request-path-Postgres
rule permits generic direct state-head registration, and how current queue
invalidation orders with generic checkpoint acceptance without violating
cross-owner rules. The assigned report is `followup-review.md`. This pass is
needed to resolve the owning contract and correction burden; it is not a vote
on duplicated proposals or a comprehensive second discovery pass.

## Independent validation selection

All eight selected discovery reports (including both fresh domain replacements)
and the focused follow-up are complete. A fresh `ponytail-rev` now independently
validates applicability, coverage, all proposed findings and correction burden,
using the actual immutable range, approved authority and `verification.md`.
Its assigned report is `findings-validation.md`. It receives no intended verdict.
A new blocker discovered by the validator requires separate focused source
validation before retention. The orchestrator writes the final verdict and any
permitted remediation task from the independently validated ledger only.
