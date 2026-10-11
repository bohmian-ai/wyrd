# Independent repository standards review

Result: **FAIL** — one bounded rustdoc finding, `REPO-004-1`. Other reviewed rules pass; two import/signature style notes are non-blocking.

## Immutable subject

Repository: `/Users/stevenforrester/Documents/GitHub/local-server-bootstrap-task-004`.
Base `cb5656b2b913dbd88a8f6736bf4a0baaf998960c`; candidate `741e42ed97ac3ff9638217bbcd4f0c0583e6d464`.
The complete cumulative diff includes the implementation and cleanup. Separate documentation range: `2044e161e1a2020f5bb87cc6a36ee893ab311340..aecdaa05229d3571dedd362e8344fc28da5808a5`, inspected with Git without checkout. No source edits or verification builds were made by this reviewer. The starting map was expanded using source and caller searches. No `.codegraph` directory exists.

## Authority coverage

All rows are additionally governed by `AGENTS.md`, `architecture/agent-rules.md`, `architecture/wyrd-design.md` (Doctrine and Client model), and `architecture/wyrd-doctrine.mdx`. Focused references were selected through `architecture/references/README.md`; references do not supersede those authorities.

| Changed surface | Owning source and consumers inspected | Additional applicable authority |
| --- | --- | --- |
| CLI startup, installation reuse and dispatch | `server/{dev,install,mod}.rs`; `DevArgs`, `DevServer`, `ProvisionedTenant`; existing `ServerInstaller`/`InstalledServer`; migrate/setup/run ownership; `pg_tests` journeys | `references/architecture/patterns.md`; `references/doctrine/architecture-constraints.md`; `references/languages/{rust-core,maintainer-style,errors,agent-harness}.md`; `operations/{README,deployment-and-release,reliability-and-recovery}.md`; `wyrd-security-posture.md` |
| Protected configuration and credential extension | `global_config.rs`, `credentials_file.rs`; shared `saved_login.rs` and `auth.rs` readers/writers; `ClientConfig`; new preservation unit test | Architecture patterns/constraints, Rust core, errors, maintainer style; security posture (credential confidentiality, atomic replacement); deployment configuration authority |
| MCP host prompting/installation | `mcp/{hosts,mod}.rs`; existing `HostInstaller`, `InstallArgs::run`, `prompt_selection`; shared answer grammar and host-file editors; selected-host journey | Agent harness, Rust core, maintainer style; security posture; `AGENTS.md` §§2, 9 |
| Stable CLI failure and shared error projection | `error.rs`, `WyrdCliError::ServerDev`, existing derive/catalog and `From<WyrdCliError>` adapter | `references/languages/errors.md`, agent harness; `AGENTS.md` §4 |
| Rust manifest/lockfile | CLI `Cargo.toml`, workspace `Cargo.toml`, `Cargo.lock`; workspace Rustix/Tokio declarations and narrow feature additions | Rust core; `AGENTS.md` §§2–4; dependency-cost and feature rules |
| Python installed CLI projection | `sdks/wyrd-sdk-python/src/cli.rs`, SDK manifest/pyproject; existing root/submodule registration and unchanged exported signature | `references/languages/{pyo3-boundaries,python-api-and-stubs,rust-core,errors}.md`; `AGENTS.md` §§7–8 |
| TypeScript installed CLI projection | `wyrd/src/cli.ts`, native manifest, package checker and packed invocation; literal CLI subcommand shape | `references/languages/typescript-guide.md`; architecture patterns; `AGENTS.md` §§2–3, 11 |
| CLI integration and SDK runtime tests | `tests/{cli,pg_tests}.rs`, existing MCP helper; Rust `local_development.rs`, Python `test_local_development.py`, TS `local-development.test.ts` | `TESTING.md`; `references/languages/{testing-workflows,spec-driven-development,implementation-execution,maintainer-style}.md`; language guides; `AGENTS.md` §§11, 16 |
| Mise/package verification plumbing and removal of standalone runner | `mise.toml`, `scripts/check_ts_package.mjs`; corrected final task evidence; installed Python/Node fixture invocation | Testing workflows, implementation execution, spec-driven development; `AGENTS.md` §§11–12 |
| Current published documentation | CLI reference and local-development guide; actual argument/env/config/recovery owners | Wyrd doctrine, deployment/release, security posture, errors; `AGENTS.md` §§1–2, 11 |
| Separate draft documentation and Quickstart prerequisite packet | Complete separate three-file diff; candidate developer-docsite-rebuild spec and TASK-002 prerequisite addition | `SPEC-developer-docsite-rebuild@2` REQ-006/010/013/014/016; original TASK-004 Docsite Rebuild Update; Wyrd doctrine, deployment/release, security posture |
| Task evidence | Original TASK-004 including corrected final selectors and failure diagnoses | Spec-driven development and implementation execution; `AGENTS.md` §§11–14 |

No Bifrost ingest/query/maintenance implementation, schema, SQL query, UI component, migration, or new runtime binding declaration is changed. The memory input is inherited by the ordinary server; analytical ownership is unchanged. Consequently Bifrost engine-specific references, Svelte UI rules, and served-OpenAPI validation are not independently applicable to this write set.

## Rule results

| Rule | Result | Source/evidence |
| --- | --- | --- |
| Durable work stays Rust/server-owned; SDKs and CLI project existing owners (`AGENTS.md` §§2–3, 9) | PASS | `DevServer::migrate` invokes installed `migrate`; `provision` invokes installed `setup`; serving invokes the normal installed binary. No client DDL or key issuance is introduced. SDK entrypoints dispatch Rust. |
| One existing credential/config format and authoritative protected writer | PASS | `GlobalConfig::{local_api_key,save_local}` use `CredentialsFile`; `config_at` selects the sibling file with the same lock/atomic replacement. Existing saved-login and token-cache callers continue using `at` and the same directory lock. The credential permission mask remains `0o077`; only the non-secret configuration sibling permits read access. |
| Public errors derive their metadata; no parallel catalog (`AGENTS.md` §4) | PASS | `ServerDev` carries `#[wyrd_error(code = "WYRD_CLI_503_SERVER_DEV", ...)]`; the existing adapter handles its projection. No generated error list was hand-edited. |
| Typed durable identities and secret-bearing values | PASS | `DevArgs.tenant: TenantSlug`, `ProvisionedTenant.id: DataTenantId`, `key: Option<SecretString>`. Database input and captured setup stdout are wrapped in `SecretString`; no secret-bearing new type derives printable Debug. The tenant-key prefix check is a local refusal before actual shared-client authentication, not replacement server authorization. |
| Crate cost/feature direction | PASS | Rustix was already declared by the workspace; terminal capability is enabled only on the Unix CLI dependency. Tokio process support belongs at the CLI process owner. Production dependencies do not acquire SQL/cloud/analytical/PyO3 behavior. No new optional Cargo feature or wildcard version is introduced. |
| Struct-centered retained state, without utility owners (`AGENTS.md` §5) | PASS | `DevServer` retains verified installation, listener, endpoint, memory input and serving child. `ProvisionedTenant` retains identity and one-time key. Persistence extends existing owners; free orchestration helpers retain no cross-call resources. |
| Async earns IO and shared runtime is reused (`AGENTS.md` §6) | PASS | New async startup stages await child IO, HTTP, signal/terminal IO or intentionally compose those operations. Parsing, key checks, configuration and answer grammar stay synchronous. Python detaches and uses `wyrd_runtime::runtime()`. |
| SQL capability signatures/fields; no client SQL | PASS / N/A | No changed production function or field accepts `PgPool`, naked connection/transaction, `TenantConn`, or `OperatorPool`. The only fixture access is `UnmigratedDatabase::migrator_pool().connect_options()` to form the external DSN; the fixture does not migrate or query for the CLI. Thus a green isolation check is not substituted for a source audit. |
| Server authorization, tenant and audit boundaries | PASS | Provisioning goes through server-owned setup/public authority. Fresh verification uses existing `WyrdClient` and `Cards`. Host setup changes only local launcher configuration. No new permission decision, audit writer, cross-tenant query or server URL-fetch boundary is implemented here. |
| PyO3 lifetime/GIL and six-layer projection (`AGENTS.md` §§7–8) | PASS | `run_wyrd_cli` retains the existing `#[pyfunction]` zero-argument signature and registration, extracts argv before detachment, and does not retain `Bound` values across async work. Signal exception handling is at the Python edge; no new Python export or stub signature is needed. No generated stub is edited. |
| TS projection, declarations and foreign-runtime ownership | PASS | Only installed entrypoint signal setup changes; native/TypeScript public signatures and generated declarations are unchanged. `server dev` is matched before calling the existing dispatcher; adjacent commands retain normal Node signal behavior. The new saved-client test uses typed `Cards`/`WyrdClient`, without private native imports. |
| Journey tiers and runtime ownership | PASS | Native CLI `pg_tests` drive real compiled processes and an unmigrated external database. SDK write/read assertions run in the owning Rust/Python/Node runtime. Python tests remain top-level functions and public imports. No Rust test initializes an interpreter or V8 lifetime. |
| External test placement/test exception validity | PASS | New CLI module is included in the existing test target; it spawns compiled CLI/server and accesses real Postgres, earning external placement. Installed-CLI subprocesses are fixture/setup under an installed-command acceptance obligation; the saved-client SDK test bodies use public typed actions. The new Rust ignored and TS/Python conditional cases require the external installed-command fixture and are explicitly selected by `test:local-dev:journey`; they do not disable previously failing tests. |
| No fixture setup substitutes for CLI provisioning | PASS | `LocalDev::new` prepares `UnmigratedDatabase` and release files only. Python `installed_local_cli` prepares paths/env and launches installed CLI; it performs no migration or credential issuance. Fresh SDK bodies write and read against the resulting saved deployment. |
| Correct test selectors and repository toolchain | PASS | Final evidence corrects native names to `pg_tests::...`; the lane invokes exact nextest expressions in mise's environment. SDK Rust exact expression is inside its installed-CLI fixture. Python node IDs and TS named test are selected explicitly. Older commands reference the removed runner, but final evidence supersedes them and current lane contains no runner reference. |
| Gate integrity / diagnosed fixes | PASS | Reviewed diff contains no assertion removal, timeout inflation, existing-test ignore, lint allowance, or boundary-glob weakening. Failure evidence diagnoses signal chaining, stale emitted package, tty alias/kqueue, Python fixture trust/type issues and syntax/lint fixes at their owners. New runtime-conditional tests are fixture gates, not repairs that turn a failing selected case into success. |
| Rustdoc on every touched item, errors and panic invariants (`AGENTS.md` §16) | FAIL | `REPO-004-1` below. New private helpers and test helpers are included in this audit. Other new types/fields/variants/stages and tests have intent/workflow documentation; partial progress is documented on startup/persistence paths. |
| No historical/legacy/task vocabulary in permanent code | PASS | Changed production sources remain Wyrd-native. Task history is retained only in planning/evidence packet. |
| Current docs agree with delivered ownership and inputs | PASS | Command/flags, database prerequisites, local persistence, explicit upgrades, terminal-only root disclosure and administrative recovery correspond to the owning code. No checkout build fallback or hidden automatic key recovery is documented. |
| Draft docsite publication and Quickstart scope | PASS | Setup and Troubleshooting remain `draft: true`, `status: partial`; absent Quickstart gets a prerequisite instruction in its existing task rather than a fabricated published page. Setup stays local; recovery detail goes to Operate. |
| Applicable static checks and verification provenance | PASS with limits | Task records post-cleanup focused journeys, production Python, fmt/lints and Python typing/unit checks; earlier unchanged codegen/dependency/TS/docs checks remain relevant. Current diff does not change generated signatures or schema source. Linux startup proof is explicitly isolated; macOS cannot execute its native binary inside the Ubuntu container. Draft accessibility check renders no published page, explicitly acknowledged. |

## Material finding

### REPO-004-1 — Incomplete required rustdoc on new fallible and invariant-bearing helpers

Classification: **VIOLATION**. Rule: `AGENTS.md` §16 requires `# Errors` on every fallible Rust function and `# Panics` whenever a panic remains possible, regardless of visibility; `architecture/agent-rules.md` repeats that this is `BLOCK_BEFORE_MERGE`. The task-review skill explicitly keeps missing rustdoc blocking.

Exact locations and evidence:

1. `crates/wyrd/wyrd-cli/src/mcp/hosts.rs:734`, `show_hosts`: only a one-line intent comment; returns `io::Result<()>` and propagates terminal write errors without `# Errors`.
2. `crates/wyrd/wyrd-cli/src/mcp/hosts.rs:749`, `show_question`: returns `io::Result<()>`, propagates write/flush failures, and has no `# Errors`.
3. `crates/wyrd/wyrd-cli/src/server/dev.rs:330`, `DevServer::ready`: documentation has `# Errors` but omits `# Panics`; the body explicitly executes `self.child.as_mut().expect("readiness follows spawn")`. Its `start` caller creates the child immediately before invoking readiness, so the omitted rule is the owner's required lifecycle invariant rather than a proposed new failure guard.
4. `crates/wyrd/wyrd-cli/tests/pg_tests.rs:235`, `LocalDev::environment`: documentation has no `# Panics`, while `.to_str().expect("UTF-8 home")` explicitly assumes UTF-8 fixture paths. `client` and `stop` call this helper.

Observable consequence: required source documentation is incomplete and fails the repository's explicit merge/completion gate, even though default Clippy does not inspect these private documentation obligations. Maintainers cannot read the required write/flush and readiness/fixture panic contracts at their owning helpers.

Smallest testable correction: document the existing errors and panic invariants at these four symbols. Preserve bodies, signal/cancellation behavior, prompt grammar, tests and assertions. No new guard, type, harness or runtime test is required for a rustdoc-only correction. Prove closure by reading all four final docs against their complete bodies and run `mise run fmt`; use the relevant existing lint lane if the implementation write set also changes. This is one batched documentation boundary, not four remediation rounds.

## Non-blocking notes

- `mcp/hosts.rs:692–693` introduces function-scoped `use rustix::fs::{Mode, OFlags}` and `use tokio::io::unix::AsyncFd`, contrary to the top-of-module import rule in `agent-rules.md`. Moving these imports to cfg-gated module scope would satisfy the rule, but this creates no runtime, security or public-contract consequence and does not independently block task acceptance.
- `dev.rs:577` spells `std::io::Result` in a new signature instead of a module import, contrary to the same authority's bare signature-type convention. This is likewise non-blocking and requires no behavior change.

## Verification limits

This was a static standards audit plus inspection of available final verification evidence; it did not rerun tests, package builds, live official-release downloads, forced terminal-write failures or draft-page accessibility rendering. No unsupported claim about those checks is made. The material finding is source-proven and independent of those limits.
