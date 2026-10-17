# Repository standards review

Result: **FAIL** — one residual mandatory rustdoc issue; all previously enumerated rustdoc corrections are present.

## Subject and method

Root: `/home/thorrester/Documents/GitHub/wyrd-worktrees/local-dev-setup`.
Base: `bc3bf25e3f78b0c429110350a3de4d58290e632f`.
Candidate: `38dbc023cd71292e1badd3abe90e87c69b76fa96`; HEAD still matched after inspection.

Inspected the complete cumulative range, using the remediation range to locate changes. Read the prior verdict, validation ledger, standards report and R1 as remediation context; no other current-round discovery report was read. Scope is this worktree, including its example consumers; sibling repository state is excluded. No CodeGraph index exists. This is a source-only standards audit, not task acceptance or a Ponytail design review. No build, Cargo, lint, codegen or runtime command was launched, and no source was changed.

## Authority coverage

All rows apply `AGENTS.md`, `architecture/agent-rules.md`, the reference router, and references `languages/spec-driven-development.md`, `languages/implementation-execution.md`, and `languages/maintainer-style.md`. The approved spec/original task and R1 establish which contract changes are authorized, without substituting for repository standards.

| Changed surface | Applicable authority read | Inspected source / consumers |
|---|---|---|
| CLI dev, terminal, MCP hosts, platform recovery visibility, lib module | Full Wyrd design, doctrine, security posture, deployment/release; references Rust core, errors, patterns, architecture constraints, testing workflows | `DevArgs`, `DevInputs`, `DevServer`, saved client, sealing readers, interruption, `Terminal`/`EchoGuard`, both host-selection callers, platform credential owner |
| Shared client configuration and restricted writer | Same Rust/client/security authorities; AGENTS §§3–6, 9, 16 | New dev tables and persistence methods; existing `CredentialsFile` document/lock/replace, config resolution, `CredentialWriter`, sibling `Gateway` facade and shared transport |
| Pure gateway DTOs/validator/re-exports | Design/doctrine; references patterns, Rust core, errors, agent harness | `ProviderDeploymentsApply`, `ProviderDeploymentsApplied`, validation and unit proof; existing secret-bearing source/view types; schema generator inputs |
| Server gateway service, route and MCP | Security posture trust/authz/SQL/audit/secret rules; agent-rules capability and transaction rules; patterns/Rust/errors/agent harness | Full apply and extracted shared write bodies, actual owning fields/signatures, audited transaction owner, sibling put/revoke/delete, route registration, MCP descriptors/gating/dispatch |
| Local analytical data root and lifecycle projection | Bifrost design System boundary, Live tail/recovery/shutdown, Resource/failure invariants; deployment operations | CLI selects canonical absolute data root and supervises ordinary server; no engine, table format, WAL or query owner is modified |
| All changed Rust test files | AGENTS §§11–12, 16; test placement/runtime rules; testing workflows, Rust core, maintainer style | CLI gateway and pg journeys including new `start_with` and portable `free_bind`; MCP connectivity/discovery/gateway; server apply/FK rollback and served OpenAPI; runtime catalog unit assertion |
| Python installed-CLI integration tests | AGENTS §§8, 11, 16; Python API/stubs, testing workflows, maintainer style | Complete cumulative Python diff; `converse`, concealed-question interrupt proof, unattended host case, response-loss proxy, gateway/example consumers, fixture cleanup and markers |
| Manifests/lock and mise/package checker | AGENTS dependency/feature/gate rules; architecture constraints, testing workflows, TypeScript guide | Workspace and spec/client/CLI/utils manifests; direct workspace utils edge and matching lock entry; local-dev lane; existing exact npm archive allowlist |
| Architecture notes, all changed docs and gateway example | Full design/doctrine/security/deployment authority; agent harness and testing references | Setup, quickstart, gateway, troubleshooting, MCP reference, example README/document, existing two-step documentation and local-env grammar |
| Active task/review Markdown | Spec-driven development and implementation execution | Original task, approved spec, prior verdict/ledger and remediation proof/diagnosis. Historical reports are records, not current source authority. |

No Python binding/export/stub, TypeScript SDK source/declaration, UI, migration, or analytical implementation changed. Their dedicated implementation rules do not independently apply; Python test and TypeScript package-consumer rules do.

## Rule results

| Rule | Result and source evidence |
|---|---|
| AGENTS §§2–3: sole owners, pure spec, SDK boundary | PASS. DTOs/validator are synchronous and IO/PyO3-free; durable apply stays on `GatewayAdministration`; `CredentialWriter::apply_deployments` reuses shared transport. SDK `Gateway` gains no credential mutation. |
| agent-rules: actual SQL capability types and lifecycle | PASS. `GatewayAdministration` retains `ServerPostgres`, not a raw pool. `write_credential` (`service.rs:688`) and `write_deployment` (`:726`) receive `&mut TenantConn<'_>` and never end it. Apply owns the transaction delivered by `audited` and commits after both stages. No manual tenant predicate or production raw SQL pool/signature was introduced; test-only raw pool observation is outside production library signatures. |
| Security posture: tenancy/RBAC before protected work | PASS. Apply invokes `audited` with `Permission::gateway_write`; it authorizes before obtaining `caller.data_tenant_id`'s tenant transaction. Request DTO contains no trusted tenant/principal identity. Shared helpers retain source assignment/provider validation and credential share locking. |
| agent-rules: one nonblocking audit path | PASS. `audited` stages through existing `audit::authorize` before work; helpers evaluate no extra permission and add no audit writer. No transaction-coupled audit, sink, staging table or relay added. |
| AGENTS §9; errors/agent harness: typed surface, stable problems, tracing | PASS. New HTTP handler uses typed request/response, existing `GatewayResult` mapping and `instrument(skip_all)`. MCP schema/dispatch uses the same operation, write permission and existing structured errors. No parallel error catalog. |
| Security posture and AGENTS §4: secret handling | PASS. Resolved DSNs are `SecretString`; supplied sealing key is `SecretBox` with bounded owner-only `read_secret_file`, then retained using existing no-clobber file path. Local env becomes redacted `SecretBearer`; applied credentials return views. Persistence reuses protected file locking/atomic replacement. Credential-bearing environmental storage URLs cannot reach the saved/ready summary: existing `BackendConfig::from_url` refuses them before server readiness/save. |
| AGENTS §5: retained state has an owner | PASS. `DevServer` owns child and effective configuration, `Terminal` its descriptor, `EchoGuard` restoration state, existing config/service/client handles own their respective workflows. No new trait or zero-sized utility owner. |
| AGENTS §6 and platform rules | PASS within source scope. New async operations await terminal/process/network/SQL IO; pure parsing/validation remains synchronous. New `cfg(unix)` includes every supported macOS/Linux target. `free_bind` now uses `TcpListener::bind(:0)` rather than procfs; no new supported-target exclusion exists. |
| AGENTS dependency ownership and manifest policy | PASS. CLI adds only `wyrd-utils = { workspace = true }` for its existing secret reader; utils was already in the client dependency closure, has no server/SQL dependency, and no new feature/version/library is added. Lock change records that direct edge. R1 records `check:deps` PASS. |
| AGENTS §11: test ownership, tiers and boundary proof | PASS for source and registration. Compiled CLI and public MCP/HTTP journeys use real server/managed Postgres; Python PTY/Node consumers remain in owning runtime; SQL failure proof uses the actual FK after an earlier rotation and checks retained envelope/value. New ignored CLI/MCP journeys use existing explicit environment-owning lanes, not failure suppression. pg cases stay in pg targets/modules. |
| AGENTS §11: independent served contract proof | PASS for source. `pg_openapi_contract` asserts runtime collection request/response schema refs/refusals; MCP descriptors, connectivity/discovery and gateway apply journey exercise runtime catalog and domain denial separately from codegen. |
| AGENTS §12 / implementation-execution: preserve gates and diagnose failures | PASS. No assertions were weakened to clear a failure. `converse` now waits for the final ready-message line, preserving every existing assertion; R1 records symptom, line-buffering cause, shared fix site, one caller, and fresh diagnostician agreement. Current `println!` and reader bodies substantiate that diagnosis. README allowance extends an existing exact package inventory to the already tracked README; source-TS and symlink checks remain. No generated artifact was hand-edited. |
| AGENTS §16: Python/API and docs alignment | PASS. New Python tests are top-level functions; proxy class is a fixture handler, not a `Test*` class. No public Python API changed, so no native/stub synchronization is needed. Examples contain variable names rather than expanded secrets. CLI/MCP/HTTP docs preserve current wire vocabulary and distinguish refusal from uncertain commit. |
| AGENTS §16: substantive Rust rustdoc/error/panic contracts | FAIL: REPO-R2-001 below. All specifically enumerated FIND-7 symbols are corrected, and new R1 helpers/constants have docs, but the cumulative catalog test remains incomplete. |
| agent-rules: imported type names in signatures and module-top imports | FAIL, non-blocking. New `global_config.rs:97` spells `std::path::PathBuf` in its signature; materially changed `pg_tests.rs::server_dev_keeps_the_gateway_managed_secret_key` retains function-local wiremock imports. Neither establishes behavioral/security/public-contract harm and neither is a material proposal. |
| AGENTS §§11–12: execution evidence | SOURCE-ONLY LIMIT. Original task records P1–P12/checks; R1 records exact focused selectors and Linux x86_64 passes, format/lint/docs/dependency/Python checks. No independent rerun or macOS execution is claimed. Final integrated change-review lanes remain outside this task pass. |

## Material proposal

### REPO-R2-001 — VIOLATION: cumulative MCP catalog test still lacks its panic contract

Related stable root cause: **FIND-TASK-001-7** (mandatory documentation on changed Rust verification items); retain the existing ID if independently confirmed rather than inventing another documentation root cause.

Authority: AGENTS §16 requires substantive rustdoc and `# Panics` whenever a changed item can panic, including private test functions. Agent-rules repeats this as `BLOCK_BEFORE_MERGE`; task-review explicitly keeps missing changed-item rustdoc blocking.

Location: `crates/wyrd/wyrd-server/src/mcp/gateway.rs:636`, `gateway_catalog_annotates_every_administration_operation`.

Evidence: The cumulative diff materially extends this test to assert the new apply schema at `tools[9]` and shifts the required-field assertion to `tools[14]`. Its full body performs indexing, `.expect` and assertions, but its only rustdoc is the two-line purpose description at 633–634; no `# Panics` section exists. These are reachable failures when catalog shape, annotations or schema differ; the test is ordinary `#[test]` in the server library. The new public tool's catalog proof therefore remains a changed verification item missing a mandatory contract.

Consequence: This is a repository hard completion gate, not a demonstrated runtime defect or a reason to redesign the test. The docs omit the assumptions the catalog proof deliberately checks.

Smallest correction owner: document this existing test's concrete panic conditions (catalog/order/schema/annotations mismatch or missing required descriptor values); preserve its body, assertions, registration and runtime behavior. Batch with any substantive correction; do not create a separate style-only remediation round. Closure is source inspection of the changed test against §16 plus the appropriate format check; a new behavioral test is unnecessary.

## Prior mandatory-doc closure and limits

The exact prior FIND-7 locations are closed: CLI `gateway`, `gateway_env`, the composite journey and VARIABLE/SECRET; server `apply_document` and SUBMITTED; MCP APPLIED. The newly added server ROTATED, CLI supplied-key constant, `resource_names`, `start_with`, sealing readers and portable listener helper also have intent/error/panic documentation as applicable. REPO-R2-001 identifies a cumulative item omitted from the earlier enumeration, not a regression in those completed corrections.

Within standards scope, remediation source also removes the procfs dependency, threads the resolved unattended policy into host offer, supervises initial concealed input, retains supplied sealing identity, preflights saved credentials before settings/provisioning, reports uncertain apply completion, and adds real SQL-aborted rollback proof. Acceptance/failure semantics are left to their assigned independent reviewers.

No source, tests, manifest, index or commit changed. No sibling repository was inspected as a prerequisite. Runtime, codegen, lint and supported-platform execution were not repeated; supplied passing results are recorded evidence, not reproduced checks.
