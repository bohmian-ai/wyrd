# TASK-005 review subject

- Base: `89d9638d4986d8ed833bfc99cd4ce28007afdb8b` (plan commit).
- Candidate: `cb6269a5143cf95210e88412f71cd8972080ad3d`.
- Immutable detached source: `/tmp/wyrd-task005-review-cb6269a51`.
- Diff: `git diff 89d9638d4986d8ed833bfc99cd4ce28007afdb8b...cb6269a5143cf95210e88412f71cd8972080ad3d` (66 files).
- Approved contract: candidate `changes/active/verdict-response-mapping/spec.md`, revision 7, REQ-015–020; original task `tasks/TASK-005-optional-citations-and-provider-smoke.md` and its carryover TASK-004 example.

## Maintainer steering

The current request explicitly supplies decisions dropping TASK-005's L1 benchmark and replacing the held-out retrieval-quality experiment/labeled set with the live OpenAI evidence judge. These task-specific decisions override older packet text and default preferences for this review. The wait inventory remains in scope. One live example proves that case, not general retrieval quality. The maintainer does not require TDD remediation; preserve recorded history separately from behavioral acceptance.

The main checkout is newer and has unrelated dirty files, including a different untracked packet snapshot. Review candidate-owned source and authority from the detached worktree. Do not overwrite current work or switch its HEAD. Review writes are limited to this new report directory.

## Independent boundaries

- `behavior_rev`: primary requirements/scenario/proof matrix, engine classification/requiredness, schema and preserved workflows; covers unassigned rules.
- `invariant_rev`: hosted evidence views, exact original provenance/invalidation, attempt-scoped IDs, resource charging/release, concurrency/tenant isolation and waits.
- `repo_rev`: public contracts/projections, authoring and result recording/read-back, SDK/MCP/HTTP/UI/docs, five-provider example and gateway refusal sanitization.
- A fresh `ponytail_rev` validates coverage and every proposed finding after discovery reports. Any new blocker from validation receives an independent focused source check.

No separate maintainer/system/domain reviewer: maintainability remains with the primary, lifecycle/failure/resource and tenant boundaries are assigned to the invariant specialist, and provider error confidentiality to the repo specialist. No migration or new durable storage boundary appears in the changed-file list. Add focused independent expertise if discovery identifies an uncovered material boundary.

## Navigation and evidence

`wyrd-spec` verifier transform/validation and verification execution contracts -> shared client authoring -> Vala blocks Agent/tools/engine -> server hosted workspace/evidence/tools -> existing result/outbox -> SDK bindings/read-back, MCP and execution UI. Fixtures and owner tests exercise the same paths. Navigation is not a coverage conclusion.

Read actual assertions, candidate-bound recorded execution and direct Git diffs. The main checkout's CodeGraph was consulted for initial navigation only; its current sources differ from this candidate. The detached worktree has no index; source reads there are authoritative. Review does not implement, commit, push or launch paid live calls.
