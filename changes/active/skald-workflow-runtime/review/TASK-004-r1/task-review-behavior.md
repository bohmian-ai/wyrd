# TASK-004 Behavior Review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `b90335e0991438e8c28b65ed9c0c4cd80f9b0d56`
- Candidate: `96e993a16706d2fb759e4cdb7371ff490b198a35`
- Approved specification: `changes/active/skald-workflow-runtime/spec.md`, revision 12
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-004-accepted-server-jobs.md`
- Review method: static inspection of the complete base-to-candidate diff, candidate source, and recorded evidence only. No build, test, Cargo, or mise command was run.
- Candidate identity was rechecked after review and remained unchanged.

## Review Findings

### Critical

None.

### Important

#### BEH-004-001 — MISSING: admission and captured-authority journeys omit required trust-boundary cases

- Violated obligations: REQ-029, REQ-032, REQ-032A, AC-004, AC-009, AC-027, Scenario 1, and Scenario 3.
- Locations:
  - `crates/wyrd/wyrd-server/tests/pg_workflow_runs.rs:868-1083`
  - `crates/wyrd/wyrd-server/tests/pg_workflow_runs.rs:1221-1300`
  - `crates/wyrd/wyrd-server/src/components/workflow/host.rs:75-123`
  - `crates/wyrd/wyrd-server/src/components/cards/resolve.rs:566-619`
- Evidence:
  - `admission_is_audited_and_side_effect_free_on_refusal` checks unauthenticated and role-denied callers, malformed requests, unknown versions, unsupported routes/tools, missing bindings, oversized input, capacity, and audit failure. It does not submit an inactive Workflow or a Workflow reference belonging to another tenant.
  - `accepted_authority_outlives_submission_only` proves token expiry and grant revocation after acceptance, plus current authorization on later get/cancel/replay. It does not mutate or deactivate a Card after acceptance, exercise a resource outside the captured scope, change a credential after acceptance, or demonstrate that a newly authorized replay cannot expand the accepted execution authority.
  - The recorded material limits explicitly state that live gateway deployment removal is not journeyed.
- Observable consequence: the required user-journey evidence does not establish that graph admission refuses inactive/cross-tenant roots or that later Card, credential, gateway-admission, and grant changes preserve the exact accepted-job authority boundary. Those are explicit security and pinned-graph behaviors, not optional hardening.
- Required correction: extend the existing admission and accepted-authority journeys using the current registry, role, token, gateway, and deterministic upstream fixtures. Prove inactive and foreign-tenant Workflow refusal before provider/tool work; mutate or deactivate a dependency after acceptance and show the pinned run is unchanged; show a newly granted resource remains inaccessible to the accepted run and replay does not refresh authority; and remove or invalidate live gateway eligibility/credentials before a later step to prove the gateway owner’s current admission still refuses it.
- Focused proof: the two existing exact Scenario 1 and Scenario 3 selectors, retaining their present audit and provider-call assertions.

#### BEH-004-002 — MISSING: idempotency/preparation evidence does not cover the complete AC-021 lifecycle

- Violated obligations: REQ-030, REQ-034C, INV-018, AC-021, and Scenario 2.
- Locations: `components/workflow/runs.rs:249-329,458-560`, `components/workflow/host.rs:99-123`, and `tests/pg_workflow_runs.rs:1096-1207`.
- Evidence: the journey proves concurrent same-principal requests share one preparation, changed requests conflict, different principals isolate the key, failed preparation is not cached, and creator cancellation does not strand preparation. It does not exercise tenant scoping, cancel a matching waiter while peers survive, prove all shutdown waiters wake exactly once, or simulate lost acceptance-response recovery without duplicate observations/provider calls.
- Observable consequence: tenant-scoped keys, independently cancellable waiters, shutdown publication, and lost-response recovery remain unproved; regressions can duplicate work, leak capacity, or strand callers.
- Required correction: extend the existing Scenario 2 journey with the current preparation gate, cancellation, shutdown, tenant fixtures, audit/observation inspection, and upstream counters. No new harness, scheduler, setting, or check is required.

#### BEH-004-003 — MISSING: built-in tool evidence omits required authorization and terminal-negative paths

- Violated obligations: REQ-052, INV-006, INV-008, INV-022, AC-020, AC-025, and Scenario 4.
- Locations: `components/workflow/tools.rs:145-290`, `query/collect.rs:216-330`, `tests/pg_workflow_runs.rs:1316-1489`, and `wyrd-testing/tests/bifrost/oracle/workflow.rs:76-254`.
- Evidence: happy paths and several negatives are covered, but oversized SQL, numeric bounds, inaccessible/foreign Card and table resources, independent tool permissions, and malformed terminal/partial-row failure through the real Agent path are not. The `tenant` JSON case proves schema closure, not tenant isolation.
- Observable consequence: authorization, argument-bound, or terminal-integrity regressions could leak data through the Agent path while the journey remains green.
- Required correction: extend Scenario 4 using existing fixtures and owners; do not add a second query engine or tool harness.

#### BEH-004-004 — MISSING: the new server gateway adapter lacks its required protocol, fallback, and Vertex proof

- Violated obligations: REQ-034, REQ-036A, REQ-038, REQ-039, REQ-041, REQ-042, REQ-043, INV-010, INV-012, INV-020, AC-011A, AC-014, AC-015, AC-016, AC-017, and Scenario 5.
- Locations: `components/gateway/workflow.rs:81-250` and `tests/pg_workflow_runs.rs:1502-1602`.
- Evidence: the new adapter projects OpenAI Chat/Responses, Anthropic, Gemini, and Vertex plus fallback, timeout, and cancellation, but the journey exercises only one OpenAI-shaped governed route and one OpenAI-compatible external binding. Public ingress journeys cannot directly prove this new adapter.
- Observable consequence: protocol projection, model placement, fallback isolation, deadline, or cancellation can be wrong specifically in the new in-process adapter.
- Required correction: extend the existing Scenario 5 journey using current controlled fixtures; add no new transport, option, compatibility surface, or bespoke check.

#### BEH-004-005 — MISSING: lifecycle evidence does not prove global eviction, snapshot replacement, queued shutdown, or deadline races

- Violated obligations: REQ-018 through REQ-023, REQ-034A through REQ-034C, REQ-048, REQ-050, INV-013, INV-018, INV-019, AC-018, AC-022, and Scenario 6.
- Locations: `components/workflow/runs.rs`, `components/workflow/host.rs:144-173`, and `tests/pg_workflow_runs.rs:1617-1796`.
- Evidence: explicit cancellation, one short deadline, one completion/cancel race, per-tenant eviction, expiry, hidden-run 404, restart loss, and shutdown with running/preparing work are covered. Global eviction, GET during replacement, completion/deadline, accepted queued shutdown, and shutdown terminal snapshot inspection are not.
- Observable consequence: visible lifecycle regressions remain outside the gate.
- Required correction: extend the existing deterministic Scenario 6 journey with current lifecycle gates and test-server shutdown path.

#### BEH-004-006 — MISSING: graph and snapshot bounds are only partially proved

- Violated obligations: REQ-017, REQ-045, REQ-050, INV-019, INV-023, AC-028, and Scenario 7.
- Locations: `components/cards/resolve.rs:566-619`, Skald `RunLedger`, `components/workflow/host.rs:373-387`, and `tests/pg_workflow_runs.rs:1810-1933`.
- Evidence: step, edge, graph, input, and one step-result bound are covered. A sibling Bifrost query, deep admissible graph, aggregate run overflow, and near-limit terminal reserve for failure/cancellation/timeout are not.
- Observable consequence: stack growth, Bifrost starvation, aggregate accounting, or insufficient terminal reserve can escape the journey.
- Required correction: extend Scenario 7 with existing Bifrost fixtures. Do not add a CPU pool, scheduler, transport surrogate, setting, or bespoke check.

## Acceptance Matrix

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Exact config/defaults/validation and fixed retention | `config.rs`; `runs.rs` | Config unit and Scenario 1 | PASS |
| `workflows:run`, builtin grants, canonical audit | runtime permissions/roles; `host.rs` | Scenario 1 | PASS |
| Active exact graph, tenant isolation, refusal side effects | `host.rs`; `cards/resolve.rs` | Inactive/foreign tenant omitted | FAIL — BEH-004-001 |
| One preparation/job per scoped key | `runs.rs`; `host.rs` | Partial Scenario 2 | FAIL — BEH-004-002 |
| Accepted authority outlives token only | captured `Caller`; tools/gateway | Partial Scenario 3 | FAIL — BEH-004-001 |
| Built-in tools retain owner auth/audit/limits | tools/cards/query owners | Partial Scenario 4 | FAIL — BEH-004-003 |
| Query owner survives waiter abort and settles | tools/host/query/Oracle lifecycle | Local and forwarded recorded evidence | PASS for covered causes |
| Gateway/external ownership separation | gateway workflow adapter; host | OpenAI basic ownership | PASS for basic ownership |
| All protocols, Vertex, fallback, deadline/cancel | new gateway projections | Not directly journeyed | FAIL — BEH-004-004 |
| Snapshots, terminal races, global eviction, shutdown | runs/host/app server | Partial Scenario 6 | FAIL — BEH-004-005 |
| Graph/run bounds, stack safety, reserve, siblings | cards resolve, RunLedger, host | Partial Scenario 7 | FAIL — BEH-004-006 |
| Registration provenance and Service hydration | existing owners extended | Recorded shared/wyrd/CLI/codegen evidence | PASS |
| Prohibited durable/remote/principal machinery absent | Complete diff | Static inspection | PASS |
| No unsupported bespoke mechanism under standing DRIFT direction | Existing/native mechanisms used | Static inspection | PASS |
| Recorded verification only | Task records green commands | Not rerun | PASS subject to coverage gaps |

## Verification Notes

- No builds, tests, Cargo commands, or mise tasks were run.
- The review relied on candidate-recorded results and inspected what named tests assert.
- Candidate remained `96e993a16706d2fb759e4cdb7371ff490b198a35`.
- No nonstandard DRIFT mechanism was proposed; corrections reuse existing owners, fixtures, and standard tooling.

## Overall Result

**FAIL**

Proposed finding IDs: `BEH-004-001` through `BEH-004-006`.
