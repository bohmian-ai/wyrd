# TASK-001 review subject and navigation

Repository: `/home/thorrester/Documents/GitHub/wyrd`.
Base: `911a8e278d829ab2722fc20ae881b67481dcf9f3`.
Candidate: `c82414d38561cc3f7e17ac3f3ff52223378abde8`.
Read actual changes with `git diff 911a8e278...c82414d38`.
Spec: `changes/active/agent-verdict-portability/spec.md`, approved revision 2.
Task: `changes/active/agent-verdict-portability/tasks/TASK-001-portable-verdict-and-repair.md`.

This is the first comprehensive implementation audit. The R1 authority attempt
was intake-only, with no findings or accepted implementation evidence. Its
approval blocker is cleared by the candidate's approved spec and current user
instruction. No acceptance evidence carries forward. The whole range is in scope.
The existing untracked R1 report is preserved. Only review reports may be written.

## Starting map

| Changed owner | Callers / consumers | Proof to inspect |
|---|---|---|
| `wyrd-spec/src/verification/execution.rs`: `Verdict::response_format`, `restrict_slots`, `portable` | named Prompt resolver; Vala check binding; final `verdict_from` | inline schema structure, author data and slot tests |
| `skald-spec/src/authoring.rs`, `prompt.rs`: named lowering, `edit_response_schema_mut` | native Chat/Responses/Anthropic/Gemini requests, Vala bind | authoring and response-schema-edit tests |
| `skald-agent/src/structured.rs`, `loop_runtime.rs`, `error.rs`, `gateway.rs` | Agent callers; conversation/session; hooks; gateway retry classification | structured_output integration; loop/session/callback regressions |
| `vala-verify/src/blocks/agent.rs`: bind/invoke/execute, removed `conforms`, error mapping | check/action block execution; input/workspace source sets; outer retries | blocks_agent integration; unchanged verdict_from and hosted sandbox guards |
| public error catalog, generated TS/docs | Python/Rust/TS/MCP clients and requirement reasons | codegen evidence and focused verifier-building-block journeys |
| shared judge fixtures and live smoke | all fixture consumers; runtime Prompt-generated schema | focused four-surface journeys, native provider smoke |

This map is a starting point, not a conclusion or coverage limit. Follow callers
and compare existing validators/orchestration owners before accepting additions.
Read root/nested AGENTS, workflow, maintainer guide, applicable topic guides,
TESTING.md, and owning architecture Prompt/Agent/Verifier/error sections.
Consult CodeGraph before locating or reading code. Commands run through mise.

## Selection

- `behavior-rev`: comprehensive task acceptance, invariants, isolation, ownership,
  maintainability and realistic negative paths. No separate invariant/maintainer
  reviewer: state stays per invocation, and new validation extends the loop owner.
- `repo-rev`: broad Rust, Python, TypeScript, MCP, docs, fixture, generated and
  error-contract rules, including changed-item rustdoc.
- `system-rev`: materially changed repair, exhaustion and terminal failure
  propagation through the shared Agent loop and Vala outer retries.
- No domain specialist: no new auth, tenancy, persistent data or shared-state
  concurrency boundary; behavior review covers per-call isolation and error safety.
- Fresh `ponytail-rev` after discovery: independent source/materiality validation.

## Available verification

The original task's Completion evidence records commands and results: 759
wyrd-spec and 295 Skald/Vala tests; 23 hosted guard tests; focused repair and
exhaustion journeys on Rust/Python/TypeScript/MCP; four live provider cases;
format/lint/dependency/codegen/docs and language checks. Treat these as recorded
evidence to assess, not independently reproduced results. Review the original
requirements/scenarios before its completion claims. Full journeys are reserved
for change review. Avoid concurrent Cargo operations; orchestrator owns any
additional execution. Reviewers may request a focused check.

## Discovery comparison and validation handoff

Selected discovery reports are complete: `task-review-behavior.md`,
`standards-review.md`, and `system-review.md`. BEH-001/SYS-001 describe one
effective-schema authority mismatch; BEH-002/SYS-002 describe one refusal-repair
boundary. BEH-003 concerns the live compatibility proof, and REPO-001 is one
grouped mandatory documentation gate. These are proposals, not retained findings.
No focused follow-up was selected: proposals do not conflict materially, and
their traced producer/consumer paths provide concrete inputs to independent
validation. The validator must scrutinize the refusal correction's fit with
the approved error contract rather than infer a new public code or behavior.
All three reports and `verification.md` are validation inputs.
