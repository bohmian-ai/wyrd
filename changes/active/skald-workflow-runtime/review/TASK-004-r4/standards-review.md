# TASK-004 r4 repository standards review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `b90335e0991438e8c28b65ed9c0c4cd80f9b0d56`
- Candidate: `5cde1b48aab0d70d8686ee8fb5f2978e26cd7f58`
- Approved authority: `changes/active/skald-workflow-runtime/spec.md`, Revision 13
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-004-accepted-server-jobs.md`
- Remediations: TASK-004-R1, TASK-004-R2, and TASK-004-R3 in the preceding review directories
- Review mode: static, strictly read-only; no build, test, Cargo, or `mise` command was run

The candidate was still the named commit when this report was written. No CodeGraph index exists in the repository, so source navigation used the committed diff and repository-native text search.

## Authority coverage

| Changed surface | Governing authority read and applied | Coverage result |
|---|---|---|
| Approved specification, task, remediation, and recorded evidence | `AGENTS.md` §§1, 11-16; `architecture/agent-rules.md`; `architecture/references/languages/spec-driven-development.md`; `implementation-execution.md`; `maintainer-style.md` | PASS — the active task/remediations name approved Revision 13, historical review subjects remain historical, and the candidate records focused and broader evidence without adding an enforcement artifact |
| Skald Workflow planning, execution, step attempts, cancellation, and snapshots | `AGENTS.md` §§3-6, 11, 16; `architecture/wyrd-design.md` Workflow and client/server sections; `architecture/wyrd-doctrine.mdx`; `architecture/references/doctrine/architecture-constraints.md`; `architecture/patterns.md`; `languages/rust-core.md`; `maintainer-style.md` | PASS — Skald remains the runtime owner; the existing `WorkflowExecutor`/`RunLedger` owners carry the lifecycle, synchronous state changes remain synchronous, and R3 establishes attempt one before a `Running` snapshot is published |
| Server-hosted accepted jobs, HTTP routes, configuration, retention, and shutdown | `AGENTS.md` §§3, 5, 6, 9, 11; `architecture/wyrd-design.md`; `architecture/wyrd-security-posture.md`; `architecture/patterns.md`; `languages/errors.md`; `languages/testing-workflows.md` | PASS — `WorkflowRunHost`, `WorkflowRuns`, and typed routes keep server-owned durable behavior, structured errors, bounded process-local state, trace instrumentation, and tracked shutdown ownership |
| Authentication, authorization, audit, tenancy, idempotency, and accepted authority | `AGENTS.md` §§2, 9, 11; `architecture/agent-rules.md`; `architecture/wyrd-design.md` runtime identity/accepted-job authority; `architecture/wyrd-security-posture.md`; `architecture-constraints.md`; `architecture/patterns.md` audit/server patterns; `languages/agent-harness.md` | PASS — verified caller tenancy scopes run identity; create/get/cancel use the canonical audited authorization path; internal tool calls retain live Cards/Bifrost/gateway owner decisions; no alternate audit writer or caller-selected tenant path was added |
| Built-in Workflow tools and bounded Bifrost query collection | `AGENTS.md` §§6, 9-11; `architecture/bifrost-design.md`; `architecture/references/domain/vala-architecture.md`; `olap-serving.md`; `analytical-operations-reliability.md`; `languages/agent-harness.md`; `languages/errors.md` | PASS — the shared collector is bounded, retains the query owner's tenant/auth/audit/terminal path, exposes closed typed schemas, refuses partial success, and introduces neither a second query engine nor a new public listener |
| Oracle analytical lifecycle, cancellation while opening, graph release, and peer journeys | `AGENTS.md` §§2, 6, 10-11; `architecture/bifrost-design.md`; `architecture/references/domain/vala-architecture.md`; `olap-serving.md`; `analytical-operations-reliability.md`; `languages/rust-core.md` | PASS — the candidate preserves the directed human decisions: deleted graph-drain polling and supervisor idle refusal stay deleted, follower release is grant-stream close with no leader acknowledgement, and cancellation remains bounded by the original query deadline |
| Provider request contract, Prompt schemas, examples, fixtures, and SDK-consumed projections | `AGENTS.md` §§3, 8-10, 11; `architecture/wyrd-design.md` Prompt/Workflow/client model; `architecture/wyrd-doctrine.mdx`; `architecture/patterns.md` contract/provider/client patterns; `languages/rust-core.md`; `languages/testing-workflows.md` | PASS — `ProviderRequest` owns one explicit adjacent provider/body tag, generated schemas and fixtures project it, no compatibility reader/alias was added, and Vertex remains distinguishable from the identical Gemini body shape |
| Rust module imports, item documentation, error shape, and dependency placement | `AGENTS.md` §§3-7, 16; `architecture/agent-rules.md`; `architecture/patterns.md`; `languages/rust-core.md`; `languages/errors.md`; `maintainer-style.md` | PASS — the R2/R3 cited interfaces now use module-scope imports and bare names; new/materially changed items have substantive rustdoc; specialized dependencies remain in their narrow owners; public failures use existing derive-backed Wyrd errors |
| Test layout, Postgres/server journeys, nextest resource grouping, and generated artifacts | `AGENTS.md` §11; `architecture/agent-rules.md`; `architecture/references/languages/testing-workflows.md`; `spec-driven-development.md`; `implementation-execution.md` | PASS — server/Postgres and multipod work lives in earned external journey targets, owner logic has in-module tests, ignored journey registration follows the established gated pattern, the nextest group reuses the repository's native resource-group mechanism, and no generated artifact was hand-authored as a new source of truth |

## Applicable-rule results

| Rule | Source evidence | Result |
|---|---|---|
| Durable server behavior remains in Rust owners; Skald owns Workflow runtime and Vala owns Oracle/Bifrost | `crates/skald/skald-workflow/src/workflow.rs:218-375`; `crates/wyrd/wyrd-server/src/components/workflow/{host,runs}.rs`; `crates/wyrd/wyrd-server/src/query/collect.rs:1-38`; Vala Oracle changes remain under `crates/vala/vala-bifrost-redux` | PASS |
| Stateful workflows use meaningful concrete owners and inherent methods | `WorkflowExecutor`, `RunLedger`, `WorkflowRunHost`, `WorkflowRuns`, `RunTools`, `BoundedQuery`, and `ResultCollector` own their dependencies or invariants; module functions are narrow schema/error/conversion helpers | PASS |
| Async is confined to IO/concurrency composition; deterministic preparation and ledger mutation stay synchronous | `Workflow::prepare` is synchronous; `PreparedWorkflowRun::execute`, server host operations, gateway calls, query collection, and Oracle settlement await real IO/tasks | PASS |
| Every published `Running` step already represents attempt one; interrupted active work keeps active-step identity | `crates/skald/skald-workflow/src/workflow.rs:273-290,336-365`; `crates/skald/skald-workflow/src/run.rs:125-136,183-189,221-234`; recorded R3 focused tests | PASS |
| Server routes use typed payloads, stable errors, and trace instrumentation | `crates/wyrd/wyrd-server/src/components/workflow/routes.rs:24-180`; `WorkflowRunHost` returns catalog-backed `WyrdError`; the existing single response mapper remains the HTTP boundary | PASS |
| Verified identity supplies tenant; authorization decisions use canonical audit and do not disclose foreign run existence | `crates/wyrd/wyrd-server/src/components/workflow/host.rs:60-122,125-203`; `crates/wyrd/wyrd-server/src/components/workflow/runs.rs` keys and lookups use `DataTenantId` plus `PrincipalId`; recorded second-tenant journeys cover the supported harness boundary | PASS |
| Public/agent tool inputs and outputs are typed and bounded, with no partial success | `crates/wyrd/wyrd-server/src/query/collect.rs:40-180`; `components/workflow/tools.rs`; row/byte/deadline ceilings and closed schemas are enforced in Rust, not only advertised | PASS |
| Public provider contract is explicit, schema-generatable, and free of a compatibility inference path | `crates/skald/skald-spec/src/request.rs:18-59`; Prompt examples/fixtures and generated schemas carry `provider` plus `body`; the former untagged shape inference was deleted | PASS |
| Foundation/client tiers remain free of server, SQL, Vala, and PyO3 drift | The new provider contract remains in `skald-spec`; `wyrd-spec` changes are pure contract/schema support; server-only Workflow hosting and Vala ownership did not move into client or foundation crates | PASS |
| Module imports and interface type names obey `architecture/agent-rules.md` | R3 correction uses bare `Value`, `Instant`, and `Bytes`, and moves the Unix `PermissionsExt as _` import to module scope; the broader R2 import corrections remain present | PASS |
| New/materially changed Rust items have substantive rustdoc, including errors, cancellation, and invariants where applicable | Workflow owners, route handlers, tool declarations, query collector, provider contract, lifecycle controls, and R3 attempt transitions document their role and failure/cancellation behavior | PASS |
| Tests use the required tier and repository harness shape | `pg_workflow_runs.rs` boots the real server/Postgres boundary; `wyrd-testing/tests/bifrost/oracle/workflow.rs` uses the production-shaped peer cluster; owner-only races remain focused unit tests | PASS |
| No gate circumvention or unsupported bespoke mechanism entered the candidate | No added production `#[allow]`, no disabled required test, no compatibility reader, graph-drain poller, supervisor idle refusal, follower release acknowledgement, alternate audit writer, new lifecycle service, standalone scanner/check, setting, or option | PASS |
| Added configuration/dependency surface is earned and repository-native | `skald-tool` is the existing server runtime dependency moved to the main dependency set; test-only `jsonschema` validates advertised output contracts; nextest test groups are an already-established native resource control and the new group bounds real Postgres connection pressure | PASS |
| Generated artifacts remain projections of owning sources | Prompt JSON schemas and schema fixtures change uniformly with the derive-backed tagged `ProviderRequest`; recorded `codegen:check` is green | PASS (recorded evidence) |

## Material findings

None.

The cumulative candidate contains no material repository-rule violation. In particular, the R3 remediation closes the two prior standards-relevant gaps without adding a mechanism: the observable attempt invariant is corrected in the existing scheduler/ledger owners, and the remaining qualified interface/dependency names now follow the repository's module-import rule.

The standing DRIFT direction was applied to every new mechanism, check, file, setting, and option in the range. No unsupported bespoke enforcement was found. The added nextest grouping uses nextest's standard test-group facility already established in this repository to bound a measured shared Postgres resource; it is not a new checker or product option. The human-fixed Oracle release and foreign-tenant harness decisions were treated as authority and were not reopened.

## Verification limits

Per the review instruction, this reviewer did not execute builds, tests, Cargo, `mise`, code generation, or formatting. The static audit relied on the immutable base-to-candidate diff, current candidate source, and the implementation evidence recorded in TASK-004 and R1/R2/R3. That evidence records successful focused Workflow attempt tests, the Skald family lane, the Wyrd server family with all Workflow journeys, the Oracle journey, format, lints, codegen, client-tier, tenant-isolation, unwrap-audit, principals, gateway, shared, and Bifrost lanes. Runtime and generated-artifact claims therefore remain evidence-backed but not independently re-executed by this reviewer.

## Overall result

**PASS**

