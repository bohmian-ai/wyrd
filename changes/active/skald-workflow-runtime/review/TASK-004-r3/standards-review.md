# TASK-004 r3 repository standards review

Immutable subject: `b90335e0991438e8c28b65ed9c0c4cd80f9b0d56..f17726fb25df1fa513875dca8d92f0073340ee0a`

Approved authority: `SPEC-skald-workflow-runtime`, Revision 13.

Review mode: complete cumulative diff, candidate source, repository authority,
and recorded evidence only. No build, test, Cargo, or mise command was run.
`.codegraph/` is absent, so immutable Git objects and repository source were
used directly. The requested candidate was checked before review and before
writing this report and remained `f17726fb25df1fa513875dca8d92f0073340ee0a`.

## Review findings

### Critical

None.

### Important

#### STD-004-R3-001 — VIOLATION: changed Rust interfaces and one journey import still bypass the module import block

- **Violated rule:** `architecture/agent-rules.md` requires every type used in
  a field, parameter, return type, bound, or `where` clause to be imported at
  module scope and referenced by its bare name. It separately requires every
  `use` statement to live at the top of its module. The narrow local-trait
  exception is for a trait needed inside one generic function; it does not
  cover an ordinary journey function.
- **Locations and evidence:**
  - `crates/wyrd-spec/src/card/prompt/mod.rs:264` adds the changed
    `raw_body` interface as `Option<&serde_json::Value>` even though `Value` is
    already imported in the module at line 35.
  - `crates/wyrd/wyrd-server/src/oracle/lifecycle_controls.rs:314` adds
    `deadline: tokio::time::Instant` to `cancel_while_opening`, and the new test
    helper at line 394 returns `tokio::time::Instant`, rather than importing
    the type in each owning module.
  - `crates/wyrd/wyrd-testing/tests/bifrost/oracle/peer_cluster.rs:1806-1811`
    materially changes `fixture_rows_ipc` while retaining
    `Result<bytes::Bytes, JourneyError>` in its interface instead of importing
    `Bytes` at the module dependency block.
  - `crates/wyrd/wyrd-server/tests/pg_workflow_runs.rs:2224-2233` introduces
    `use std::os::unix::fs::PermissionsExt as _;` inside
    `server_routes_keep_gateway_and_external_ownership`; this is an ordinary
    non-generic journey function and the import is not in the module's
    top-of-file dependency block.
- **Consequence:** the cumulative candidate still fails a mandatory repository
  source-shape rule after the R2 bare-type remediation. The affected modules no
  longer expose their complete dependency surface in the required module-level
  import block. Passing recorded format and lint lanes cannot waive an explicit
  source rule that those lanes do not enforce.
- **Required testable correction:** import and use the bare `Value`, `Instant`,
  and `Bytes` names in the affected owning modules. Move the Unix
  `PermissionsExt as _` import to the top-level import block under
  `#[cfg(unix)]`. Preserve behavior and rerun the existing format/lint and
  affected recorded test lanes. Add no scanner, allow attribute, repository
  check, setting, or option.

### Suggestions

None. This acceptance review does not record optional improvements.

## Authority coverage

| Changed surface | Applicable authority read and applied | Coverage result |
|---|---|---|
| Active Revision 13 spec, TASK-004, R1/R2 remediation records, and recorded evidence | `AGENTS.md` §§1, 11-16; `architecture/agent-rules.md`; `architecture/references/languages/spec-driven-development.md`; `implementation-execution.md`; `maintainer-style.md` | PASS — current task/remediation metadata names Revision 13 while immutable historical R1 review inputs remain historical |
| Provider-tagged `ProviderRequest`, Prompt authoring, provider clients, cache, loader, Cards, fixtures, and generated schemas | `AGENTS.md` §§2-4, 8-10, 12; `architecture/wyrd-doctrine.mdx`; `architecture/wyrd-design.md`; `architecture/references/doctrine/architecture-constraints.md`; `architecture/patterns.md`; `rust-core.md`; `errors.md` | **FAIL only for the test-support interface in `STD-004-R3-001`**; adjacent tagging, provider ownership, and generated-contract ownership otherwise pass |
| Skald Workflow preparation, per-Agent tool binding, execution transitions, and shared-client composition | `AGENTS.md` §§2-6, 10, 15-16; `architecture/wyrd-doctrine.mdx`; `architecture/wyrd-design.md` Workflow section; `architecture-constraints.md`; `patterns.md`; `rust-core.md` | PASS — Skald retains the executor and provider/tool primitives; server/client layers compose them without a second engine |
| Server Workflow host, run table, routes, config, boot/shutdown, permissions, and state | `AGENTS.md` §§2-6, 9, 12, 15-16; `architecture/wyrd-design.md`; `architecture/wyrd-security-posture.md`; `patterns.md`; `agent-harness.md`; `errors.md` | PASS — cohesive owners, typed routes/errors, fresh route authorization, bounded captured authority, and accepted-job architecture are present |
| Server Cards graph pinning and exact-ref reads | `AGENTS.md` §§2-6, 9, 16; `architecture/wyrd-design.md` registry/reference sections; `architecture-constraints.md`; `patterns.md`; `rust-core.md` | PASS — server Cards owner retains SQL/tenancy and produces the exact pinned graph without client HTTP traversal |
| In-process gateway projection and external binding resolution | `AGENTS.md` §§3-6, 9-10, 15-16; `architecture/wyrd-design.md`; `architecture/wyrd-security-posture.md`; `patterns.md` provider/external-network/server sections; `errors.md` | PASS — provider dialects stay typed/provider-owned, secrets remain indirect and redacted, and governed gateway admission remains live |
| Shared bounded query collector, scheduled-query cancellation, MCP projection, and tool schemas | `AGENTS.md` §§3-6, 9-12, 16; `architecture/bifrost-design.md`; `architecture/references/domain/vala-architecture.md`; `olap-serving.md`; `datafusion.md`; `analytical-operations-reliability.md`; `agent-harness.md`; `errors.md` | **FAIL only for the new lifecycle-control interfaces in `STD-004-R3-001`**; bounded collection, terminal ownership, typed tool schema, and canonical Oracle authorization otherwise pass |
| Oracle analytical lifecycle, admission/resources, peer loss, stream settlement, and follower graph release | `AGENTS.md` §§3-6, 10-12, 15-16; `architecture/bifrost-design.md`; `vala-architecture.md`; `olap-serving.md`; `datafusion.md`; `analytical-operations-reliability.md` | PASS — grant-stream close remains follower release, the leader does not await a follower acknowledgement, and the deleted graph-drain polling/supervisor idle refusal remain excluded |
| Rust server journeys, Bifrost journeys, Python tests, fixtures, and test-support harnesses | `AGENTS.md` §11 and §16; `architecture/agent-rules.md`; `testing-workflows.md`; `agent-harness.md`; `rust-core.md` | **FAIL — `STD-004-R3-001`**; journey placement and real server/peer boundaries otherwise follow the test taxonomy, including the approved fixture-only foreign-tenant gateway limitation |
| `.config/nextest.toml`, `Cargo.toml`, and `Cargo.lock` | `AGENTS.md` §§4, 11-12, 15; `architecture/agent-rules.md`; `testing-workflows.md`; `rust-core.md` | PASS — native nextest test groups extend the established repository mechanism, and `jsonschema` was already workspace-installed and is dev-only; no bespoke check or third-party mechanism was introduced |
| Governing architecture and security updates | `AGENTS.md` §§1-2, 9, 15; `architecture/wyrd-design.md`; `architecture/wyrd-security-posture.md`; `architecture/bifrost-design.md`; `architecture/wyrd-doctrine.mdx` | PASS — accepted process-local authority and the implemented Oracle ownership model are described in existing authorities rather than a new parallel document |

## Applicable rule results

| Repository rule | Exact source evidence | Result |
|---|---|---|
| Current task artifacts identify the approved spec revision | `spec.md:1-4`; `TASK-004-accepted-server-jobs.md:1-7`; `TASK-004-R1-close-accepted-job-gaps.md:1-20`; `TASK-004-R2-align-revision-and-source-contracts.md:1-18` | PASS |
| Imports remain at module scope and interface types use bare imported names | `prompt/mod.rs:264`; `lifecycle_controls.rs:314,394`; `peer_cluster.rs:1806-1811`; `pg_workflow_runs.rs:2229-2233` | **FAIL — `STD-004-R3-001`** |
| Stateful workflows and dependency-backed behavior have cohesive concrete owners | `components/workflow/host.rs:49-57`; `components/workflow/runs.rs:217-237`; `query/collect.rs:264-277`; `components/cards/resolve.rs:548-661` | PASS |
| Async is limited to IO and intentional orchestration | Workflow, gateway, Cards, query, and lifecycle async methods await server, transport, database, task, or stream work; pure graph/input/schema/projection checks remain synchronous | PASS |
| Server owns identity, authorization, tenancy, audit, and lifecycle effects | `components/workflow/host.rs:75-115,184-208`; `components/workflow/routes.rs:76-166`; `architecture/wyrd-security-posture.md:176-193` | PASS |
| Public handlers use typed bodies, stable errors, and scrubbed trace instrumentation | `components/workflow/routes.rs:41-166`; route annotations at `:76`, `:120`, and `:153`; Wyrd error mapping remains catalog-backed | PASS |
| Accepted runs retain no bearer/secret and cannot widen after acceptance | `architecture/wyrd-design.md:443-455`; `architecture/wyrd-security-posture.md:181-193`; caller/run-key construction and live per-call owner checks in `workflow/host.rs`, `gateway/workflow.rs`, and `query/collect.rs` | PASS |
| Provider wire requests are explicitly tagged and provider-specific handling stays behind provider owners | `skald-spec/src/request.rs:18-59`; provider client conversions under `skald-providers/src/clients/`; generated Prompt schemas requiring `provider` and `body` | PASS |
| Generated artifacts follow source and are not a second hand-maintained contract | Adjacent-tag derive in `request.rs:18-27`; matching schema and fixture diffs; recorded `codegen:regen` and `codegen:check` evidence | PASS |
| Every user/agent-facing capability has production-shaped journey coverage | `wyrd-server/tests/pg_workflow_runs.rs`; `wyrd-testing/tests/bifrost/oracle/workflow.rs`; recorded Rust, Python, TypeScript, gateway, server, and Bifrost journey evidence | PASS |
| External tests earn separate binaries and managed environments | `pg_workflow_runs` boots a real server/Postgres path; Oracle Workflow journeys drive real loopback HTTP/gRPC/mTLS peer boundaries; nextest groups bound shared Postgres/peer resources | PASS |
| New checks, settings, dependencies, and test controls use established or broadly standard mechanisms | No new repository check; `.config/nextest.toml:72-77,109` uses nextest's native test-group facility; existing workspace `jsonschema` is used only as a dev dependency | PASS |
| No gate circumvention or unsupported compatibility path entered the diff | No added `#[allow]`/`#[ignore]`, compatibility reader, migration, provider alias, alternate audit writer, second query engine, release acknowledgement, graph-drain poller, or supervisor idle refusal | PASS |
| Rust documentation covers new/materially changed items and fallible operations | Workflow, query, config, provider, Oracle, fixture, and journey items carry intent/invariant rustdoc and `# Errors`/`# Panics` where applicable; R1's declaration-doc findings remain closed | PASS |
| Tenant isolation, current owner authorization, and secret handling stay server-owned | Caller-derived tenant/principal/run keys, canonical Cards/Bifrost/gateway decisions, secret-reference resolution, and cross-tenant not-found/denial journeys; no bearer or credential value is retained in a run | PASS |
| Oracle release follows approved native ownership rather than bespoke residue polling | `analytical.rs:2492-2531,2698-2780,7779-7792`; `query_stream.rs:785-802`; architecture wording in `bifrost-design.md:397-408` | PASS |

## Open questions

None. The remaining violation has a behavior-neutral correction within existing
module import blocks and requires no product, architecture, security,
compatibility, concurrency, resource-ownership, or persistent-data decision.

## Verification notes

- Per the strict review constraint, this reviewer ran no builds, tests, Cargo
  commands, or mise commands.
- Recorded cumulative evidence reports PASS for format/lints, code generation,
  client-tier/tenant/PyO3/unwrap boundaries, Skald/shared/Wyrd/principals/gateway
  lanes, Python and TypeScript unit/integration lanes, the Rust Workflow-loading
  journey, the complete Bifrost suite, and focused server/Oracle journeys.
- The R2 remediation records additional PASS evidence for `mise run fmt`,
  `mise run lints`, `WYRD_TEST_PACKAGES=wyrd-server mise run test:wyrd`,
  `mise run test:bifrost:journey:oracle`, and `mise run codegen:check`.
- Recorded green lanes do not supersede the explicit import/source-shape rule;
  no verification limitation has been converted into a pass.
- The human-standing decisions were applied as authority: no deleted Oracle
  polling/refusal mechanism is required, stream close is follower release with
  no leader-side acknowledgement wait, and no second-tenant gateway credential
  harness is required.

## Overall result

**FAIL**

The cumulative candidate satisfies the reviewed ownership, contract, security,
test-taxonomy, generated-artifact, provider, and Oracle lifecycle authorities,
but `STD-004-R3-001` remains a direct mandatory repository-rule violation.
