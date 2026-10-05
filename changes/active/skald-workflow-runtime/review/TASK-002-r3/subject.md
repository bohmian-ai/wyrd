# Immutable cumulative TASK-002 re-review subject

Repository: /home/thorrester/Documents/GitHub/wyrd
Branch at establishment: wyrd/skald-workflow-runtime/TASK-002
Original cumulative base: 0569b79702218600c4f9790f45cc03100d5c6f1c
Committed candidate: 8a8282331042c3cc7610478b3d46583dbbf121f8
Immutable source snapshot: /tmp/wyrd-task002-r3-8a8282331 (git archive of the candidate; read source HERE, not the dirty live tree)
Cumulative actual diff: /tmp/wyrd-task002-r3.diff; reproduce from live Git using explicit BASE/CANDIDATE.
Approved spec: changes/active/skald-workflow-runtime/spec.md, Revision 12, in snapshot.
Original superseded task: same packet tasks/TASK-002-load-and-register-graphs.md.
Active replacement: tasks/TASK-002-cleanup.md.
Prior reviews: review/TASK-002-r1 and review/TASK-002-r2; read prior verdict/ledger and r1 remediation.
Current remediation: review/TASK-002-r2/TASK-002-R2-close-cleanup-review-gaps.md, including committed Implementation Evidence.
Stable prior IDs FIND-TASK-002-1 through -10. Preserve IDs for still-open related obligations, new findings start at 11.

Candidate boundary: user handed off six committed remediations and separately described an uncommitted error-code follow-up whose lanes are running. Review committed HEAD through snapshot; exclude all working-tree changes. This is not approval of the pending follow-up. At establishment 14 tracked files differ from candidate (shared Workflow, error catalog, SDK selectors/docs/declarations/journey assertions). Those changes remain untouched; /tmp/wyrd-task002-r3-excluded-working.diff records exclusion provenance only and is NOT review subject or evidence. Do not read the working follow-up for discovery, do not use its test results as candidate proof. No source reset/stash/commit/rebase or baseline authorship investigation.

Authority: read snapshot AGENTS.md, architecture/agent-rules.md, architecture/wyrd-design.md and wyrd-doctrine.mdx, references/README.md router, complete applicable references including spec-driven-development and maintainer-style. No .codegraph exists, use rg/Git/current snapshot source. Current approved architecture governs, prior superseded private mechanics do not.

Starting map (expand independently, not complete coverage or conclusions):
- Shared facade/view: wyrd-client/src/workflow.rs Workflow::from_path/run, WorkflowCards::load/Cards::workflow.
- Shared graph/hydration: cards/hydrate/{mod,graph,workflow}.rs CardGraphHydrator::{resolve_graph,resolve_refs,load_root,resolve_root_selector}, GraphTraversal/GraphScope and WorkflowBodies; trace Service Bundle sibling and Runtime consumers.
- Existing loader/reference/provenance: wyrd-loader load/parse/resolve/validate and canonical reference visitor/to_durable/CardRefIdentity.
- Runtime lowering: skald-workflow/src/{bodies,workflow_surface,plan}.rs; existing Agent/Prompt binder/default providers and native MockProvider echo. No replacement runtime assumed.
- Server: components/cards/{resolve,service}.rs EffectiveSpecs::{resolve,validate_workflows,load,body}; graph_ready_submissions temporary metadata; RegistrationWriter::write, RegistrationPlan, BindingProjector, tenant/audit/idempotency/version/relationship/upload boundaries. SQL recheck_active_card_refs expected UID fence and lifecycle lock.
- SDK projections: Rust reexports; Python PyWorkflow/PyWorkflowCards, Cards context and native registration/stubs/exports; TypeScript native workflow/cards wrapper + public index.ts JsonValue/WorkflowRun/step/error shapes + generated declarations.
- Proof: loader/client retained exact tests; server pg_workflow_registration 3 tests and SQL pg_cards_register lifecycle lock; Rust tests/workflow_loading.rs ambient child process and public native journey; Python cards CRUD journey and workflow parameter-injection precedent; TS integration/workflow-loading.test.ts and unit/workflow-types.test.ts; fixture team/mixed/shadowed/local-workflow/team-v2 Native variants vs retained canonical gateway example and retired refusal fixture.
- Manifests/dependencies: Rust journey dev-dependency deletion, Cargo.lock, workspace-hack feature union. No new dependency presumed.

Verification: committed remediation Implementation Evidence reports all task lanes and exact focused commands PASS, plus known check:workspace-hack failure. Audit evidence claims against assertions/source, not completion summary. Root may collect read-only snapshot checks; shared live build/runtime results contain excluded source and are not independently candidate-bound. Coordinate any heavy/environment/build lane with root before starting.

Explicit user supplemental scope: examine non-Workflow Python Cards selectors using DataCard parse_space/parse_name/parse_version errors and report a finding if independently confirmed, even if earlier/pre-existing. Do not assume the user's diagnosis true. Include in a clearly marked user-requested supplemental boundary rather than pretending it originated in the cumulative diff. Pending Workflow error follow-up is excluded; current committed Workflow error ownership remains in ordinary task scope. User did not request implementation of supplemental issues.

Roles required: behavior, invariant, repo, maintainer, system; sensitive tenancy/security and registry durability/concurrency. Every agent writes only its exact assigned report in live review/TASK-002-r3 directory; snapshot source remains read-only and do not touch others' reports. All agents are concurrent. Fresh discovery agents must not read current peer conclusions. If snapshot/candidate subject cannot be maintained, report BLOCKED.
