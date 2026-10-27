# TASK-001 remediation review

Repository: /home/thorrester/Documents/GitHub/wyrd-worktrees/wyrd-verifier-upload-artifact
Original base: 1c52b78bbedec4bdb3cdd6bf8807d9298fd28d44
Prior candidate: da7622b52994946e03b9ba241032b99d096063cf
New candidate: 0d30e262aafa07766ca73cd0613c71c861b9ddd0
Remediation diff: git diff da7622b52 0d30e262a
Cumulative context: git diff 1c52b78bb...0d30e262a
Approved spec: changes/active/verifier-artifact-workspaces/spec.md revision 3
Original task: changes/active/verifier-artifact-workspaces/tasks/TASK-001-reusable-artifact-workspaces.md
Prior verdict/ledger/reports: changes/active/verifier-artifact-workspaces/review/task-001-r1/
Remediation: task-001-r1/TASK-001-R1-complete-artifact-workspaces.md (implementation evidence appended)

Review the seven prior FIND-TASK-001-1..7 closures, latest diff, affected callers and tests. Carry forward original accepted selection/reference identity, inventory integrity, RBAC/tenancy, citations, cleanup and shared lifecycle/parser ownership from prior source-backed reports unless new source changes affect them. Do not repeat comprehensive discovery. Prior review artifacts entered history in e31df6868; those report additions are context, not unrelated implementation. No rebase; ancestry is continuous. Exclude pre-existing dirty service-identity-and-telemetry TASK-007. Candidate source is immutable; reports only may be written. No implementation/commit/push.

## Explicit new human-directed scope

After requesting re-review, the user instructed: "I want the review to address the 1st 2 risks". These are: (1) symlink/empty-directory/corrupt-PDF/output-collision upload refusals share WYRD_LOADER_400_INVALID_ENVELOPE and identical inner codes, requiring prose to distinguish; (2) expected tool input refusals report SKALD_TOOL_500_CALL. Prior remediation explicitly forbade inventing new variants solely to strengthen assertions; it did not independently validate these risks as findings. Reopen only these shared error producer/projection paths under the user's explicit direction. Assess existing typed errors, public catalog semantics and smallest correction; distinguish human-directed requests from independently discovered FIND IDs. If correction needs a material new public/security/architecture decision outside approved authority, name it rather than silently prescribe it. Do not implement.

## Navigation map (starting point, expand as necessary)

- FIND-1: server hosted/workspace.rs Workspace::search/snippet, Collected; existing workspace_search_matches_lines test; snippet capped independently of regex span.
- FIND-2: Python src/state/mod.rs PyCards::upload_artifact; observe/mod.rs json_value visibility; existing invalid_argument/parse_version_bump, shared Cards::upload_artifact/ArtifactUpload::prepare; authored Cards stub and every changed upload caller.
- FIND-3/4/5: Python integration/test_verifier_building_blocks.py Probe/play upstream, fixed fixtures/cards/artifact_workspaces; code.run sandbox invocation reading opaque mounted bytes; real search/read/bifrost.query/final and verification result flush; exact codes/negative assertions; checked-in Card setup with fixed names. Preserve content-sensitive real upstream responses.
- FIND-6: loader ExtractError payload, upload/tests.rs paths panic docs, shared client Cards upload cancellation/retry docs.
- FIND-7: docs verifier-recipes Uploaded files; machine llms copies; final SDK signatures and existing CodeFromFile sources.
- New human error scope: loader upload/extract and LoaderError/diagnostics wrapping; client upload conversion and SDK public projections; hosted tool dispatch/RecordedTool/Skald ToolError producer-to-consumer and sibling consumers. Use existing catalogs and error guidance first.

## Authority and evidence

Read .agents/skills/wyrd-task-review/SKILL.md, root and applicable nested AGENTS (Python/docs; TS if its projection enters error tracing), changes/spec-driven-development.md, agent_docs/index.md and applicable rust/errors/maintainer-style/agent-surfaces guides, architecture/README.md and owning Artifact/Verifier/Agent tools and security sections, TESTING.md. CodeGraph exists: use mise exec -- codegraph explore before textual code discovery. Read committed source/diffs rather than implementation summary. Evidence/diagnoses live in original/remediation tasks; reported green checks are not independently reproduced execution.

## Reviewer selection

- One fresh focused behavior reviewer for the explicitly added two public-error risks, with source-backed reachability/materiality/minimum correction. Exact report: error-boundary-review.md.
- Then one fresh ponytail-rev for all seven closures, changed code/callers/gates, carried-forward acceptance and independent validation of the error-boundary report. Exact report: findings-validation.md. Prior findings keep IDs; human-directed additions remain separately identified. Any newly found blocker needs separate fresh focused validation before retention.
- No broad specialist team: runtime ownership, tenancy, storage, security and deployment authority are not materially changed in remediation. Add specialists only on new material evidence.

All reviewers share this map as navigation, not conclusions. No parallel Cargo-backed commands; coordinate with root before executing any. Static source review is appropriate where sufficient. Every selected reviewer owns only its exact report, and must not modify implementation or others' work.

## Reconciliation

Both selected reports are complete. Fresh validation closed every original
FIND-TASK-001-1..7 and confirmed the two human-directed error proposals.
No new independent blocker was discovered. The validator resolved safe-details
forwarding against ConversationTurn's open JSON and all provider projections;
no separate follow-up or spec revision was necessary. Verdict/remediation use
only the independently validated ledger. Original closures carry forward;
remaining correction scope is HUMAN-ERROR-1/2 only.
