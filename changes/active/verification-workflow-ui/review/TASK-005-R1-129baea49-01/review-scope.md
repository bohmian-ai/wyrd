# TASK-005 remediation immutable subject

Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/verification-workflow-ui-resume`.
Original base: `05fc05138a0a72d59a2fd9b96fae5e6e4ad6f285`.
Prior reviewed candidate / remediation base: `147c235fd19dd4a400da1515a908db0d5521477d`.
New candidate: `129baea4983d5b9444993503938e3641f760670b`, initially clean HEAD.
First remediation commit: `5a3f80c5953686e5ace197d8e83f9fc36acebc1c`.
Inspect `git diff 147c235fd 129baea49` for remediation; cumulative `git diff 05fc05138a...129baea49` is available. Ancestry continuous, no rebase correspondence needed.
Approved spec: verification-workflow-ui revision 8, supporting authorization-model revision 9, as in original review.
Original task: `changes/active/verification-workflow-ui/tasks/TASK-005-pr-review-sets.md`.
Prior verdict/reports: sibling `TASK-005-147c235fd1-01/`.
Remediation task: `TASK-005-147c235fd1-01/TASK-005-R1-recorded-inspection-and-component-contract.md` (now includes implementation evidence).
Stable findings: FIND-TASK-005-1 (duplicate fallback requirement keys), -2 (preterminal invocation requirements hidden), -3 (StatusMark component contract).
No `.codegraph/` exists. Production source immutable; write reports only in this new directory. Committing historical review reports is not a product change or an authorization to commit this review.

## Navigation map (not a coverage conclusion)

- Shared presentation owner: packages/ui/src/lib/components/VerificationExecution.svelte, requirement navigation and final-result versus invocation inspector.
- Shared state owner: packages/ui/src/lib/execution.ts requirementState/requirementAttempts/executionStates; graph and fallback consumers.
- Existing component contract: brand/components.json, StatusMark export and brand/gen-theme.test.mjs.
- Proof: ui-library.test.ts new repeated/unfinished requirement selectors; ui-library.integration.test.ts new staged approval Repository reader journey plus existing deleted-definition/subject-output journeys.
- Fixtures: fixtures/cards/approvals/staged.yaml and staged-repository.yaml; existing signoff fixture unchanged.
- Existing retained invocation producer/schema/read/authorization unchanged; assess via prior source-backed evidence and affected consumers.
- Review docs committed alongside remediation; inspect evidence additions, do not mistake prior reports newly tracked for new product behavior.

## Independent assignment

One fresh ponytail-rev independently validates prior finding closure, changed code and affected graph/inspector/contract/fixture/real-server paths, proof strength, TDD evidence and prior evidence applicability. No initial discovery fanout: this is a bounded UI/fixture remediation, no durable/backend/auth/wire change. Add a separate focused validator only if this reviewer discovers a new blocker; no reviewer validates its own new finding. Reviewer returns reconciled matrix for verdict.md rather than duplicating it in report.
Root orchestrates verification and writes final verdict; root does not review or validate its own findings. No test/Cargo commands by reviewer without coordination; JS source inspection is unrestricted.

Authorities remain original AGENTS/nested rules, approved spec/original task/R1, changes/spec-driven-development.md, TESTING.md, agent_docs/maintainer-style.md, architecture retained visibility/Verifier rules, UI skill/references. Current accepted AC-009 differences remain accepted. Remediation TDD claimed PASS, original TASK-005 TDD VIOLATED must remain independently recorded.
