# Task 001 cumulative remediation re-review

- Root: `/home/thorrester/Documents/GitHub/wyrd-verifier-building-blocks`.
- Original base: `08b38f74255d344e1c5ec9a6d278935a26277622`.
- Prior candidate: `896e563bee69ab6b5f47b31977d7de1a0ca88d57`.
- Immutable candidate: `eb1681461897ee1eb9fe1fa4e00cb0b435658559`.
- Full diff: `cumulative.diff`; latest fix locator: `remediation.diff`. Review the complete original base-to-candidate, not only fixes.
- Approved spec r6: `changes/active/verifier-building-blocks/spec.md`, snapshot `spec-snapshot.md` here.
- Original task: `changes/active/verifier-building-blocks/tasks/TASK-001-verifier-contract.md`, snapshot `task-snapshot.md` here.
- Prior review directory: `changes/active/verifier-building-blocks/review/task-001-20261015-896e563be/`; read `verdict.md`, `findings-validation.md` and `TASK-001-R1-contract-gaps.md` there. Initial six FIND-TASK-001-1..6 are closure hypotheses, not conclusions.
- Current original task handoffs and TASK-005 explicitly assign removal of the legacy LLM judge everywhere, including offline eval, to TASK-005. Owner's current direction overrides the earlier remediation wording to preserve it indefinitely. Task 001 must retain base compilation until that later cutover; do not demand its removal now or reject the owner's final removal decision. Scenario simulation itself is not blanket-removed. The approved new seam uses BlockError, not JudgeError.
- Do not edit production, specification, original tasks or previous reports. Write only your assigned new review report. Uncommitted docsite/planning files are excluded. Check pinned source with git show if the tree changes; report candidate changes to orchestrator.

## Shared navigation map — independently expand it

1. `wyrd-spec/src/card/blocks/{mod,reference,validate,schema_path,extract}.rs`: original authoring/reference/known-shape logic and extraction outputs. Latest changes share BuiltinTool membership, check Verdict structural schemas and bound complete schema traversal. Compare existing Prompt/schema, DAG/reference visitor owners and all producers/consumers.
2. `wyrd-spec/src/card/agent.rs::BuiltinTool`; `auth/permission_scope.rs`; `wyrd-runtime/src/permission.rs` and `request_context.rs`: new typed catalog/tool resource, global/scoped coverage/intersection, strict combination validation, role/effective/accepted authority serialization. Trace existing wildcard/gateway/Bifrost cases and sibling consumers; no default tool grants are prescribed.
3. `wyrd-spec/src/verification/execution.rs::ToolCallRecord`, `vala-eval/src/blocks.rs`: typed tool names, optional output digest and byte count, injected action/check handoff and result consumers. No runtime projector implementation expected here.
4. `skald-spec/src/{media,prompt,lib}.rs`, IDs, refs visitor, eval operator/planner/spec, staged module exports: audit cumulative behavior/reuse/regressions and required rustdoc, not only new fix lines.
5. `wyrd-server/src/components/gateway/batches.rs::invocable`: exhaustive match adapted for new scope. Trace the real gateway permissions/discovery caller and ensure existing scope behavior survives.
6. Original Task 001's scenarios/constraints/non-goals and all prior findings: full cumulative acceptance. Runtime tools/queue/sandbox/providers/final live protocol activation remain later tasks.

## Available proof (claims until logs/source verify)

Task/remediation evidence reports 1139 library tests across wyrd-spec, wyrd-runtime, wyrd-skald-spec and vala-eval; all-features Clippy with warnings denied; workspace all-features tests cargo check; fmt/check:deps/codegen:check (no generated diff)/diff whitespace pass. Inspect task evidence for actual focused tests. Do not consume the conversational implementation summary or another current discovery report as evidence. Orchestrator may add fresh proof below. No full integration/journey suite is required for this bounded remediation.

## Required review discipline

Fresh orchestrator proof at pinned candidate: task-scoped four-package library command passed 1139/1139, zero skipped (`library-check.log`); `mise run codegen:check` passed with no tracked generated changes (`codegen-check.log`); cumulative `git diff --check` passed. Production source/HEAD unchanged. The original R1 task plus evidence is additionally captured as `remediation-task-snapshot.md` here. Reviewers can cite these fresh logs without rerunning broad suites.

Read `.agents/skills/wyrd-task-review/SKILL.md`, applicable AGENTS/design/agent/reference authority, original task/spec snapshots, prior verdict/ledger/remediation and actual source. Follow CodeGraph first for discovery. Separate source-confirmed prior closure, new defects and verification limits. Prior IDs stay stable; new confirmed IDs start at 7. Group shared causes across all callers. No optional hardening or generic schema proof engine; consider approved conservative bounded structural compatibility and runtime refusal separately. Naming/placement/style alone nonblocking; mandatory changed-item rustdoc is the explicit exception. Complete assigned role coverage/report with PASS/FAIL/BLOCKED. No production mutations or implementation.
