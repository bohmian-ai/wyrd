# Immutable Task 001 review subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-verifier-building-blocks`
- Base: `08b38f74255d344e1c5ec9a6d278935a26277622` (merged dependency state).
- Candidate: `896e563bee69ab6b5f47b31977d7de1a0ca88d57`.
- Complete cumulative diff: `cumulative.diff` alongside this file; inspect with `git diff <base> <candidate>` too.
- Approved authority: original `changes/active/verifier-building-blocks/spec.md`, revision 6, captured as `spec-snapshot.md`.
- Original task: `changes/active/verifier-building-blocks/tasks/TASK-001-verifier-contract.md`, captured as `task-snapshot.md`. Implementation evidence appended there is a claim, not acceptance authority.
- The owner explicitly approved removing JudgeError from the new ActionExecutor/CheckExecutor contracts. Candidate includes that change. Existing offline scenario evaluation is preserved.
- No prior Task 001 verdict or remediation is known. The earlier plan-readiness review is not implementation acceptance.
- Uncommitted docsite/planning changes are not implementation under review. Do not modify production source, task, or spec. Each reviewer owns only its assigned report.

## Shared navigation map (starting points, not coverage conclusions)

1. `crates/wyrd-spec/src/card/blocks/{mod,validate,reference,schema_path,extract}.rs`: authoring, pure registration and schedule validation, reference parsing/known schema paths, locked outputs. Compare existing `card/verifier.rs`, eval/trace/assert operators and Prompt owners; expand to consumers and tests.
2. `crates/wyrd-spec/src/verification/execution.rs`, `verification.rs`, `ids.rs`, `error.rs`: requests, declarations, upload plans, status/results/errors, typed slot/execution IDs; compare original public verification shapes and their consumers. New exports are staged; Task 005 owns activation and removal of old public types.
3. `crates/wyrd-spec/src/refs/mod.rs`: existing visitor extended for block refs; trace loader callers, reference normalization and validation.
4. `crates/wyrd-spec/src/vala/eval/{plan,operator,spec,mod}.rs`: shared generic DAG planner and operator validation. Trace existing offline evaluation consumers/regressions.
5. `crates/skald/skald-spec/src/{media,prompt,lib}.rs`, `wyrd-spec/src/card/prompt/parameter.rs`: format vocabulary and Prompt media-kind accessor. Trace existing native-media binding, schemas, normalization and extension/MIME callers.
6. `crates/vala/vala-eval/src/{blocks,lib}.rs`: injected ActionExecutor/CheckExecutor, BlockError, call inputs/provenance/position/deadline. Compare existing executor and judge invoker owners; runtime implementations belong to Tasks 002/003.
7. Task scope includes existing auth contracts `wyrd-spec/src/auth/permission_scope.rs` and runtime permission owners; inspect actual candidate and the approved task for global/scoped tool invocation obligations.

## Verification evidence available

Implementer reports 1069 library tests passed across wyrd-spec, wyrd-skald-spec and vala-eval; all-features Clippy with warnings denied; workspace all-features tests cargo check; fmt, check:deps and diff whitespace checks passed. Exact commands and named tests are in the task evidence. These reports are not fresh review execution or independent proof. `codegen:check` was not run, with the claim that registered generator inputs did not change. Review that claim against actual source and governing gates. Full integration/journeys are intentionally deferred until all five tasks merge. Review is static unless a focused check is needed; record proof limits.

## Review instructions

Fresh review verification: `mise exec -- cargo nextest run --locked -p wyrd-spec -p wyrd-skald-spec -p vala-eval --lib` passed all 1069 tests at this candidate; see `library-check.log`. The three exact regression selections in `focused-check.log` also passed. `git diff --check <base> <candidate>` passed. `mise run codegen:check` subsequently passed; see `codegen-check.log`. Its generation left tracked outputs unchanged. No integration, journey, or broad aggregate was run during review.

Read the invoked `.agents/skills/wyrd-task-review/SKILL.md` for your assigned role, plus AGENTS.md, agent rules, approved authority and applicable references. Source must remain immutable: use `git show <candidate>:<path>` if working source changes. Follow CodeGraph first for discovery; its indexed source is a navigation aid, verify against candidate where necessary. Independently expand this map. Do not consume other discovery reports or the conversational implementation summary. Only behavioral, security, tenancy, durability, public-contract consequences, deletion of unearned code, and required rustdoc can block; style-only preferences cannot. Return the complete role-specific coverage and evidence report, proposed findings with local IDs, and PASS/FAIL/BLOCKED.
