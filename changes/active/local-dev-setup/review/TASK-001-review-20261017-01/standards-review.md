# Repository standards review

Result: **FAIL**

## Immutable subject and scope

Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/local-dev-setup`.
Base: `bc3bf25e3f78b0c429110350a3de4d58290e632f`.
Candidate: `5cfb0a3ac61434256fa97a86f3506b124158780c`.
The complete cumulative `git diff <base>...<candidate>` was inspected, including all 36 changed paths, surrounding owners, tests, configuration, and relevant consumers. HEAD remained the candidate during this pass. No `.codegraph/` exists. No other reviewer's report was read.

This report evaluates repository standards, not the task's acceptance verdict or a Ponytail redesign. The supplied navigation map was expanded to the HTTP ingress limiter, credential-file owner, storage URL parser, restricted credential writer, CLI test registration, and package-inventory checker. External example working trees have no supplied immutable commit range and are outside this source verdict; their claimed results cannot establish immutable external compliance.

## Authority coverage

All paths below also fall under `AGENTS.md`, `architecture/agent-rules.md`, `architecture/references/README.md`, `architecture/references/languages/spec-driven-development.md`, and `architecture/references/languages/implementation-execution.md`. The approved spec and original task were read to distinguish authorized contract changes from a standards violation; their implementation summary was treated only as verification claims.

| Changed surface / paths | Applicable authority inspected | Source and consumer coverage |
|---|---|---|
| CLI `server/dev.rs`, new `terminal.rs`, `lib.rs`, `mcp/hosts.rs`, `mcp/mod.rs`, `platform/credential.rs` | `wyrd-design.md` Doctrine and Client model; `wyrd-doctrine.mdx`; `wyrd-security-posture.md`; `operations/deployment-and-release.md`; references `architecture/patterns.md`, `doctrine/architecture-constraints.md`, `languages/rust-core.md`, `languages/maintainer-style.md`, `languages/errors.md`, `languages/testing-workflows.md` | Full changed items; DevArgs input selection, DevInputs, DevServer lifecycle, terminal echo guard, saved client construction, existing platform recovery and host selection |
| Shared client `global_config.rs`, `config.rs`, `gateway.rs`, `gateway_credential.rs` | Same client/security/Rust authorities; AGENTS §§3–6, 9, 16 | Protected `CredentialsFile` locking/document/replace entry points; config loading; new DevSection/DevDatabase types; CLI writer versus SDK Gateway boundary; transport delegation |
| Pure gateway contract `wyrd-spec/src/gateway/deployment.rs`, `mod.rs` | Design/Doctrine; AGENTS §§2–5, 9, 16; references patterns, Rust core, errors, agent harness | New request/response/validator, schema derives, typed identifiers, SecretBearer-backed credential source, module re-exports and unit test |
| Server gateway `service.rs`, `routes.rs`, `mcp/gateway.rs` | Security posture authorization, tenant isolation, cryptography and audit sections; AGENTS §§4–6, 9; agent-rules SQL capability/transaction/audit rules; references Rust core, patterns, errors, agent harness | Full new apply path, shared write_credential/write_deployment bodies, audited owner, TenantConn access, source validation/sealing, mounted HTTP registration, MCP catalog/gating/dispatch and global bounded ingress |
| Bifrost local-root selection and shutdown supervision | `bifrost-design.md` System boundary, Live tail/recovery/shutdown, Resource and failure invariants; deployment operations | CLI only supplies the canonical data-root setting and supervises ordinary server shutdown; no analytical engine, WAL, storage format or query owner changed |
| Rust tests: CLI `gateway_server_journey.rs`, `pg_tests.rs`; MCP connectivity/discovery/gateway; server pg_administration_tests and pg_openapi_contract | AGENTS §§11–12, 16; agent-rules test placement/runtime/doc rules; references testing workflows and maintainer style | Full changed test bodies and helpers; CLI test root; real server/database fixture use, ignored journey-lane registration and precise commands in task evidence |
| Python `tests/integration/test_local_development.py` | AGENTS §§8, 11, 16; references Python API/stubs, testing workflows, maintainer style | New terminal conversation helper and guided/standalone tests, modified lifecycle helpers and provider-key fixture isolation; public imports; module integration marker and per-test local_dev marker |
| `mise.toml`, `scripts/check_ts_package.mjs` | AGENTS §§11–12; agent-rules gate integrity; testing workflows; TypeScript guide placement/declaration boundaries | Local-dev task additions and package archive inventory checker, including its existing exact allowed-path mechanism; no TS SDK source or generated declaration changed |
| `docs/` setup, quickstart, gateway, troubleshooting and MCP reference; `examples/gateway-setup/{README.md,gateway.json}`; changed operations/security architecture prose | Design/Doctrine, security posture, deployment operations; AGENTS §§2, 9, 11–12; agent harness, testing workflows | Every changed prose/document example inspected alongside the CLI/wire vocabulary and source owners; shipped gateway example contains variable names, not expanded credentials |
| Task evidence Markdown | Spec-driven development and implementation execution | Available exact proof recipes and claimed outcomes/limits; no artifact summary accepted as source proof |

Manifests inspected: workspace Cargo.toml and the touched spec/client/CLI crate manifests. No manifest or lockfile is changed by this range; no dependency, feature or version-specific API assumption was introduced by this review.

## Rule results

| Applicable rule | Result | Exact evidence / limit |
|---|---|---|
| AGENTS §§2–3: pure foundational contracts, server durable ownership, sole shared client | PASS | ProviderDeploymentsApply contains declarative vectors and synchronous validate; GatewayAdministration owns sealing and writes; CredentialWriter sends over its retained WyrdClient. No SQL/IO/runtime/PyO3 import added to wyrd-spec. |
| AGENTS §§2, 7–8: no new first-class SDK secret mutation or misplaced foreign-runtime logic | PASS | New mutation is on `gateway_credential::CredentialWriter`, not Gateway or language bindings. Python changed files are owning-runtime integration tests; no native export/stub change exists. |
| agent-rules: production SQL signatures use TenantConn/OperatorPool; callee does not end supplied transaction | PASS | `service.rs::write_credential` and `write_deployment` accept `&mut TenantConn<'_>` and do not commit/rollback. apply_deployments owns the transaction passed by audited, and commits after both write stages. No production raw PgPool/Transaction field or argument introduced. |
| AGENTS §9 and security posture: verified caller supplies tenancy; typed RBAC before protected writes | PASS | apply_deployments delegates to audited with `Permission::gateway_write()`; audited authorizes and derives tenant connection from `caller.data_tenant_id`. New wire request has no trusted tenant/principal field. |
| agent-rules and security posture: one nonblocking audit path, no audit transaction coupling | PASS | New apply calls existing audited -> `audit::authorize` before SQL. Neither write helper independently evaluates permission nor writes audit; no new sink/table/relay appears. |
| AGENTS §§4, 9: public errors are stable Wyrd errors; typed handlers; scrubbed tracing | PASS | New route takes Json<ProviderDeploymentsApply>, returns Json<ProviderDeploymentsApplied>, instruments skip_all, and uses GatewayResult/shared mapping. Shared writer returns existing WyrdError. No parallel catalog or hand-built public problem added. |
| Security posture cryptography/secret handling; AGENTS §4 | PASS for changed production secret handling | DB values use SecretString in resolved inputs; local_env becomes existing SecretBearer; response contains ProviderCredentialView. Protected saves reuse CredentialsFile. `BackendConfig::from_url` rejects credentials/query/fragment before server readiness and save_settings, so an environmental credential-bearing storage URL does not establish a new reachable successful secret-output path. No real secret is checked into the diff. |
| AGENTS §5: retained lifecycle and dependency state has concrete owner | PASS | DevServer retains child/config/install/key; Terminal retains AsyncFd; EchoGuard retains saved attributes; GatewayAdministration and GlobalConfig extend existing capability owners. |
| AGENTS §6: meaningful async; no ad hoc runtime | PASS on supported targets | New async CLI work awaits terminal/process/network IO; server write helpers await SQL. Pure document validation/name checking/config serialization remain synchronous. `cfg(unix)` includes all four supported macOS/Linux targets; no new supported-target feature exclusion. |
| AGENTS §11: public capability has real boundary proof; runtime ownership and fast-lane placement | PASS for source placement | CLI composite journey invokes compiled CLI against WyrdTestServer; MCP invokes tool then reads HTTP; server transaction proof uses PgFixture; Python PTY behavior stays in Python integration tests. No Python/Node runtime is initialized in Rust. New ignored Rust journey follows existing explicit journey lanes, not a failing-test suppression. Actual execution is not independently rerun. |
| AGENTS §11: served OpenAPI and runtime MCP verified independently of codegen | PASS for proof bodies | Changed pg_openapi_contract asserts collection request/response schema refs and errors; MCP discovery/connectivity plus gateway tool journey cover new catalog entry/dispatch/refusal. |
| agent-rules and AGENTS §12: no weakened gate or hand-edited generated artifacts | PASS | `check_ts_package.mjs:91` adds exactly README.md to the existing exact archive inventory for an already tracked package README; comparison remains exact and source-TS/symlink checks remain. No generated file changed. Existing retained assertion changes are accompanied by new stronger preflight/preservation assertions in pg_tests. |
| AGENTS §16: substantive rustdoc for changed items, including tests/constants, and panic sections | **FAIL** | REPO-001 below. Most production additions are documented, but test helpers, test function panic behavior and new constants are incomplete. |
| agent-rules: imported names in signatures | FAIL, non-blocking | `global_config.rs:98` names `std::path::PathBuf` in new config_dir return instead of importing PathBuf. No behavioral/security/contract consequence; it does not independently block this review. |
| AGENTS §§11–12: final execution evidence | Limited | Task claims P1–P12 and checks passed, but captured command logs are absent; external examples are uncommitted. Static `git diff --check <base>...<candidate>` passed in this review. No Cargo/build/codegen/lint/integration commands were launched, as instructed. |

## Material finding

### REPO-001 — Changed Rust test items miss mandatory rustdoc content

Classification: VIOLATION. Blocking under the explicit rustdoc exception in wyrd-task-review.

Authority: `AGENTS.md` §16 requires rustdoc for every new/materially modified Rust item, including private test helpers and constants, and a `# Panics` section whenever panic remains possible. `architecture/agent-rules.md` repeats this as `BLOCK_BEFORE_MERGE`; testing code has no exemption.

Locations and evidence:

- `crates/wyrd/wyrd-cli/tests/gateway_server_journey.rs:20` — new `gateway_env` uses `tempdir().expect`, `cargo_bin().expect` and `.output().expect` but has no `# Panics` section. The changed caller `gateway` at line 14 now delegates to this panicking helper, also without panic documentation.
- Same file line 463 — new `deployment_document_applies_local_environment` has many fixture/process/assertion panic paths but no `# Panics` section. Its new `VARIABLE` and `SECRET` constants at lines 464–465 have no rustdoc.
- `crates/wyrd/wyrd-server/src/components/gateway/pg_administration_tests.rs:1481` — new `apply_document` calls `serde_json::from_value(...).expect(...)` without `# Panics`; the new `SUBMITTED` constant at line 1496 has no rustdoc.
- `crates/wyrd/wyrd-mcp/tests/bifrost/mcp/gateway.rs:97` — newly added `APPLIED` sentinel constant has no rustdoc.

Consequence: The changed verification helpers hide their fixture/decoding/process assumptions from maintainers, and the candidate violates a repository hard completion gate even though compilation/lints may pass. This is a bounded documentation correction, not a demand to redesign or change tests.

Smallest testable correction: Add concise, substantive rustdoc to those new constants explaining their test role and add the concrete `# Panics` conditions to the changed helper/test functions. Preserve fixture behavior, assertions, test selection and existing errors. Batch with any substantive remediation; do not create a separate style-only round.

Closure proof: inspect these changed symbols against §16 and run the narrow relevant formatting/check recipe for the resulting write set. No new behavioral test/harness is needed for documentation-only closure.

## Non-blocking notes

The newly fully qualified PathBuf return listed above is a standards mismatch with no material consequence. It is not proposed as a remediation finding. No other optional improvements are requested.

## Verification limits

This is a source-only independent standards pass. No build, lint, codegen, database, provider, native-package or platform matrix test was run. All supplied green test claims remain claims supported by inspected proof bodies, not independently reproduced runs. No external sibling working tree was used as immutable review evidence. No source, manifest, task or other report was edited.
