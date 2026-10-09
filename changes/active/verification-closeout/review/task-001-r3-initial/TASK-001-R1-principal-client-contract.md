---
id: TASK-001-R1
kind: remediation
status: ready
spec: SPEC-verification-closeout
spec_revision: 3
parent_task: TASK-001
remediates: [FIND-TASK-001-2, FIND-TASK-001-3]
requirements: [REQ-003A, AC-007]
---

# R1: Close principal declarations and direct Verifier invocation

## Subject and intended outcome

Approved spec: `changes/active/verification-closeout/spec.md` revision 3. Original task: `changes/active/verification-closeout/tasks/TASK-001-r3-principal-roles-and-local-flow.md`. Review: `changes/active/verification-closeout/review/task-001-r3-initial/verdict.md`. Base `c46afdcac`; reviewed candidate `437205debc628538ba6aa4ec828601c7c40145b4`. This task corrects the two independently validated findings and delivers the subsequently approved REQ-003A/AC-007 without changing the immutable review verdict.

Principal discovery must expose an accurate typed CardRef value to Python and TypeScript callers. Changed Rust declarations must meet the repository rustdoc gate.
Users must also be able to register one direct task Verifier, bind it to an Agent
or Service, and invoke it through the real-time endpoint when their principal
has scoped permission for that exact Verifier.

## Diagnoses and selected corrections

### FIND-TASK-001-2 — Principal CardRef types do not match the returned value

`PrincipalSummary` describes a Principal, not a Card. Its identity is
`principal_id`; `card_ref` is an optional reference to a Card associated with
that Principal. A Service principal bound to a Service Card has a `card_ref`,
while an unbound Service principal has `null`. An Agent principal is Card-bound
and has a `card_ref`; a User principal has `null`. The Rust wire type is
`Option<CardRef>`, and the shared client returns that value without changing
the Principal's identity or behavior.

`sdks/wyrd-sdk-ts/wyrd/src/index.ts::PrincipalSummary` declares the non-null
value as a loose string record although the SDK already has `CardRef`;
`sdks/wyrd-sdk-python/python/wyrd/stubs/principals.pyi` declares
`dict[str, str]`, although the Python boundary produces a dictionary whose
optional UID can be null. Callers taking a discovered Card-bound principal
into another Card operation therefore receive inaccurate type guidance, and
TypeScript accepts invalid shapes. The current discovery journeys assert
principal identity but do not use its Card reference.

Use the existing TypeScript `CardRef` type for the non-null TypeScript result
and retain `null` for unbound principals. In the Python stub source, describe
the actual mapping with required Card identity fields and accurate
optional/null fields, then regenerate the assembled public stubs through the
existing generator. Keep the Python runtime dictionary; declaring its
`CardRef` class as the result would misstate runtime behavior. This finding
only aligns SDK types, tests, and documentation with the existing response.
Do not change principal storage, server response values, conversion,
transport, or validation behavior to repair a declaration-only defect.

### FIND-TASK-001-3 — Changed Rust items lack mandatory rustdoc

New declarations at `wyrd-cli/src/principal/mod.rs::assignment`, `wyrd-sdk-rust/src/lib.rs::otel`, `wyrd-sdk-python/src/lib.rs::principals`, `wyrd-sdk-ts/native/src/lib.rs::principals`, `wyrd-server/tests/integration/main.rs::pg_principal_roles`, and `wyrd-sql/src/queries/auth/role_assignments.rs::LIST_USER_ROLE_ASSIGNMENTS_SQL` lack item-level rustdoc. The SQL module has sibling new constants with the same issue. `AGENTS.md` §16 and `architecture/agent-rules.md` require substantive documentation for new or materially changed Rust items, including private and test items; the green lints did not check all of them.

Document the changed declarations in place with purpose, workflow role, and any relevant invariant or side effect. Inspect the complete cumulative changed Rust declaration set once for the same omission, including any new corrections in this remediation. Fallible changed functions require accurate `# Errors` sections. No new documentation checker or style-only task is needed.

### REQ-003A — One direct task has its own Verifier spec

Today `VerifierImplementation::Eval` always deserializes into `EvalSpec`:
`crates/wyrd-spec/src/vala/eval/spec.rs` requires an Eval `tasks` map and
permits dataset, sampling, pass gate, and DAG behavior. A standalone real-time
assertion or LLM judge is not an Eval workflow. Add a distinct typed
`VerifierImplementation::Task(TaskVerifierSpec)` variant. `TaskVerifierSpec`
is the only new struct for this implementation. It contains exactly one
`kind`-tagged definition directly under `implementation.spec`; there is no
`task` or `tasks` wrapper. Reuse
the existing `AssertionTask` and `LlmJudgeTask` payload types through a
`VerifierTask` enum; do not introduce replacement task payload structs. Reject
dependencies and
conditions that only make sense inside an Eval task graph, and reject unsupported
task kinds at deserialization or registration. Do not invent a second
assertion language, a new Card kind, or a `builtin` implementation.

The intended Rust contract follows the existing flattened `TriggerSpec`
pattern (the exact private placement is implementation-owned):

```rust
pub enum VerifierImplementation {
    Drift(DriftSpec),
    Eval(EvalSpec),
    Task(TaskVerifierSpec),
}

pub struct TaskVerifierSpec {
    #[serde(flatten)]
    pub definition: VerifierTask,
}

#[serde(tag = "kind", rename_all = "snake_case")]
pub enum VerifierTask {
    Assertion(AssertionTask),
    LlmJudge(LlmJudgeTask),
}
```

The required authored shape is:

```yaml
apiVersion: wyrd/v1
kind: Verifier
metadata:
  space: wyrd-team
  name: cohort-check
  version: "1.0.0"
spec:
  implementation:
    kind: task
    spec:
      kind: assertion
      id: cohort_valid
      context_path: $.cohort
      operator: in
      expected: [loyal_win_back, price_sensitive]
```

An LLM judge uses the same outer `kind: task` shape with
`implementation.spec.kind: llm_judge` and the existing `judge_ref`, comparison,
and retry fields. Keep `kind: eval` mapped only to `EvalSpec` and its internal
task graph. Ensure registration, reference discovery, schemas/OpenAPI, client
declarations, and direct result typing understand the new variant; use the
existing direct scorer rather than creating a parallel engine.

Add checked-in loader YAML and a parsing/registration test that loads the
complete Verifier Card, proves it deserializes as `TaskVerifierSpec`, and rejects
an Eval-only dependency or an extra nested `task:` wrapper. Extend existing
fixture paths rather than writing a separate YAML parser. Prove both
`assertion` and `llm_judge` variants deserialize.

### REQ-003A — Invoke permission is scoped to the selected Verifier

The current real-time route checks `Permission::eval_run()` (`evals:run`,
`All`) and then checks the subject against a Card-bound caller's observation
scope. That does not answer whether the caller may invoke this Verifier. Add
`verifier:run` with `PermissionScope::Verifier(CardUid)` for one exact Verifier
Card UID and support `All` for every Verifier in the tenant. Resolve the
selected Verifier in the tenant, authorize its UID before execution, and audit
that exact allow/deny
decision through the existing non-blocking audit path. An unrelated Verifier
UID grant, `evals:run` alone, or a binding alone must not authorize invocation.
Direct execution still requires an available subject Card in the same tenant;
the observation-attribution Card scope does not gate this request.

Apply the same Verifier permission rule to public manual invocation when it
selects a Verifier through a binding. Update the built-in `workload` grant to
`verifier:run`/`All` so existing local and signed-in journeys retain their
ability to verify. Update permission parsing, scope coverage, role expectations,
HTTP/MCP discovery and docs, SDK projections, and permission audit assertions.
Do not treat a Verifier binding as an implicit grant or weaken the tenant
boundary.

### REQ-003A — Agent, Service, and real-time client proof

Use the existing `agent1/eval.yaml` and `service.yaml` fixture pattern. Add
the task Verifier YAML as a sibling Card. In the checked-in fixtures, bind it
to a standalone Agent's `spec.verified_by` and to a Service's
`spec.verified_by`, both without `runs_on`; also exercise a Service component
binding where the real-time client resolves an Agent by alias. Keep the
existing Eval binding with `runs_on: { kind: observations_ready }` so one Agent
can have both an async Eval and an explicit-only task Verifier. The same
Verifier may be registered with no binding. Make `VerificationBinding.runs_on`
optional in this remediation so those checked-in bindings can load; TASK-003
then consumes that contract for its broader activation and result behavior.

The checked-in binding shapes are:

```yaml
# Agent.spec or Service.spec
verified_by:
  - verifier: ./cohort-check.yaml

# Service.spec.components entry for an Agent alias
components:
  - alias: agent1
    ref: agent1/agent.yaml
    verified_by:
      - verifier: agent1/eval.yaml
        runs_on:
          kind: observations_ready
      - verifier: agent1/cohort-check.yaml
```

Extend the existing Rust, Python, and TypeScript real-time journeys (or their
current shared fixture) so a client selects the bound task Verifier, sends JSON
context, receives the expected assertion verdict, and receives a denial with
only an unrelated Verifier UID grant or `evals:run`. Include an allowed call
from a Card-bound principal granted that Verifier UID, and show a foreign-tenant
Verifier cannot be invoked. The request must use the public SDK and real server
route, not call the scorer in-process. An LLM-judge task needs only a local mock
provider test, not live credentials. Preserve direct invocation of existing
Eval and Drift Verifiers under the new generic permission.

## Constraints and non-goals

- Preserve tenant isolation, permission/audit ordering, existing not-found behavior, and all accepted gateway, OTLP, verification, and local-login paths.
- Keep durable contracts in `wyrd-spec` and SDK projections in their owning packages. Do not hand-edit generated public `.pyi` output.
- Do not add Role aliases, principal direct permissions, an MCP principal surface, compatibility routes, another refresh path, a Rust gateway adapter, or a second attribution policy layer.
- The user excluded historical migration work from this review. Do not add a forward migration or historical-schema test in this task. External non-UUIDv7 cursor rejection and non-blocking structural notes are also outside this remediation.
- TASK-002 and TASK-003 own result persistence, activation behavior, and
  removal of publication flushes from older journeys. This remediation owns
  the optional binding shape needed for explicit-only Task Verifiers, the new
  task contract, invocation authorization, and focused journeys.

## Acceptance and focused proof

| Finding | Acceptance criterion | Focused proof |
|---|---|
| FIND-TASK-001-2 | Both SDK declarations expose the actual optional CardRef: present for a bound Service or Agent, null for an unbound Service or User. Python's non-null return remains a mapping; runtime values and Principal behavior stay the same. | Add a focused typed caller use of `card_ref` in the existing Python and TypeScript principal test/typecheck owners; run their exact named focused commands, `mise run py:typecheck`, the owning TypeScript typecheck, and `mise run codegen:check`. |
| FIND-TASK-001-3 | Every new or materially changed Rust declaration in the cumulative candidate has substantive rustdoc, including private constants and module/test declarations. | Inspect the cumulative Rust diff against `AGENTS.md` §16, then run `mise run fmt` and `mise run lints`. A test mirroring documentation presence is unnecessary. |
| REQ-003A task contract | Checked-in YAML for one assertion and one LLM judge deserializes into `TaskVerifierSpec`; `eval` still deserializes into `EvalSpec`; nested `task`, graph dependencies, and unsupported kinds fail. | Extend the existing loader/Card tests with exact named selectors; run the owning cards lane and `mise run codegen:check`. |
| REQ-003A binding and client | A task Verifier resolves from Agent, Service, and Service component bindings; a public client can invoke it with JSON context and receive its verdict through the real-time endpoint. | Extend the existing loader fixture and Rust/Python/TypeScript real-time journey owners, using the repository-managed server and exact focused commands for named tests. |
| REQ-003A permission | `verifier:run` for the selected UID or `All` authorizes invocation; unrelated UID, `evals:run` alone, foreign tenant, and missing permission refuse. Decisions are audited and `workload` remains functional. | Extend permission/scope and route integration tests, MCP visibility tests, built-in-role tests, and one client journey denial; run `mise run test:principals:integration` plus the narrow owning tests. |

Run `mise run fmt`, `mise run lints`, `mise run codegen:check`,
`mise run check:deps`, and the relevant Python/TypeScript format and typecheck
lanes for touched projections. Run focused loader, authorization, route, MCP,
and real-time SDK journeys through `mise`; record exact selectors for every
named test. Full unrelated Bifrost and gateway suites and broad aggregate
gates run once at change review.

## Implementation evidence

Commits: `b7dfc99b4`, `c5f5b92fc`, `284e4bc92`, `b28a69236`, `d075016ed`,
`7d3c83e32`, `d9f9f57af`, `1c0cd630a`, `cb135b313`, `a3e204818`.

The task kind is a standalone Verifier implementation. It is not an Eval. It
runs one check through the existing per-check executors,
`AssertionTaskExecutor::assert` and `JudgeTaskExecutor::judge`, and through the
existing Skald judge invoker (`EvalEngine::judge_invoker`). It has its own
pieces: `VerifierReport::Task`, `VerificationExecutionDetail::Task`, the
`task_context` input, and `VerifierKind::{TaskAssertion, TaskLlmJudge}`.
`Judgment.counts` is absent for a task. The queued runner refuses a task with
`implementation_unavailable`.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| FIND-TASK-001-2 | `b7dfc99b4`: Python and TS `PrincipalSummary.card_ref` typed as optional CardRef | `mise run py:typecheck`; `mise run ts:typecheck`; `mise run codegen:check` | PASS |
| FIND-TASK-001-3 | `c5f5b92fc`, plus rustdoc on every item touched since | `mise run fmt`; `mise run lints` | PASS |
| REQ-003A task contract | `crates/wyrd-spec/src/card/verifier.rs` (`TaskVerifierSpec`, `VerifierTask`, rejects `depends_on`/`condition`); `crates/wyrd-spec/tests/fixtures/verifier-task-{assertion,llm-judge}.yaml` | `mise exec -- cargo nextest run --locked -p wyrd-spec --lib -E 'test(=card::verifier_card_tests::task_verifier_yaml_deserializes_as_task_verifier_spec) \| test(=card::verifier_card_tests::task_verifier_rejects_graph_fields_wrappers_and_unknown_kinds)'`; lib lanes of wyrd-spec, wyrd-runtime, vala-eval, wyrd-client, and wyrd-loader (1371 passed); `mise run codegen:check` | PASS |
| REQ-003A binding (Agent, Service, component; no `runs_on`) | `crates/wyrd/wyrd-cli/tests/fixtures/loader/end_to_end/{agent1/cohort-check.yaml,agent1/agent.yaml,service.yaml}`; `runs_on` is optional; `freeze` skips explicit-only bindings | `mise exec -- cargo nextest run --locked -p wyrd-cli --test cli --run-ignored=all -E 'test(/^loader::end_to_end::/) \| test(/^card_lifecycle::/)'` with the Postgres wrapper (22 passed) | PASS |
| REQ-003A direct execution, assertion and mock LLM judge | `crates/wyrd/wyrd-server/src/verification/direct.rs` (`task`) | `pg_verification_routes::direct_execution_judges_inline_without_runs`, `::direct_execution_refusals_are_stable`, `::direct_execution_calls_the_judge_provider` via `mise exec -- cargo nextest run --locked -p wyrd-server --test integration -E 'test(/^pg_verification_routes::/)'` (9 passed) | PASS |
| REQ-003A permission | `Resource::Verifier`, `PermissionScope::Verifier(CardUid)`, `verifier_run`/`verifier_run_any`; `workload` and `editor` hold `verifier:run`/All; `VerificationControl::{execute,start_run}` authorizes the resolved Verifier UID and audits allow and deny; `VerifierRunQueue::target_cards` supplies the binding's Verifier | `mise exec -- cargo nextest run --locked -p wyrd-runtime --lib -E 'test(=permission::tests::verifier_run_is_uid_scoped_and_distinct_from_eval_run) \| test(/^builtin_roles::tests::/)'`; route tests above, including the evals-only, unrelated-UID, and exact-UID Card-bound cases and the manual-run evals-only denial; `pg_verifier_runs` (26 passed); `mise run test:principals:integration` (exit 0) | PASS |
| REQ-003A MCP discovery | `crates/wyrd/wyrd-server/src/mcp/verification.rs` gates visibility on `Resource::Verifier`/`Action::Run` | `mise exec -- cargo nextest run --locked -p wyrd-mcp --test mcp -P journey --run-ignored=all -E 'test(/^verification::pg_tests::/) \| test(=discovery::pg_tests::agent_discovers_only_authorized_tables_and_layout)'` (4 passed) | PASS |
| REQ-003A Rust client journey | `fixtures/cards/verify_in_real_time/{answer-check,answer-graded}.yaml` bound on the `agent` component without `runs_on` | `mise exec -- cargo nextest run --locked -p wyrd-sdk-rust --test integration -P journey --run-ignored=all -E 'test(/^verify_in_real_time::/)'`: `agent_answer_is_judged_by_its_task_check`, `graded_answer_passes_the_task_llm_judge`, `caller_without_the_verifier_grant_is_refused`, `exact_verifier_grant_verifies_only_that_verifier`, `another_tenant_cannot_verify_the_assistant`; Eval and Drift cases are kept (14 passed) | PASS |
| REQ-003A Python client journey | `sdks/wyrd-sdk-python/tests/integration/test_verify_in_real_time.py` | `uv run python -m pytest -q -m integration tests/integration/test_verify_in_real_time.py tests/integration/test_local_development.py` with the Postgres wrapper (15 passed); `mise run py:format`; `mise run py:lints`; `mise run py:typecheck` | PASS |
| REQ-003A TypeScript client journey | `sdks/wyrd-sdk-ts/wyrd/tests/integration/verify-in-real-time.test.ts`; the harness `scopedApiKey` accepts typed scoped permissions | `pnpm exec vitest run tests/integration/verify-in-real-time.test.ts tests/integration/local-development.test.ts` with the Postgres wrapper (15 passed); `mise run ts:format`; `mise run ts:lints`; `mise run ts:typecheck` | PASS |
| Docs | `docs/.../how-to/evaluate`, `how-to/observe`, `concepts/authorization`, regenerated `llms-full.txt` | `mise run docs:check` | PASS |
| Boundaries | none added | `mise run check:deps` (exit 0) | PASS |

Non-goals confirmed excluded: no migration, no historical-schema test, no Role
alias, no direct principal permissions, no MCP principal surface, no
compatibility route, no second refresh path, no Rust gateway adapter, no second
attribution layer, and no queued Task runs. `changes/active/verified-change-contract/*`
is not part of this candidate.
