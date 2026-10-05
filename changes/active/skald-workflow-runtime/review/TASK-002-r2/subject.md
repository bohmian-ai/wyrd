# Immutable review subject and navigation map

Repository: /home/thorrester/Documents/GitHub/wyrd
Base: 0569b79702218600c4f9790f45cc03100d5c6f1c (original TASK-002 base)
Candidate: e7d16b5bd622b9a564a49edb18239df7f209ca92
Full cumulative diff: /tmp/wyrd-task002-r2.diff; reproduce with git diff BASE CANDIDATE.
Approved authority: changes/active/skald-workflow-runtime/spec.md, Revision 12.
Active replacement task: changes/active/skald-workflow-runtime/tasks/TASK-002-cleanup.md.
Original superseded task: tasks/TASK-002-load-and-register-graphs.md in that packet.
Prior verdict, validated six findings and remediation: review/TASK-002-r1/{verdict.md,findings-validation.md,TASK-002-R1-close-validated-graph-gaps.md} in that packet. Preserve FIND-TASK-002-1 through -6 for closure; current Revision 12 supersedes conflicting old mechanics.
Revision 12 approval and readiness: spec.md opening and review/revision12-plan/. Authority/skill edits in cumulative diff accompany explicitly approved revision; assess applicability rather than treating them automatically as unrelated product drift.
Clean tracked worktree at start; no .codegraph directory, use rg and existing navigation.

Starting map (expand independently; not complete coverage or conclusions):
- Public composition: crates/shared/wyrd-client/src/workflow.rs Workflow::{from_path,run,as_skald}, WorkflowCards::load, Cards::workflow; callers three SDKs and client unit tests.
- Shared graph owner: cards/hydrate/{mod,graph,workflow}.rs CardGraphHydrator, GraphTraversal, GraphScope, WorkflowBodies; existing Service/disk hydration sibling consumers. Trace authored Sibling vs external Ref and registered UID-bound reads.
- Existing parser: crates/shared/wyrd-loader/src/{lib,parse,resolve,validate}.rs; canonical wyrd-spec reference slots and graph identity. Prior normalization/type additions and their removal must be assessed cumulatively.
- Runtime: crates/skald/skald-workflow/src/{bodies,workflow_surface,plan}.rs from_card_bodies/validate_card_bodies/card_body_dependencies; native builder/runtime/provider/tool callers.
- Server: wyrd-server/src/components/cards/{resolve,service}.rs EffectiveSpecs load/validate_workflows/registration planning/write; SQL queries/cards/relationships.rs recheck_active_card_refs; tenant transaction, RBAC/audit, lifecycle/UID fences.
- Projections: Rust SDK lib.rs; Python src/{workflow,state/mod}.rs + agent/cards package stubs/exports; TS native/src/{workflow,cards}.rs + wyrd/src/index.ts and generated index.d.ts/index.d.cts. Inspect manifests, registration and public caller lifetimes.
- Proof: loader bundle tests; workflow::tests::from_path_uses_existing_loader; server tests/pg_workflow_registration.rs three named tests and SQL pg_cards_register.rs; Rust tests/workflow_loading.rs; Python tests/integration/cards/test_cards_crud.py and unit workflow save/load; TS integration/workflow-loading.test.ts; checked-in examples/workflows/code-review and tests/fixtures/workflow-loading.
- Verification claims: task Implementation Evidence section contains claimed selected counts, all listed lanes, command correction and explicit limits. Treat as evidence to audit, not conclusions. No raw execution logs supplied. Orchestrator may collect additional focused evidence; do not mutate source or rebuild/generated tracked files.

Read applicable AGENTS.md, architecture/agent-rules.md, wyrd-design.md, wyrd-doctrine.mdx, spec-driven-development.md, maintainer-style.md and reference router. Sensitive domains: tenancy/security (exact authorized external reads, provenance, public selector trust boundary); registry durability/concurrency (preflight-to-write expected UID, lifecycle locks, atomic audited registration). No runtime engine implementation changes beyond changed bodies lowering are assumed without tracing.
Review-only: write only assigned report. You are not alone; do not alter/revert source or other reports. Read complete cumulative diff and applicable authorities; independently expand map and inspect callers. No other reviewer conclusions are supplied. Report coverage, exact evidence, findings and required role-specific result. Tests optional if useful; coordinate with root before heavy/build/environment lanes. All test commands use mise. User requests review, not remediation implementation.
