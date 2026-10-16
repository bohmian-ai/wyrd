# Immutable TASK-005 R3 acceptance review

## Isolation and range
Original root `/home/thorrester/Documents/GitHub/wyrd-verifier-building-blocks`. Read ALL candidate source and authority ONLY from detached checkout `/tmp/wyrd-task005-review-02848d737`, HEAD `02848d737a621938ec0b5f817e1aa84d5f3df1fd`. Set workdir explicitly. User's working tree/untracked prior reports/later commits are not reviewed source. The detached checkout has no `.codegraph/`; skip CodeGraph rather than querying mutable original-tree source. No source mutation, generator writes, diff/patch/source archives. New review directory holds reports only.

Cumulative base exclusive `f2a726c90edbf8956c7b91c843379175ee20eac0`. Actual full subject: `git diff f2a726c90...02848d737`. Prior candidate `de69cb69b68eced72f7d760681d27253e013f8a1`; recent diff `git diff de69cb69b...02848d737` locates changes but does not replace cumulative acceptance.

## Integration context
Between candidates the branch integrated main via merge `1fa4526cc0a0619a36ae93458908863a32e15084` (parents `d3fa6444b8cb46dc27727ce108bd837fb98a9207`, `0ae89b382af138d3493953a5eeef90ccf9fa17b9`). This imports release/build/docs baseline work. Do not mistake approved upstream integration context for unrequested TASK-005 machinery or prescribe deleting unrelated main work. Inspect actual task consumers and integration seams in the full candidate. R2 implementation-only locator is `git diff 1fa4526cc...02848d737` (30 files); source of truth remains full cumulative candidate.

## Authority and prior material
- Frozen `changes/active/verifier-building-blocks/spec.md` approved r9; local evidence PUT and operation-level preparation already approved, do not reopen.
- Original `changes/active/verifier-building-blocks/tasks/TASK-005-server-integration.md` plus prior original task snapshot as needed.
- R1: `changes/active/verifier-building-blocks/review/task-005-20261016-99882cf82/` verdict, findings-validation, TASK-005-R1-verification-corrections.md, TASK-005-S1-provider-conformance-sweep.md and findings.md.
- R2 remediation/evidence at candidate: `changes/active/verifier-building-blocks/review/task-005-r2-20261016-de69cb69b/TASK-005-R2-input-capacity-and-provider-corrections.md`.
- Frozen prior R2 report copies IN THIS NEW REVIEW DIR: prior-r2-verdict.md, prior-r2-findings-validation.md, prior-r2-followup-review.md; provenance/hashes in prior-inputs.md. These are mandatory closure hypotheses; original prior report files remain untouched. Prior stable IDs 1..13; prior open 2/7/8/10/11/12/13. Preserve IDs on residuals; next unused is 14 for genuinely new confirmed causes. Prior closed findings require current source proof.
- Read full assigned role in frozen .agents/skills/wyrd-task-review/SKILL.md, AGENTS.md, architecture/agent-rules.md, required spec-development/maintainer references and applicable authorities via reference router.
- Owner instruction remains NOTHING HAS SHIPPED / NO MIGRATIONS NEEDED: no old-catalog/history/pending-dispatch upgrade machinery or proof. Previously rejected DATA-001/BEH-04 and R2 absent-target receipt BEH-R2-01 do not re-enter without new approved current-behavior evidence.
- Human-directed queued trace link/sampling/routing is a separate obligation, not retroactive FIND ID. Agent-runtime-composition r2 is draft, not implementation requirements; S1 intentionally records live failures, no invented SystemOne/MCP composition demand.

## Shared starting navigation (expand independently)
- Public control: components/verification/service.rs execute/enqueue/authorize_run/authorize_evidence and engine error mapping; routes.rs/MCP share control; shared client verify/Observe and all SDK projections.
- Slot conformity: Vala blocks/engine.rs conform/run/EngineError before planning and drive; InputSlot/InputDeclaration; JSON Schema existing owner; required/optional missing semantics; direct vs queued wrappers.
- Resolved media/resource: hosted/blocks.rs + TenantEvidence::admit/read/artifact_file/stored and existing authorized Artifact/inventory path; combined original file budget, slot type/format/size and reuse across extract/native media/workspaces/citations.
- Capacity: Vala ToolSession/Refusal now watch + refusal_raised; AgentExecutor invoke selection vs run_prompt; concurrent siblings; Sandbox/Workspaces Busy; engine Busy and runner deferred/refunded fenced release; direct503.
- Google protocol: skald-prompt builder google_settings Gemini/Vertex override; skald-spec wire/google_generate.rs FunctionCall.thought_signature and constructors/adapter/convert; skald-tool helpers; gateway adapter; Python google projection; typed assistant history/rebuild in Skald Agent.
- Queue/results: Scribe inputs ACK -> SubmissionSink -> SQL accept/claim/reclaim -> runner result/settle -> per-payload result ID and selected status pointer/features/Operator frozen context; optional scheduled slots/baseline identity.
- Tracing: telemetry context serve_linked_work/adopt_span/sampling/routes, SQL Cause, runner claim/trace, accepted authority, gateway capture and SDK timeline; preserve operator policy independence.
- Proof: Vala blocks/engine tests + integration/blocks_agent.rs concurrent cancellation; Skald Prompt builder tests and Agent integration/loop_gemini.rs signed replay; pg_verification_routes and pg_hosted_artifacts direct/queued/media/Artifact fixtures; prior rollback/reclaim/optional-slot/trace tests and three SDK journeys.
- Documentation: exact FIND-7 item inventory in R2; results.rs/runner/service/Bifrost result identity prose; two runtime docsite pages for FIND-8; current docs/generated/declarations/source parity.
- Complete cumulative consumers remain in scope: registry/loader/ref lineage, CLI/UI mocks, retained scenario evaluation, Bifrost table/Gate/Oracle attribution, packaging/boot/settings, external examples at recorded immutable commits. Do not review only latest seven patches.

## Verification inputs and limits
R2 committed evidence table records exact RED/GREEN + focused Vala/Skald/server proof, route+hosted slice 18/18; fmt/lints/check:deps/docs:check/codegen:check/diff check. No SQL changed in R2, so check:tenant-isolation not rerun. Verify claims against tests/source; they are supplied evidence, not fresh reviewer runs. Original gate on 03712af5e predates R1/R2; do not call it candidate gate. Explicit limits: no dedicated MCP input test; new Agent cancellation not recomposed with queued refund, which uses existing runner proof. Assess real shared-path proof and task requirements without inventing extra suites. No broad build/gate/live-provider/credential use is requested during static discovery.

External repos: use only original task's recorded commit objects in /home/thorrester/Documents/GitHub/wyrd-example-ts, wyrd-example-py, wyrd-example-rs via git show/diff; no moving worktrees or local artifacts. Latest recorded heads TS5c79080, Py1e27f68, Rust669d130 (expand with git rev-parse if needed).

## Independence and write ownership
Discovery must not read THIS round's peer reports/conclusions or receive an intended verdict. Read prior copies as hypotheses, not conclusions. Only write assigned absolute report path in original new review directory. You are not alone: preserve all peer/user files. Reports only, no implementation/authority/task/status/index/commit/merge/push/deploy. If fixed detached source changes, report BLOCKED.
