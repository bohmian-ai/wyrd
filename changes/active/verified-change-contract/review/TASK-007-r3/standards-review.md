# TASK-007 r3 Repository-Standards Review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-vcc-t007`
- Base: `f8811ac5035c3aa165d34c38992f9889b3c9081f`
- Candidate: `57ee8dd0b3dddc3e1da34bf543ecc566bae2e831`
- Scope: complete cumulative base-to-candidate diff, including the original task and both remediation changes
- Candidate identity at the beginning and end of review: `57ee8dd0b3dddc3e1da34bf543ecc566bae2e831`

The prior r1/r2 reports were treated only as subject history. Their conclusions and intended verdict were not used as review evidence.

## Authority coverage

| Changed surface | Governing authority read | Coverage evidence |
|---|---|---|
| Repository-wide Rust, ownership, async, documentation, imports, dependencies, and verification | `AGENTS.md` §§2–16; `architecture/agent-rules.md`; `architecture/references/languages/rust-core.md`; `architecture/references/languages/testing-workflows.md`; `architecture/references/languages/spec-driven-development.md` | Complete Rust diff, manifests, `mise.toml`, task evidence, new/modified tests, and representative consumers inspected. |
| Operator/Card/verification contracts and protocol documentation | `architecture/wyrd-design.md` Doctrine, Client model, Verifier, Trigger, Operator, Registry lifecycle, Error catalog, and Decisions; `architecture/wyrd-doctrine.mdx`; `architecture/references/doctrine/positioning-and-vocabulary.md`; `architecture/references/architecture/patterns.md`; `architecture/references/languages/errors.md` | `architecture/wyrd-design.md`, `wyrd-spec` Operator and connection contracts, IDs, error catalog, composition extraction, schemas, and fixture consumers inspected. |
| Credentials, tenant isolation, authorization, audit, outbound delivery, and SSRF | `architecture/wyrd-security-posture.md`; `architecture/agent-rules.md`; `architecture/references/languages/agent-harness.md`; `architecture/references/languages/rust-core.md` Postgres/SSRF/Secrets sections | Migration, `TenantConn` and `OperatorPool` query paths, CRUD service, audit composition, key owner, registration authority check, `ScreenedHttp`, redirect handling, and security tests inspected. |
| Durable dispatch, retries, leases, deadlines, and verification settlement | `architecture/wyrd-design.md` Operator and Coordination clock; `architecture/references/domain/evaluation.md`; `architecture/references/domain/analytical-operations-reliability.md`; `architecture/references/languages/rust-core.md` | Dispatch schema and queries, shared settlement types, claim loop, worker, provider modules, runner integration, metrics, and Postgres/server tests inspected. |
| HTTP, runtime OpenAPI, CLI, and MCP projections | `AGENTS.md` §§9–11; `architecture/references/architecture/patterns.md`; `architecture/references/languages/agent-harness.md`; `architecture/references/languages/errors.md` | Typed routes, instrumentation, runtime router registration, OpenAPI contract test, CLI body-file path, MCP descriptors/scope filtering/dispatch, and MCP journey inspected. |
| Shared Rust client and Rust SDK | `AGENTS.md` §§2–3, 5, 9; `architecture/wyrd-design.md` Client model; `architecture/references/architecture/patterns.md`; `architecture/references/languages/rust-core.md` | `wyrd_client::OperatorConnections`, SDK re-export, and Rust real-server journey inspected. |
| Python/PyO3 package, exports, stubs, typing, and journey | `AGENTS.md` §§7–8, 11, 16; `architecture/references/languages/pyo3-boundaries.md`; `architecture/references/languages/python-api-and-stubs.md`; `architecture/references/languages/testing-workflows.md`; `architecture/references/languages/errors.md` | Native wrapper/registration, public `wyrd.operators` package, assembled stubs, unit typing tests, and real-server integration journey inspected. |
| TypeScript/N-API package, declarations, typing, and journey | `AGENTS.md` §§2–3, 11; `architecture/references/languages/typescript-guide.md`; `architecture/references/languages/testing-workflows.md`; `architecture/references/languages/errors.md` | Native wrapper, public TS closed unions, generated declarations, error-code projection, unit tests, and real-server integration journey inspected. |
| Generated schemas, fixtures, testing support, and tooling | `AGENTS.md` §§8, 11–12, 16; `architecture/agent-rules.md`; `architecture/references/languages/testing-workflows.md`; `architecture/references/languages/agent-harness.md` | Schema source registration, paired generated/schema-test artifacts, test harness changes, exact focused commands, broader recorded lanes, and `mise.toml` typing lane inspected. |

No Bifrost ingest/query/storage architecture was changed by this candidate, so `architecture/bifrost-design.md` and the Bifrost domain implementation references do not govern the implementation beyond existing journey regression coverage.

## Rule-by-rule evidence

| Applicable rule | Result | Exact evidence |
|---|---|---|
| Server owns durable behavior; clients project one typed wire contract through `wyrd-client`. | PASS | Contracts are in `crates/wyrd-spec/src/operator_connection.rs`; durable CRUD/delivery is in `wyrd-server`; `crates/shared/wyrd-client/src/operator_connections.rs` is the sole shared client; the three SDKs wrap it. No SDK introduces persistence, key resolution, or a parallel transport. |
| `wyrd-spec` remains IO-free, async-free, and PyO3-free. | PASS | Added spec code contains validation/serialization/schema types only. No runtime, network, filesystem, SQL, or PyO3 dependency entered the crate. |
| Stateful workflows have concrete owners and discoverable inherent methods. | PASS | `OperatorConnections`, `OperatorConnectionControl`, `OperatorKeys`, `OperatorDispatchQueue`, `ClaimLoop`, `OperatorWorker`, and `OperatorDelivery` own their respective dependencies and workflows. Provider-specific wire mechanics are confined to private `slack` and `pager_duty` modules without a speculative trait. |
| Async is limited to IO or intentional composition of IO. | PASS | Client methods await HTTP; connection control awaits SQL/key work; key reads await blocking/Vault/SQL work; claim/delivery loops await SQL/network/cancellation. Pure contract validation and template/origin transforms remain synchronous. |
| Raw pools do not cross domain signatures; tenant SQL uses `TenantConn`; cross-tenant discovery uses `OperatorPool`; callees do not commit caller-owned transactions. | PASS | `crates/wyrd/wyrd-sql/src/queries/operator_connections.rs` and `operator_dispatches.rs` accept `&mut TenantConn<'_>` for tenant operations and `&OperatorPool` only for named discovery. Commits remain in server workflow owners, not SQL query functions. Tenant queries rely on forced RLS instead of manual tenant predicates. |
| PostgreSQL owns coordination timestamps and retry/lease eligibility. | PASS | The migration and dispatch SQL use `statement_timestamp()` for creation, next attempt, lease, retry, deadline, and ordering decisions. Rust supplies durations rather than a wall-clock timestamp. |
| Authorization decisions are transactionally audited and fail closed. | PASS | `components/operators/service.rs` opens the tenant transaction, evaluates `operators:read`/`operators:write`, appends through the canonical audit helper, and commits the decision with the operation or refusal. Engine-only delivery/rewrap transitions do not create authorization audit events. |
| Secrets use redacted types/`Debug`, never appear in Cards/public reads/diagnostics, and encrypted rows bind authenticated context. | PASS | Provider inputs use secret wrappers; `VaultKeysConfig`, `OperatorKeysConfig`, `OperatorKeys`, `SecretKey`, and `SealedSecret` redact debug output; public views omit ciphertext and secrets; `SecretIdentity` binds tenant, connection, provider, name, and secret version into AES-GCM AAD. Redaction tests cover public problem JSON and configuration/debug output. |
| User-supplied destinations are screened and pinned before credentials are attached, including redirects. | PASS | `verification/operators.rs` builds a screened request through `ScreenedHttp`, validates same-origin authority, and applies credentials only to the screened request. The modified OIDC screening owner exposes the existing pinning mechanism; delivery tests cover forbidden networks and origin-changing redirects. |
| Public errors come from the derive-backed Wyrd catalog and internal errors do not leak provider/database/filesystem strings. | PASS | New stable Operator variants are in `crates/wyrd-spec/src/error.rs`; HTTP uses the shared response mapper; Python/TypeScript/CLI/MCP project the same problem data; key/provider failures collapse to bounded selector-free classes. |
| Typed write handlers are trace-instrumented with secret bodies scrubbed. | PASS | All five handlers in `components/operators/routes.rs` use typed contracts and `#[tracing::instrument]`; create/update skip raw bodies and caller/state. |
| MCP reads remain discoverable and writes require explicit scope plus normal server authorization. | PASS | `mcp/operators.rs` always advertises list/get, exposes create/update/disable only through `write_descriptors`, checks `Permission::operators_write`, and delegates every operation to `OperatorConnectionControl`, which authorizes and audits again. |
| Python boundary placement, registration, public exports, stubs, and runtime tests are complete. | PASS | Wrapper code is under `sdks/wyrd-sdk-python/src/operators.rs`; registration is in the SDK root; public exports are under `python/wyrd/operators`; generated stubs and public import/typing tests exist; the integration test drives the real server. No `Bound` value crosses an await and no ad hoc runtime is created. |
| TypeScript is a thin N-API projection with closed unions, generated declarations, and a journey test. | PASS | `native/src/operators.rs` delegates to `wyrd-client`; `wyrd/src/index.ts` projects closed provider-tagged inputs/views; N-API declarations are updated; unit and integration tests cover typing and server behavior. |
| New user-facing HTTP/Rust/Python/TypeScript/CLI/MCP behavior has primary user-journey coverage. | PASS | `pg_operator_connection_routes.rs`, Rust SDK `operator_connections.rs`, Python and TypeScript integration tests, CLI lifecycle additions, MCP `operators.rs`, and provider delivery integration tests exercise the shipped boundaries. Unit tests support rather than replace those paths. |
| Generated artifacts are source-derived and verified by repository lanes. | PASS | Generator registrations were updated with the checked-in schemas; Python assembly source was updated; runtime OpenAPI is tested through `pg_openapi_contract.rs` rather than checked in. Implementation evidence records `codegen:check`, `ts:napi:check` coverage through the TypeScript lanes, and type checks as green. |
| Cargo features and dependency cost are earned and narrow. | PASS | No Cargo feature was added. The only new dependency edge is existing workspace `zeroize` in `wyrd-server`, the crate that owns in-memory key/token cleanup. |
| Non-test environment/filesystem/parsing/configuration paths return errors; `expect` is reserved for a true invariant. | **FAIL — STD-R3-001** | `OperatorKeys::new` at `components/operators/keys.rs:263-265` panics for an out-of-range configured version. The claimed validation invariant is not universal: `WyrdServerConfig::validate` calls Operator-key validation only when `role.serves_api()` (`config.rs:2869-2876`), while `attach_config_fields` constructs `OperatorKeys` for the built state (`boot/mod.rs:1491-1500`). A non-API target can therefore parse `2147483648` and panic during boot instead of returning a typed configuration error. |
| Every new/materially modified Rust item, including private and trait methods, has meaningful rustdoc. | **FAIL — STD-R3-002** | Newly added items lack rustdoc, including `HttpsOrigin::fmt` and `HttpsOrigin::deserialize` (`wyrd-spec/src/operator_connection.rs:135-145`), `SealedSecret::fmt` (`wyrd-sql/src/queries/operator_connections.rs:117-122`), and `OperatorDispatchQueue::default` (`wyrd-sql/src/queries/operator_dispatches.rs:154-157`). This is the explicit `BLOCK_BEFORE_MERGE` rule in `architecture/agent-rules.md` and `AGENTS.md` §16; public-item compiler lints do not cover these trait methods. |
| Types are imported at module top and used by bare name in fields, parameters, return types, trait bounds, and `where` clauses. | **FAIL — STD-R3-003** | Candidate code uses fully qualified types in governed positions: `OperatorKeysConfig.active_version: std::num::NonZeroU32` and its default return (`config.rs:1855,1877-1878`), `read_body(path: &std::path::Path)` (`wyrd-cli/src/operator_connection.rs:118`), and `std::fmt::{Formatter, Result}` in new `Debug` signatures (`wyrd-sql/src/queries/operator_connections.rs:117-118`, `components/operators/keys.rs:225-230`, `config.rs:1820-1824,1864-1868`). The repository rule explicitly requires top-level imports and bare signature/field names. |
| No gate was weakened or bypassed. | PASS | No new `#[allow]`/`#[ignore]`, boundary-glob broadening, or deleted failure proof was found in the cumulative diff. `mise.toml` adds the Operator typing test and tightens unused-ignore checking rather than suppressing it. |

## Material findings

### STD-R3-001 — Reachable non-API boot panic violates configuration error handling

- Classification: `VIOLATION`
- Violated rule: `AGENTS.md` §4 and `architecture/references/languages/rust-core.md` Error handling require environment/configuration failures to return errors and reserve `expect()` for true invariants.
- Location: `crates/wyrd/wyrd-server/src/components/operators/keys.rs:263-265`; enabling path at `crates/wyrd/wyrd-server/src/config.rs:2869-2876` and `crates/wyrd/wyrd-server/src/boot/mod.rs:1491-1500`.
- Evidence: `active_version` is a `NonZeroU32`, so TOML/environment accepts values through `u32::MAX`. Its range validation is skipped for roles that do not serve the API, but `OperatorKeys::new` unconditionally narrows with `expect` when config-derived state is attached.
- Observable consequence: a worker-only or otherwise non-API server target configured with `WYRD_OPERATOR_KEK_ACTIVE_VERSION=2147483648` aborts boot with a panic rather than producing `ConfigError`/`ServerBootError`. The constructor is also public and accepts the same invalid state directly.
- Required correction: enforce the persisted `i32` range for every role before state construction, and make `OperatorKeys::new` reject rather than panic if its public input is still invalid. Add a focused non-API configuration/boot test proving `i32::MAX + 1` returns a typed error and never unwinds; retain the existing `i32::MAX` exact-value proof.

### STD-R3-002 — Newly added Rust items omit mandatory rustdoc

- Classification: `VIOLATION`
- Violated rule: `AGENTS.md` §16, `architecture/agent-rules.md` Rust documentation rule, and `architecture/references/languages/rust-core.md` Documentation require meaningful rustdoc for every new or materially modified item regardless of visibility; missing documentation is `BLOCK_BEFORE_MERGE`.
- Location: representative confirmed sites are `crates/wyrd-spec/src/operator_connection.rs:135-145`, `crates/wyrd/wyrd-sql/src/queries/operator_connections.rs:117-122`, and `crates/wyrd/wyrd-sql/src/queries/operator_dispatches.rs:154-157`.
- Evidence: the new trait methods have no `///` documentation. They are not inherited implementation detail exempted by the rule, and ordinary `missing_docs` compiler coverage does not require docs on trait implementation methods.
- Observable consequence: maintainers lack the required explanation of normalization/display, deserialization rejection, redaction, and default delivery-ceiling invariants at the exact implementations that enforce them; the candidate fails a repository hard acceptance criterion even though it compiles.
- Required correction: document every undocumented item introduced or materially modified by the cumulative candidate, including private/test and trait-implementation methods. The docs must explain workflow role and relevant redaction/default/validation invariants; add `# Errors`, `# Panics`, and cancellation/partial-progress sections where the item can fail, panic, or make async/durable progress. A source audit of all changed Rust declarations is the focused closure proof.

### STD-R3-003 — Candidate signatures and fields bypass the mandatory import manifest style

- Classification: `VIOLATION`
- Violated rule: `architecture/agent-rules.md` requires every `use` at module top and bare type names in fields, function parameters, return types, trait bounds, and `where` clauses.
- Location: `crates/wyrd/wyrd-server/src/config.rs:1855,1877-1878`; `crates/wyrd/wyrd-cli/src/operator_connection.rs:118`; `crates/wyrd/wyrd-sql/src/queries/operator_connections.rs:117-118`; `crates/wyrd/wyrd-server/src/components/operators/keys.rs:225-230`; `crates/wyrd/wyrd-server/src/config.rs:1820-1824,1864-1868`.
- Evidence: those positions spell `std::num::NonZeroU32`, `std::path::Path`, and `std::fmt::{Debug, Formatter, Result}` inline instead of declaring the module dependencies in their top-level import blocks and using bare names.
- Observable consequence: the changed modules no longer expose their complete dependency surface in the required top-of-file manifest, and the candidate directly violates an explicit repository source-shape rule.
- Required correction: import the existing standard-library types once at each module top and use their bare names in every candidate-added signature/field/impl position. `cargo fmt` plus a source search over the cumulative changed Rust declarations is sufficient focused proof; no runtime test is needed.

## Verification notes

- Independently run during this review: `git diff --check f8811ac5035c3aa165d34c38992f9889b3c9081f..57ee8dd0b3dddc3e1da34bf543ecc566bae2e831` — exit 0.
- Candidate identity was re-read after inspection and remained `57ee8dd0b3dddc3e1da34bf543ecc566bae2e831`.
- The candidate's committed implementation evidence records exit 0 for the focused r2 tests and the broader SQL/shared/Wyrd/server/CLI/MCP/Rust/Python/TypeScript journeys, type checks, code generation, tenancy/client/PyO3/unwrap boundaries, formatting, lints, Python lint/format, and diff check. Those results were reviewed as available evidence, not independently re-run in this review.
- Credentialed live Slack/PagerDuty smoke remains intentionally gated release evidence and was not run; local mock journeys cover protocol and failure classification.
- Green Clippy/codegen/test lanes do not close `STD-R3-001` (the skipped-role path is untested), `STD-R3-002` (trait-implementation rustdoc is not compiler-enforced), or `STD-R3-003` (the import-shape rule has no cited gate).

## Overall result

**FAIL**

The candidate substantially follows the Operator architecture, security, tenancy, SDK, generated-contract, and journey-test rules, but the reachable configuration panic and two explicit source-shape hard requirements prevent repository-standards approval.
