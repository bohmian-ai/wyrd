# Immutable TASK-004 R1 re-review
Root: /home/thorrester/Documents/GitHub/wyrd-worktrees/verifier-building-blocks-task-004
Base: 41b0f01d9b47d9b07b078b2431fcdd4cba56a9b0
Candidate: dddf985563029a004fa768110e36d0c5f12bcded
Branch: change/verifier-building-blocks-task-004
Prior candidate: 07adb7907951f343544e550f6cabc5b7aa3aadea
Authority: changes/active/verifier-building-blocks/spec.md approved r8; original tasks/TASK-004-sdks-examples-and-docs.md; prior review task-004-20261015-07adb7907/verdict.md and TASK-004-R1-sdk-contract-and-journey-closure.md (paths relative to change directory).

Human decisions: all extra remediation changes approved, including fixture validation test and *.pdf binary. No spec revision required. FIND-004-7 superseded by r8; external repositories now TASK-005. Real-server journey execution deferred after TASK-005; require authored, compiled/typechecked and discovered proof only. code.run arguments provisional/TASK-005. Do not re-litigate these decisions.

Review cumulative.diff (base through candidate), using remediation.diff (prior candidate through candidate) to locate R1 owners. Planning/review artifacts are provenance, not runtime changes. Existing untracked prior-review files are user inputs; leave untouched. Source immutable; write only assigned report. Never print environment variables. Do not commit/push/edit production or generate artifacts. No full journeys or gate. Cargo checks orchestrator-only.

Shared navigation map (starting point, expand independently):
- Shared client observe/{mod.rs,tests.rs}, verify/{execution.rs,mod.rs}, storage/upload; Execution wait and transport retry owner.
- Python src/{verify.rs,client.rs,observe/mod.rs}; existing LazyConnection owner; verify/__init__.pyi TypedDict results; tests/typecheck/verify_contracts.py and unit/state/test_observe_surface.py, public exports.
- TS native/src/{cards.rs,verification.rs}, wyrd/src/index.ts nativeInputs/verify/submit; generated index declarations; unit/observe.test.ts.
- Three-language integration verifier_building_blocks, scheduled_drift_alerts_operator, verify_in_real_time and support helpers/provider doubles; fixtures/cards/verifier_building_blocks with nine samples; wyrd-spec/tests/verifier_fixtures.rs.
- Full task also includes support-desk examples, storage reuse, typed permissions, docs/runtime/extraction/tool references, architecture and generators. Inspect cumulative diff and consumers.
- Sensitive domains: security/tenant/evidence authority (grants, hosted tool denial/delegation, credentials); concurrency/recovery (poll deadline, retained/lazy connection, replay).

Read AGENTS.md, architecture/agent-rules.md, architecture/wyrd-design.md, architecture/wyrd-doctrine.mdx, reference router and applicable references, spec-driven-development.md and maintainer-style.md; Bifrost authority for affected paths. CodeGraph first (index exists).
Prior implementation evidence is a claim to verify. Independently inspect complete task and cumulative candidate with R1 emphasis. Do not read other discovery reports before finishing your own.
