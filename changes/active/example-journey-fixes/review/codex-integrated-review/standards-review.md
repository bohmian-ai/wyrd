# Repository standards review

## Immutable subject

- Repository: detached worktree at the review root
- Base: `ef03f624a1f3a6bcd51bbd17060271f0b5e8cabf`
- Candidate: `c4ba2b0502f8968a78e5e960bcc9eba43e960643`
- Range reviewed: the complete `base..candidate` diff (153 files; production,
  contracts, migrations, generated artifacts, SDKs, tests, fixtures, docs, and
  build configuration), not the task summaries
- Candidate identity rechecked after source review: unchanged

## Authority coverage

| Changed surface | Governing authority read | Source and consumer coverage | Result |
|---|---|---|---|
| Wyrd contracts, generated schemas, SDK projections | `AGENTS.md` §§2-3, 7-9, 16; `wyrd-design.md`; `wyrd-doctrine.mdx`; `references/doctrine/architecture-constraints.md`; `references/architecture/patterns.md`; `references/languages/{errors,python-api-and-stubs,pyo3-boundaries,typescript-guide}.md` | `wyrd-spec` gateway/verification source, generated JSON, Python `_card_types.py` and gateway exports/stubs, TypeScript card types/declarations, N-API boundary, OpenAPI contract test | PASS |
| Skald typed Jev dialect and Vala judge use | `AGENTS.md` §§2-6, 9-10, 16; `wyrd-design.md`; `references/languages/rust-core.md`; `references/domain/evaluation.md` | `skald-spec` wire/request/response/adapter/prompt, provider client/export, prompt and agent paths, gateway adapters, `vala-eval` judge and integration test, fixture | PASS |
| Bifrost planning, live-table execution, and analytical proof | `AGENTS.md` §§2-6, 9-11, 16; `bifrost-design.md`; `references/domain/{vala-architecture,olap-serving,datafusion,analytical-operations-reliability}.md` | Oracle planning/error mappers, live predicate handling, public SDK/MCP journeys, shared deterministic workload, published/live/mixed/remote-Scribe consumers | PASS |
| MCP catalog and common-framework compatibility | `AGENTS.md` §§2, 9, 11, 16; `wyrd-design.md`; `references/languages/{agent-harness,testing-workflows,typescript-guide}.md` | MCP catalog, operators/principals/probe, real 35-tool schema assertion, official MCP/OpenAI Agents TypeScript journey, package lock/config | PASS |
| Gateway attribution and trace propagation | `AGENTS.md` §§2-6, 9-11, 16; `wyrd-design.md`; `wyrd-security-posture.md`; `references/domain/telemetry-observations.md`; `references/languages/{rust-core,pyo3-boundaries,python-api-and-stubs,typescript-guide}.md` | Shared client HTTP trace owner, server inbound middleware/router/MCP re-scope, gateway ingress/invocation/capture/workflow, verification direct/eval/result paths, telemetry links, Rust/Python/TypeScript consumers and journeys | PASS |
| Verifier actor persistence and result schema | `AGENTS.md` §§2-6, 9-11, 16; `agent-rules.md` SQL/tenancy rules; `wyrd-design.md`; `wyrd-security-posture.md` | `TenantConn` verifier-run writes, migration/check constraint, claimed/status/result projection, Bifrost result table, PostgreSQL and server journeys | PASS |
| Managed-secret sealing, rotation, CLI local bootstrap | `AGENTS.md` §§2-6, 9, 11, 16; `agent-rules.md`; `wyrd-security-posture.md`; `references/architecture/patterns.md` | deployment `SealingKeyring`, tenant/name/provider binding, gateway resolver/service/snapshot, operator rewrap inventory/CAS, boot/state/config, CLI owner-only persisted key, docs and restart/rotation/tenant tests | PASS |
| SQL capability and tenant isolation | `agent-rules.md` raw-pool, `TenantConn`, `OperatorPool`, commit, and cross-tier rules; `wyrd-security-posture.md` tenant/data isolation | All changed production SQL signatures/fields and callers; tenant work uses `TenantConn`, cross-tenant rewrap uses `OperatorPool`; no added raw `PgPool`, naked connection, or caller-owned transaction | PASS |
| Tests, fixtures, manifests, `mise`, and documentation | `AGENTS.md` §§11-12, 16; `agent-rules.md`; `references/languages/testing-workflows.md`; `references/languages/spec-driven-development.md` | inline/unit vs PostgreSQL/journey placement, ignored gated journeys, exact commands in task evidence, Cargo feature/dependency placement, TypeScript worker cap diagnosis, docs and security-posture update | PASS |

## Rule-by-rule audit

| Repository rule | Evidence | Result |
|---|---|---|
| Durable contracts remain in typed Rust owners and foreign runtimes remain thin projections (`AGENTS.md:98-124`, `AGENTS.md:153-180`) | Jev wire types are in `crates/skald/skald-spec/src/wire/jev.rs:23`; shared outbound propagation is in `crates/shared/wyrd-client/src/transport/trace.rs:28`; Python only reads its runtime span then delegates at `sdks/wyrd-sdk-python/src/observe/mod.rs:129`; TypeScript does the same at `sdks/wyrd-sdk-ts/native/src/cards.rs:665`. No server behavior moved into an SDK. | PASS |
| Foundational `wyrd-spec` stays IO/async/PyO3 free (`AGENTS.md:98-100`) | Its changed files contain contract/schema changes only; provider IO remains in `skald-providers`, gateway execution in `wyrd-gateway`/server, and Python boundary work in `wyrd-sdk-python`. | PASS |
| Rust owners are cohesive concrete structs; pure helpers remain synchronous (`AGENTS.md:226-267`, `AGENTS.md:269-285`) | `JevClient`, `GatewayInvocation`, `ManagedSecretBinding`, `SealedSecretRewrap`, `VerificationControl`, `EvalEngine`, `SupportWorkload`, and `SupportOracle` own their state; planning/parsing helpers such as `requested_subject`, `caller_planning_fault`, and `reads_scalar_subquery` are synchronous. | PASS |
| Domain identifiers are not degraded to raw strings in materially modified Rust (`AGENTS.md:184-188`) | Server and SQL paths use `RunId`, `CardUid`, `PrincipalId`, and `DataTenantId`. `PublicWyrdGatewayCaller` converts its typed inputs to `Option<String>` fields at `crates/shared/wyrd-client/src/workflow/gateway.rs:45-50`, rather than retaining `Option<RunId>`/`Option<CardUid>`. This is an explicit style deviation, but values are accepted only as typed references at `:73` and used only as HTTP header serialization at `:126-130`; no invalid value, public-contract divergence, or runtime consequence is reachable. | FAIL (non-blocking style note) |
| Imports are the module dependency manifest; no function-scoped imports (`architecture/agent-rules.md:5`) | New function-local imports remain in production at `crates/shared/wyrd-telemetry/src/lib.rs:275-276` and in journey tests at `crates/wyrd/wyrd-cli/tests/pg_tests.rs:643-644` and `sdks/wyrd-sdk-rust/tests/integration/verify_in_real_time.rs:582-589`. They introduce no behavioral/security consequence. | FAIL (non-blocking style note) |
| Every new or materially modified Rust item has substantive rustdoc, and fallible functions document errors (`AGENTS.md:710-723`, `architecture/agent-rules.md:32`) | Audited the added/modified modules, structs, fields, enum variants, constants, helpers, trait methods, and tests across Rust production and test code. Representative new public items are documented at `crates/skald/skald-spec/src/wire/jev.rs:23-179`, `crates/shared/wyrd-client/src/transport/trace.rs:23-55`, `crates/wyrd/wyrd-gateway/src/managed.rs:22-145`, and `crates/wyrd/wyrd-testing/src/bifrost/support_workload.rs:36-364`. | PASS |
| Public failures use stable catalog errors and internal failures do not leak internals (`AGENTS.md:193-204`; `references/languages/errors.md`) | Jev gateway refusals map through existing Wyrd/provider mechanisms; Oracle exposes authorized logical-planner caller faults at `crates/vala/vala-bifrost-redux/src/oracle/mod.rs:3762-3797` and retains opaque internal mapping at `:3800-3813`; tests cover 4xx suggestions, 5xx opacity, and unauthorized-name non-disclosure. | PASS |
| Tenant SQL uses `TenantConn`; cross-tenant work uses `OperatorPool`; callees do not commit (`architecture/agent-rules.md:2-4,7`) | Changed verifier-run methods retain `&mut TenantConn<'_>`; gateway-secret inventory/CAS is explicitly on `OperatorPool` at `crates/wyrd/wyrd-sql/src/queries/auth/human_connections.rs:389-430`. No raw pool or transaction type was added to a production signature/field. | PASS |
| Secrets use redacted wrappers, are tenant-bound, are not logged/returned, and rotation reuses the one deployment keyring (`AGENTS.md:205-207`; `wyrd-security-posture.md`) | `SealedPayload.secret` is `SecretString` with custom redacted `Debug` at `crates/wyrd/wyrd-gateway/src/managed.rs:37-67`; plaintext buffers are zeroized at `:99-110` and `:125-143`; payload binds tenant/name/provider; boot rewrap adds the table through an operator CAS; CLI key creation is random, owner-only, no-clobber, synced, and never printed at `crates/wyrd/wyrd-cli/src/server/dev.rs:508-557`. | PASS |
| Authorization remains blocking and audit/result writes retain the one non-blocking Scribe path (`AGENTS.md:125-148`) | Gateway Card scope is checked only for requested `card_uid`; resolved verifier subjects do not introduce a second authorization gate. Direct verification still checks permission before engine work and stages decisions/results through `AppState::scribe_outbox`; no new audit writer, WAL, or audit failure was added. | PASS |
| Trace context is observability only and cannot supply identity/authority (`wyrd-security-posture.md`; telemetry reference) | Inbound parsing yields only `TraceContext` at `crates/wyrd/wyrd-server/src/http/middleware/trace_context.rs:52-70`; attribution comes independently from authenticated `Caller` and typed subject headers. Malformed/repeated traceparent is ignored and never affects authorization. | PASS |
| Bifrost authorization precedes planning and source IO (`wyrd-security-posture.md:346-352`) | The candidate changes only post-authorization planning error mapping and live-filter placement; task evidence identifies unchanged `authorize_resolved_tables` and journeys proving unauthorized table/column names do not leak (`TASK-002:50-53`). | PASS |
| MCP reads/writes preserve scopes and publish bounded typed object-root schemas (`AGENTS.md:125-126`; agent-harness reference) | `CreateArgs` nests the tagged union below `request` with `deny_unknown_fields` (`crates/wyrd/wyrd-server/src/mcp/operators.rs:56-70`); real-catalog tests validate all tool roots and the official clients list/convert/call them. No tenant/principal identity moved into tool input. | PASS |
| PyO3 boundaries release the GIL, do not retain `Bound` across await, and reuse Rust transport (`AGENTS.md:287-318`) | Python `verify` snapshots trace context before `py.detach`, then runs shared `wyrd_client` behavior at `sdks/wyrd-sdk-python/src/observe/mod.rs:392-424`; no Python object crosses the await/blocking boundary. | PASS |
| Python/TypeScript declarations and generated contracts match their sources (`AGENTS.md:320-335`; Python/TypeScript references) | Source, public exports, `.pyi`, N-API declarations, hand-authored TypeScript, JSON schemas, and OpenAPI assertion were all changed together. Recorded `codegen:check`, `py:typecheck`, `ts:typecheck`, and `ts:napi:check` are green (`TASK-001:53-68`, `TASK-004:78-86`). | PASS |
| User/agent-facing changes have real journeys in every shipped runtime; language lifetimes stay in their runtimes (`AGENTS.md:381-423`) | Jev, gateway attribution/trace, Bifrost errors/subqueries, MCP framework compatibility, and local-dev key persistence have real-server Rust/Python/TypeScript/MCP/CLI journeys as applicable. Python behavior is exercised in pytest and Node/MCP behavior in Vitest, not emulated in Rust. | PASS |
| Gated integration tests may be ignored only for repository-owned journey lanes; gates are not weakened (`AGENTS.md:381-396`; `agent-rules.md:13`) | Added `#[ignore]` annotations name repository-managed Postgres/Bifrost journey lanes and are run with `--run-ignored=all` by the recorded commands; no failing check was disabled, no `#[allow]` was added, and the worker cap has a recorded load diagnosis (`TASK-003:42-55`). | PASS |
| Generated artifacts are regenerated, not treated as independent source (`architecture/agent-rules.md:25`) | Source changes exist for every JSON/schema/Python/TypeScript declaration change, and task records show `codegen:regen` followed by clean `codegen:check` (`TASK-001:53-68`, `TASK-004:121-123`, `TASK-005:51-57`). | PASS |
| Dependency cost remains at the narrowest owner; no new Cargo feature is introduced (`AGENTS.md:122-124`; `agent-rules.md:1`) | `rand`/`base64` move into CLI production dependencies because `server dev` creates the key; DataFusion is a normal dependency only of the non-shipping `wyrd-testing` harness because its library now owns the reference oracle; OTel additions are test dependencies. No Cargo feature was added. | PASS |
| Permanent source contains no task/agent-plan references (`architecture/agent-rules.md:30`) | Search of added production, test, fixture, config, and docs lines found no task IDs, plan IDs, or implementation-agent notes. | PASS |

## Material findings

None. I found no repository-rule violation with a behavioral, security,
tenancy, durability, public-contract, or generated-artifact consequence. The
two failed style rows above are recorded for calibration only; under the task
review threshold they are not blocking findings and receive no finding IDs.

## Verification assessment

The task records provide focused and breadth evidence for every touched layer:

- Jev contract/provider/gateway/SDK/codegen and the three language journeys:
  `changes/active/example-journey-fixes/tasks/TASK-001-jev-typed-provider.md:42-72`.
- Bifrost error/subquery journeys plus the published/live/mixed/remote
  DataFusion-oracle suite and `check:deps`:
  `changes/active/example-journey-fixes/tasks/TASK-002-bifrost-errors-and-subqueries.md:48-136`.
- Real MCP/OpenAI Agents clients, full catalog, TypeScript integration lane,
  lints/docs/diff checks:
  `changes/active/example-journey-fixes/tasks/TASK-003-mcp-compatibility.md:35-55`.
- Attribution, trace, SQL, codegen, tenant-isolation, SDK, MCP, and server
  journeys:
  `changes/active/example-journey-fixes/tasks/TASK-004-gateway-attribution-and-trace.md:40-123`.
- Managed-secret binding/rotation/restart/local-dev journeys and the gateway
  gate:
  `changes/active/example-journey-fixes/tasks/TASK-005-local-keyring.md:43-63`.

I independently ran `git diff --check` successfully. I attempted to rerun
`mise run check:deps` and `mise run check:tenant-isolation`; after relocating
mise's state/cache into `/tmp` to satisfy the read-only home, Cargo reported an
existing build-directory lock held by another review process. I stopped rather
than violate the repository rule against overlapping Cargo-backed commands.
The recorded candidate evidence for both checks is complete and green at
`TASK-002:135`, `TASK-004:121-123`, and `TASK-005:55-57`.

## Overall result

**PASS**

The complete candidate diff conforms to the material repository standards.
There are no material standards findings for independent validation.
