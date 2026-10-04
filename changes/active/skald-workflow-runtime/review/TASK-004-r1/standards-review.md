# Repository Standards Review — TASK-004

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `b90335e0991438e8c28b65ed9c0c4cd80f9b0d56`
- Candidate: `96e993a16706d2fb759e4cdb7371ff490b198a35`
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-004-accepted-server-jobs.md`
- Approved specification: `changes/active/skald-workflow-runtime/spec.md`, revision 12
- Review mode: source, cumulative diff, and recorded evidence only. No build, test, Cargo, or mise command was run.
- Candidate identity was rechecked at the end of source inspection and remained `96e993a16706d2fb759e4cdb7371ff490b198a35`.

## Authority coverage

| Changed surface | Applicable authority read | Coverage and result |
|---|---|---|
| Shared-client Workflow hydration and local binding (`wyrd-client`) | `AGENTS.md` §§2–6, 9; `wyrd-design.md` Workflow and client contracts; `wyrd-doctrine.mdx`; `references/architecture/patterns.md`; `references/doctrine/architecture-constraints.md`; `references/languages/rust-core.md` | Thin composition remains in the shared client and delegates to Skald. No SQL, Vala engine, server owner, or second transport entered the client tier. PASS. |
| Runtime permission and builtin role grants | `AGENTS.md` §§2, 3, 9; `agent-rules.md` audit rules; `wyrd-security-posture.md` authorization; `references/languages/agent-harness.md`; `references/languages/errors.md` | `Resource::Workflows` is paired with existing `Action::Run`; writer and agent receive the permission at the canonical role owner, while request handlers use typed permission checks. PASS. |
| Skald Workflow graph bodies, preparation, execution transitions, and public surface | `AGENTS.md` §§3–6, 10, 16; `wyrd-design.md` Workflow; `wyrd-doctrine.mdx`; `references/architecture/patterns.md`; `references/languages/rust-core.md`; `maintainer-style.md` | Workflow execution remains in Skald, synchronous preparation was separated from async execution, and the owner remains a concrete executor/prepared-run shape. PASS except for the documentation rule captured in `STD-004-002`. |
| Server Workflow config, state, boot, routes, lifecycle, and private gateway adapter | `AGENTS.md` §§3–6, 9, 15–16; `wyrd-security-posture.md`; `references/architecture/patterns.md`; `references/languages/agent-harness.md`; `references/languages/errors.md`; `references/languages/testing-workflows.md` | Typed configuration, bounded state, structured public errors, trace-instrumented handlers, authenticated caller context, and process-local lifecycle are located in `wyrd-server`. PASS. |
| Cards graph pinning and authorized exact-reference read | `AGENTS.md` §§2–6, 9; `agent-rules.md` SQL/RLS/audit rules; `wyrd-design.md` Cards/Workflow references; `wyrd-security-posture.md`; `references/architecture/patterns.md` | Registry reads retain `TenantConn`, RLS, exact UID/body pinning, and the Cards audit boundary. No raw pool or client-side tenancy was added. PASS. |
| Built-in Workflow tools and shared bounded query collector | `AGENTS.md` §§2, 6, 9–11, 16; `agent-rules.md`; `wyrd-security-posture.md`; `references/languages/agent-harness.md`; `references/languages/errors.md`; `references/domain/olap-serving.md`; `references/domain/datafusion.md` | Input bounds, server-derived identity, Cards/Oracle authorization, stable error projection, and terminal-safe row collection are present. Output-contract and rustdoc failures are captured in `STD-004-003` and `STD-004-002`. |
| Oracle cancellation while opening, distributed cleanup, resource ownership, and analytical architecture | `AGENTS.md` §§10–12, 15–16; `agent-rules.md`; `bifrost-design.md` §§Query contract, distributed analytical execution, resource/failure invariants; `references/domain/vala-architecture.md`; `references/domain/olap-serving.md`; `references/domain/datafusion.md`; `references/domain/analytical-operations-reliability.md`; `implementation-execution.md` | The narrow `open_cancellable` change follows the existing lifecycle owner. The broader removal of child-drain/accounting enforcement and its tests violates the authoritative structured-cancellation and test-integrity rules; see `STD-004-001`. FAIL. |
| Gateway ingress and external gateway binding | `AGENTS.md` §§9–10; `agent-rules.md` SSRF rule; `wyrd-security-posture.md` external-network safety; `references/architecture/patterns.md` provider/external network patterns | The Workflow adapter enters the existing gateway invocation owner, and external bindings carry references rather than plaintext credentials. No new fetch implementation or alternative SSRF mechanism was added. PASS. |
| HTTP/OpenAPI/MCP projections and stable errors | `AGENTS.md` §§2, 9, 11; `references/languages/agent-harness.md`; `references/languages/errors.md`; `references/languages/testing-workflows.md` | HTTP handlers use typed request/response bodies, `WyrdError`, served OpenAPI annotations, and tracing. MCP reuses the extracted collector. PASS except for the Workflow AgentTool output schemas in `STD-004-003`. |
| Fixtures, test configuration, integration tests, and multi-pod journey | `AGENTS.md` §11–12; `agent-rules.md` test location/load/gate integrity; `references/languages/spec-driven-development.md`; `references/languages/implementation-execution.md`; `references/languages/testing-workflows.md` | The server/Oracle tests are in justified external binaries and use repository fixtures. The `pg-servers` nextest group uses the same standard bounded test-group mechanism already present for journey/Postgres suites, so it is not novel drift. Deleting/weakening Bifrost invariant tests is a standards violation; see `STD-004-001`. |
| Architecture and task evidence edits | `AGENTS.md` §§1–2, 12, 14–15; `bifrost-design.md`; `references/languages/spec-driven-development.md`; `references/languages/implementation-execution.md` | Recorded diagnostics are detailed, but an implementation task cannot approve a material Bifrost resource/cancellation semantic change by editing the authority and deleting its tests. FAIL under `STD-004-001`. |

## Rule-by-rule audit

| Rule | Source evidence | Result |
|---|---|---|
| Core behavior remains with its established owner; no duplicate engine or transport | `skald-workflow::PreparedWorkflowRun` owns prepared execution; `WorkflowRunHost` wraps it; `ServerWyrdGatewayCaller` calls `GatewayInvocation`; `BoundedQuery` calls `query::service::stream_query`. | PASS |
| Struct-centered Rust style | `WorkflowRunHost`, `WorkflowRuns`, `Reservation`, `AcceptedRun`, `RunTools`, `BoundedQuery`, and `ResultCollector` own cohesive state and operations. | PASS |
| Async only for IO or intentional async composition | Pure Workflow preparation is `Workflow::prepare`; async host, gateway, query, drain, and route operations await IO/tasks. | PASS |
| Typed domain identifiers and public wire contracts | `DataTenantId`, `PrincipalId`, `WorkflowRunId`, `IdempotencyKey`, `CardRef`, and typed Workflow request/result types are used across the new surface. | PASS |
| Stable cross-boundary errors use the Wyrd catalog | Routes and tool projection use `WyrdError`; no parallel code/status/remediation implementation was added. | PASS |
| Tenant identity comes from verified caller context; SQL uses the approved connection boundary | `WorkflowRunHost` keys on `caller.data_tenant_id` and principal; Cards resolution uses `&mut TenantConn<'_>`; no raw pool crosses the new production interfaces. | PASS |
| Every permission decision uses the canonical audit owner | Workflow create/get/cancel call `audit::authorize`; Cards and Bifrost tools delegate to their existing authorized/audited owners; gateway calls keep the gateway exception. | PASS |
| Secrets are references, redacted, and resolved only at the execution owner | `ServerExternalGatewayBindingConfig` contains `ExternalGatewayBindingConfig`/`SecretRef`; the Workflow Card/input does not carry credential bytes. | PASS |
| Server handlers are typed, traced, bounded, and do not construct pools/clients | `components/workflow/routes.rs:24-179`; config bounds in `config.rs:1657-1804`; server state owns the run host. | PASS |
| Agent-facing tools have typed inputs and outputs | Inputs are closed and validated, but both new outputs are advertised only as `{ "type": "object" }` despite known response contracts. | **FAIL — `STD-004-003`** |
| Every new/materially modified Rust item has substantive rustdoc | New `AgentTool` implementation methods in `components/workflow/tools.rs` lack item rustdoc. | **FAIL — `STD-004-002`** |
| Cancellation is structured: stop admission, cancel descendants, join work, then release exactly once | Candidate releases an analytical graph after live attempts disappear without proving nested memory/cache/transport descendants are joined, and edits the architecture to allow that. | **FAIL — `STD-004-001`** |
| Never delete or weaken a required test to clear a failure | Recorded evidence names two removed cleanup-invariant tests and changes the surviving resource test to accept the previously rejected state. | **FAIL — `STD-004-001`** |
| New mechanisms/checks/settings must use established or widely used mechanisms | `TaskTracker`, cancellation tokens, watch channels, nextest test groups, typed config defaults, and existing gateway/query/Card seams are established repository/native mechanisms. No material unsupported novelty was found. | PASS |
| User/agent-facing behavior has a real server journey and negative/edge proof | `pg_workflow_runs.rs` covers HTTP create/replay/get/cancel, authority, route, tool, race, bounds, and shutdown paths; `wyrd-testing/tests/bifrost/oracle/workflow.rs` covers the forwarded multi-pod query path. | PASS, subject to `STD-004-001` test-integrity failure |
| OpenAPI, codegen, boundary, format, lint, and affected suites have recorded proof | Task evidence records `test:wyrd`, `test:shared`, principals, gateway journey, Bifrost lanes, codegen, client-tier, tenant isolation, unwrap audit, fmt, lints, and focused selectors. | PASS as recorded evidence; not independently executed in this review |

## Review findings

### Critical

#### STD-004-001 — VIOLATION: distributed Oracle cleanup was weakened by changing authority and removing the tests that enforced it

- Violated rules:
  - `architecture/bifrost-design.md:849-850`: cancellation is structured — cancel descendants, join work, and release ownership exactly once.
  - `architecture/bifrost-design.md:382-391` and `:411-413`: follower/leader graphs own their descendants and release after structured work joins; head cancellation joins every descendant.
  - `AGENTS.md` §12 and `architecture/references/languages/implementation-execution.md` “Implementation and test integrity”: never pass a gate by deleting or weakening a required test or by changing semantics to fit a failure.
  - `architecture/references/languages/implementation-execution.md` “Material”: stop for a redesign of ownership or resource/cancellation semantics; a task execution record cannot approve that change.
- Locations:
  - `architecture/bifrost-design.md:401-409`
  - `crates/vala/vala-bifrost-redux/src/resources.rs:3761-3817` (changed `OracleQueryResources::release`)
  - `crates/vala/vala-bifrost-redux/src/oracle/analytical_supervisor.rs:972-1007` (`release_graph`)
  - `crates/vala/vala-bifrost-redux/src/oracle/admission.rs` (removed `drain_children`/child-idle proof)
  - `changes/active/skald-workflow-runtime/tasks/TASK-004-accepted-server-jobs.md:579-609` (the implementation record explicitly says the drain loops, release refusal, poison check, and two invariant tests were deleted)
- Evidence: the base authority required every memory, slot, task, cache, and transport owner to release only “after the graph drains.” The candidate changes that sentence to allow graph release “without waiting for nested children still being torn down.” The production release path now rejects only live supervisor attempts before removing the graph; it no longer proves nested memory children or equivalent descendant ownership are gone. The candidate also removes `OracleQueryResources::nested_idle`, deletes the release-time surviving-child refusal/poison, removes the tests `assert_a_live_envelope_child_fails_settlement` and `follower_retains_a_graph_whose_children_never_drain`, and rewrites `oracle_release_paths_are_exact_and_idempotent` to accept a late child. Those are not merely fixture changes: they alter the Bifrost resource-owner contract to make a failure pass.
- Observable consequence: Oracle can publish graph cleanup and return the query slot while descendant memory/cache/transport work still exists. The child may keep root memory charged after admission says the query ended, so a queued query can be admitted against a slot whose descendant resource ownership has not settled. Cleanup residue also stops being attributed to the graph/readiness owner that the authority requires. Even if the late child eventually drops, the candidate no longer proves the structured lifetime; if it does not drop, only aggregate root memory remains, with the graph identity and cleanup failure already discarded.
- Testable correction: restore the authoritative “join descendants before release” behavior, the child/accounting refusal at the owning graph/resource boundary, and the removed invariant tests. Fix the producer of the late child using the established structured-task/cancellation owner (including the existing detached worker/cache-cycle work if relevant), so normal cancellation actually joins or retains attributable residue. Do not add another timeout, check, setting, or downstream guard, and do not change `bifrost-design.md` merely to bless an unjoined child.

### Important

#### STD-004-002 — VIOLATION: new Rust trait-implementation methods lack required rustdoc

- Violated rule: `AGENTS.md` §16 and `architecture/agent-rules.md` require substantive rustdoc on every new or materially modified Rust item, including private methods and trait implementation methods; missing documentation is `BLOCK_BEFORE_MERGE`.
- Locations:
  - `crates/wyrd/wyrd-server/src/components/workflow/tools.rs:147`
  - `crates/wyrd/wyrd-server/src/components/workflow/tools.rs:151`
  - `crates/wyrd/wyrd-server/src/components/workflow/tools.rs:155`
  - `crates/wyrd/wyrd-server/src/components/workflow/tools.rs:159`
  - `crates/wyrd/wyrd-server/src/components/workflow/tools.rs:238`
  - `crates/wyrd/wyrd-server/src/components/workflow/tools.rs:242`
  - `crates/wyrd/wyrd-server/src/components/workflow/tools.rs:246`
  - `crates/wyrd/wyrd-server/src/components/workflow/tools.rs:261`
- Evidence: `QueryTool` and `CardsTool` implement `AgentTool::name`, `description`, `input_schema`, and `output_schema` without item documentation. Adjacent new methods such as both `invoke` implementations do carry workflow/error rustdoc, showing these are new candidate-owned items rather than untouched drift.
- Consequence: the candidate fails an explicit repository merge blocker, and maintainers cannot tell from the implementation items which parts of each tool contract are stable projections versus internal convenience values.
- Testable correction: add concise, substantive rustdoc to the eight methods explaining the stable name/description and the input/output contract each projects. No new abstraction, lint allowance, or documentation check is needed.

#### STD-004-003 — INCORRECT: the two new AgentTool output contracts are not typed to their actual results

- Violated rules:
  - `architecture/references/languages/agent-harness.md` “Tool Contracts”: agent tools have typed inputs and outputs.
  - `architecture/references/architecture/patterns.md` “Agent Surface Pattern”: agent-facing paths project the same typed request/response contract.
  - `AGENTS.md` §9: public request/response bodies are typed; clients and agent surfaces do not invent weaker parallel shapes.
- Locations:
  - `crates/wyrd/wyrd-server/src/components/workflow/tools.rs:159-161`
  - `crates/wyrd/wyrd-server/src/components/workflow/tools.rs:261-263`
  - Actual query result contract: `crates/wyrd/wyrd-server/src/query/collect.rs:287-604`
- Evidence: `bifrost.query` advertises only `{ "type": "object" }`, although its implementation and description promise the closed `{columns, rows, terminal}` response and the shared collector constructs that exact shape. `cards.get` likewise advertises only an unconstrained object although it returns the canonical Card envelope. The inputs are deliberately closed and bounded, so the weak outputs are not an intentional open-boundary pattern. No test asserts either output schema.
- Observable consequence: an agent/provider cannot validate or reason from the tool definition about the fields it will receive, and schema consumers accept objects that the implementation never returns. The Workflow tool projection is therefore weaker than the typed owner contract it claims to reuse.
- Testable correction: project the existing canonical result schemas into `output_schema`: a closed schema for the collector’s `columns`, `rows`, and `terminal` object, and the existing Card envelope schema (or its established runtime schema projection) for `cards.get`. Reuse the existing schema owners; do not introduce a parallel handwritten contract or a new generator/check. Add a focused schema assertion beside the owning tool tests.

### Suggestions

None. Optional refactors and preferences are outside this acceptance review.

## Open questions

None. The findings resolve from current repository authority and source without a new product or architecture decision.

## Verification notes

- Per the review instruction, no build, test, Cargo, or mise command was run.
- The task’s recorded evidence reports successful focused Workflow scenarios, server/shared/principals/gateway suites, codegen, client-tier, tenant-isolation, unwrap audit, formatting, and lints. It reports eight of nine `test:bifrost` lanes before the final D8 assertion adjustment and a successful server journey rerun afterward.
- Green recorded commands do not override `STD-004-001`: the execution record itself establishes that cleanup-invariant tests were removed/weakened to accept changed resource semantics.
- `git diff --check` was inspected read-only and reported no whitespace error.

## Overall result

**FAIL**

Repository standards are not satisfied because the candidate materially weakens Bifrost structured cleanup and test integrity, omits mandatory rustdoc on new Rust items, and exposes untyped output schemas for the new agent tools.
