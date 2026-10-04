# TASK-004 r5 repository standards review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `b90335e0991438e8c28b65ed9c0c4cd80f9b0d56`
- Candidate: `d4d4e2da53abfc677abdb804e71517c3b6849f49`
- Approved authority: `changes/active/skald-workflow-runtime/spec.md`, Revision 13
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-004-accepted-server-jobs.md`
- Remediations: TASK-004 R1 through R4 in the preceding review directories
- Review mode: static and strictly read-only. No build, test, Cargo, `mise`,
  formatter, code-generation, or executable verification command was run.

The candidate remained the named commit when this report was written. The
repository has no `.codegraph/` index, so navigation used the committed
cumulative diff, current source, callers, manifests, generated artifacts, and
repository-native text search.

## Authority coverage

| Changed surface | Applicable authority read and applied | Coverage result |
|---|---|---|
| Approved specification, task, four remediations, and recorded evidence | `AGENTS.md` §§1, 11-16; `architecture/agent-rules.md`; `architecture/references/languages/spec-driven-development.md`; `implementation-execution.md`; `maintainer-style.md` | PASS — the active artifacts consistently identify Revision 13 and the immutable cumulative subject; review and implementation records remain evidence rather than production enforcement machinery |
| Shared-client Workflow loading and local binding | `AGENTS.md` §§2-6, 9; `architecture/wyrd-design.md` client and Workflow contracts; `architecture/wyrd-doctrine.mdx`; `architecture/references/doctrine/architecture-constraints.md`; `architecture/patterns.md`; `languages/rust-core.md` | PASS — the shared client composes the existing loader, Cards hydration, and Skald owner without importing a server owner, Vala engine, SQL implementation, or second transport |
| Permission, built-in roles, accepted authority, audit, and tenancy | `AGENTS.md` §§2-4, 9; `architecture/agent-rules.md`; `architecture/wyrd-design.md` runtime identity and accepted-job contract; `architecture/wyrd-security-posture.md`; `architecture-constraints.md`; `architecture/patterns.md`; `languages/agent-harness.md`; `languages/errors.md` | PASS — verified caller identity supplies tenant and principal, create/get/cancel use the canonical authorization/audit owner, and Cards, Bifrost, and gateway calls retain their live owner decisions without a second audit path or retained bearer secret |
| Skald provider request, Prompt schemas, Workflow planning/execution, snapshots, attempts, and prepared deadline | `AGENTS.md` §§3-6, 9-12, 16; `architecture/wyrd-design.md` Prompt and Workflow sections; `architecture/wyrd-doctrine.mdx`; `architecture/patterns.md`; `languages/rust-core.md`; `maintainer-style.md`; `languages/errors.md` | PASS — provider/body discrimination remains a typed Skald contract; `WorkflowExecutor` remains the scheduling/lifecycle owner; attempt one is established before `Running`; `WorkflowExecutor::new` owns the one absolute deadline and `PreparedWorkflowRun::deadline` exposes that same value |
| Server Workflow routes, host, process-local run table, configuration, retention, and shutdown | `AGENTS.md` §§3-6, 9, 11-12, 16; `architecture/wyrd-design.md`; `architecture/wyrd-security-posture.md`; `architecture/patterns.md`; `languages/rust-core.md`; `languages/errors.md`; `languages/testing-workflows.md` | PASS — typed and instrumented handlers delegate to cohesive `WorkflowRunHost`/`WorkflowRuns` owners; state is bounded and process-local; stable errors, request context, cancellation, retention, and tracked shutdown remain server-owned |
| Cards graph pinning and exact-reference reads | `AGENTS.md` §§2-6, 9; `architecture/agent-rules.md` SQL, RLS, transaction, import, and audit rules; `architecture/wyrd-design.md` Card/Workflow references; `architecture/wyrd-security-posture.md`; `architecture/patterns.md` | PASS — graph resolution uses the established Cards/registry owner and `TenantConn`; caller tenancy is not accepted from a payload and no raw pool or parallel tenant predicate enters the production path |
| Built-in Workflow tools and shared bounded query collector | `AGENTS.md` §§3, 5-6, 9-12, 16; `architecture/bifrost-design.md`; `architecture/references/domain/vala-architecture.md`; `olap-serving.md`; `analytical-operations-reliability.md`; `languages/agent-harness.md`; `languages/errors.md` | PASS — tool contracts are closed and typed, results are bounded and terminal-safe, and the R4 correction projects the prepared run's exact deadline through the already shared `RunTools` clones before acceptance (`host.rs:374-400`, `tools.rs:43-95,188-215`) |
| Gateway invocation and external binding | `AGENTS.md` §§3, 9-10; `architecture/agent-rules.md` SSRF rule; `architecture/wyrd-security-posture.md`; `architecture/patterns.md` provider/external-network patterns | PASS — the private Workflow adapter enters the existing gateway owner, credentials remain secret references resolved at the owner, and no alternate fetch, transport, authorization, or SSRF mechanism was introduced |
| Oracle query opening, cancellation, graph settlement, resources, and peer lifecycle | `AGENTS.md` §§3, 6, 10-12, 16; `architecture/bifrost-design.md`; `architecture/references/domain/vala-architecture.md`; `olap-serving.md`; `analytical-operations-reliability.md`; `languages/rust-core.md` | PASS — the candidate preserves the directed authority: graph-drain polling and supervisor idle refusal stay deleted, follower release is grant-stream close, the leader awaits no follower release acknowledgement, and cancellation/settlement remains under the original query deadline |
| HTTP/OpenAPI/MCP and generated contract projections | `AGENTS.md` §§2, 8-11; `architecture/wyrd-design.md`; `architecture/patterns.md`; `languages/agent-harness.md`; `languages/errors.md`; `languages/testing-workflows.md` | PASS — typed route and tool contracts reuse their owners, public errors remain catalog-backed, Prompt schemas uniformly project the source tagged request, and no generated artifact becomes an independent source of truth |
| Rust, Python, server/Postgres, and multi-pod tests; nextest and manifest changes | `AGENTS.md` §§4, 11-12, 16; `architecture/agent-rules.md`; `languages/testing-workflows.md`; `spec-driven-development.md`; `implementation-execution.md`; `languages/rust-core.md` | PASS — external binaries earn their placement by driving real server/Postgres or authenticated Oracle peers; focused owner logic stays in source modules; Python tests remain in the Python runtime; nextest test groups use the repository's established native resource-group facility; the R4 unit proof reuses the existing in-module test shape |

## Applicable rule results

| Repository rule | Exact source evidence | Result |
|---|---|---|
| Durable behavior stays with its established owner | Skald execution remains in `crates/skald/skald-workflow/src/workflow.rs:140-375`; accepted-job ownership is in `components/workflow/{host,runs}.rs`; Bifrost collection delegates through `query/collect.rs`; Oracle mechanics remain under `vala-bifrost-redux` | PASS |
| Stateful workflows use cohesive concrete structs and inherent methods | `WorkflowExecutor`, `RunLedger`, `WorkflowRunHost`, `WorkflowRuns`, `RunTools`, `BoundedQuery`, and `ResultCollector` own their state/dependencies; free functions are narrow conversions, schema builders, or error projections | PASS |
| Async is limited to IO or intentional async composition | Workflow preparation and deadline binding are synchronous; execution, database/registry, gateway, query, stream, tracker, and route operations await real IO or structured tasks | PASS |
| Interface types are imported at module scope and used by bare name | The R2/R3 corrections remain present; the R4 interfaces import `Instant`, `PreparedWorkflowRun`, and `OnceLock` at module scope (`workflow_surface.rs:8-17`, `tools.rs:13-28`) and use bare names in fields/signatures | PASS |
| New/materially changed Rust items have substantive rustdoc | `WorkflowExecutor::deadline` (`workflow.rs:205-209`), `PreparedWorkflowRun::deadline` (`workflow_surface.rs:689-698`), `RunTools.deadline`, `new`, `bind_deadline`, and the R4 proof (`tools.rs:43-123,376-472`) document role and invariant; fallible/cancellable operations retain their required sections | PASS |
| Public handlers use typed payloads, stable errors, and scrubbed tracing | `components/workflow/routes.rs:24-180` uses typed Wyrd request/response types, catalog-backed `WyrdError`, and `#[tracing::instrument(skip_all, ...)]` on create/get/cancel | PASS |
| Verified identity supplies tenancy and authorization decisions use the canonical audit owner | `components/workflow/host.rs:60-203` derives run scope from `Caller`, calls `audit::authorize`, and only then accesses tenant/principal-scoped process state; Cards/query/gateway calls delegate to their existing authorized owners | PASS |
| Accepted execution retains no bearer secret and does not widen authority | The run captures caller attribution/scopes, while live tool owners re-evaluate their own decisions; no token or credential value is stored in `WorkflowRuns`, `RunEntry`, or `RunTools` | PASS |
| Agent-facing tool inputs/outputs, sizes, deadlines, and cancellation are typed and bounded | `components/workflow/tools.rs:156-327` reuses the closed query schemas and canonical Card schema, clips result bytes, uses the prepared deadline, creates child cancellation, and awaits a tracked query owner | PASS |
| One prepared deadline owns the run and its built-in queries | `WorkflowExecutor::new` samples with checked arithmetic (`workflow.rs:140-197`); `PreparedWorkflowRun::deadline` returns that value (`workflow_surface.rs:672-698`); host binding happens before reservation acceptance (`host.rs:374-400`); each clone reads the shared `OnceLock` (`tools.rs:43-95,205-215`) | PASS |
| Provider wire contracts remain explicit and generated projections follow source | `skald-spec/src/request.rs` owns the adjacent provider/body tag; provider clients construct explicit variants; Prompt schema, fixtures, examples, and SDK tests move with that source; recorded codegen evidence is green | PASS (recorded evidence) |
| Tenant SQL uses the approved connection boundary and caller-owned transaction lifecycle | Cards graph pinning receives `TenantConn`; production signatures do not accept raw pools; `Preparation::prepare` commits the transaction it opened at `host.rs:321-332` rather than a callee ending a caller-owned transaction | PASS |
| No gate is weakened or bypassed | The cumulative candidate adds no production `#[allow]`, disabled required test, compatibility reader, alternate audit writer, duplicate query engine, graph-drain poller, supervisor idle refusal, or follower-release acknowledgement | PASS |
| New mechanism/check/file/setting/option is established or broadly standard | `TaskTracker`, cancellation tokens, `OnceLock`, typed config, nextest test groups, and in-module Tokio paused-time tests are standard-library/runtime or existing repository mechanisms. No bespoke checker, service, protocol, timer task, channel, retry, poll, setting, or dependency was introduced by R4 | PASS |
| User/agent-facing behavior has production-shaped journey coverage | `wyrd-server/tests/pg_workflow_runs.rs` drives the HTTP/server/Postgres boundary; `wyrd-testing/tests/bifrost/oracle/workflow.rs:80-233` drives the authenticated multi-pod query path and now supplies a post-tool continuation that would expose an early tool-only deadline | PASS (recorded execution evidence) |
| Test runtime and placement follow repository taxonomy | Server/Postgres and peer-cluster journeys remain earned external test binaries; `components/workflow/tools.rs:353-472` is an in-module Rust-only owner test; Python behavior remains in Python tests | PASS |
| Human-fixed boundaries remain excluded | No Oracle graph-drain polling or idle refusal is restored; no follower release acknowledgement is added; foreign-tenant model execution is not required from an unseeded fixture; published `Running` reserves attempt one and interrupted work settles `Cancelled`; built-in query tools bind once to the prepared deadline | PASS |

## Material findings

None.

The cumulative candidate contains no material repository-rule violation. The
R4 correction uses the existing prepared-run owner plus the standard-library
`OnceLock` already shared by `RunTools`; it does not create a deadline service,
second clock owner, configuration option, background timer, channel protocol,
dependency, checker, or harness. The earlier standards-relevant documentation,
typed-tool-schema, import-shape, provider-contract, and attempt-transition gaps
remain closed in current source.

The standing DRIFT direction was applied to the complete cumulative range.
Every added mechanism, check, file, setting, or option is either an established
Wyrd owner/mechanism or a conventional Rust/Tokio/nextest facility used for the
same purpose in comparable projects. No unsupported bespoke mechanism was
found, and none is required as remediation.

## Open questions

None.

## Verification notes

- Per the strict review constraint, this reviewer executed no build, test,
  Cargo, `mise`, formatter, linter, code-generation, or runtime command.
- Recorded cumulative evidence reports successful focused Workflow tests,
  Skald/shared/Wyrd/principals/gateway lanes, Python and TypeScript unit and
  integration lanes, the complete Bifrost suite, code generation, client-tier,
  tenant-isolation, PyO3/unwrap checks, formatting, and lints.
- R4 specifically records the focused paused-time deadline-owner test, 338
  Skald tests, 684 Wyrd-server tests with 23 skipped, and 43 Oracle journey
  tests, including the updated forwarded-Workflow selector.
- Generated-artifact and runtime claims are therefore supported by recorded
  evidence and static source/diff inspection, not independently re-executed by
  this reviewer.

## Overall result

**PASS**
