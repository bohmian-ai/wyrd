# Task 002 repository standards review

**Verdict: FAIL.** Base `395274d58bf76a2075f245ca3a5c1bd044925027`, candidate `3dd1555a0f40117ee512ecaaff1f5b51845ef3b6`. Reviewed the full three-dot range. The approved revision 4 packet amendment at `46af20f00` supplies authority. No CodeGraph index was present.

## Authority coverage

Root `AGENTS.md`, `architecture/README.md`, `changes/spec-driven-development.md`, `TESTING.md` and `agent_docs/maintainer-style.md` apply throughout. The nested rules are `crates/wyrd/wyrd-sql/AGENTS.md`, `docs/AGENTS.md`, `sdks/wyrd-sdk-python/AGENTS.md` and `sdks/wyrd-sdk-ts/AGENTS.md`.

| Changed surface | Additional authority | Source and consumers inspected |
| --- | --- | --- |
| Rust contract, error catalog and schemas | Rust, errors and agent surfaces guides, Wyrd Design runtime identity and public surfaces | `wyrd-spec` contracts, errors, exports and schema example, TS generator, served OpenAPI and generated errors and wire types |
| Provider protocol, cryptography, auth and boot | Rust and errors guides, Security Posture trust boundaries, browser sessions, GitHub App association and outbound credentials, Operations ownership | `wyrd-crypt`, manifests and lock, `wyrd-auth` browser/login, server auth, boot, config, state, route composition and `components/github` |
| Tenant SQL, login state and migrations | SQL nested rules, Rust/errors guides, Postgres tenant binding, revision 4 storage amendment | Migration, `postgres.rs`, GitHub installation and auth login-state queries and exports, service caller, real Postgres race/RLS/routing proofs |
| HTTP, MCP, CLI, shared and Rust SDK clients | Rust/errors/agent surfaces guides, runtime identity and permission/audit authority | HTTP handlers and router, MCP projection and discovery, CLI dispatch and client, `wyrd-client/src/github.rs`, Rust SDK re-exports, OpenAPI and CLI/MCP/Rust journeys |
| Python SDK | Python nested rules, Rust/errors guides | PyO3 production and test bindings, registration, package exports, authored stubs, public integration and instantiation tests |
| TypeScript SDK | TypeScript nested rules, Rust/errors/agent surfaces guides | Native and test bindings, public facade, testing package exports, generated native/wire/error declarations, public integration and unit tests |
| Test support and runners | Rust guide, `TESTING.md`, workflow proof sections, SQL/SDK nested rules | Mock provider, test PEM, browser and server builder, server support, Rust/CLI/MCP/SQL/Python/TS test registration and `mise.toml` |
| Architecture, guide, references and active packet | Architecture normative rules, docs nested rules and `docs/CONTRIBUTING.md`, approved revision 4 spec/contracts/design and workflow scope/evidence | Runtime identity and Security Posture amendments, setup guide, MCP/error references, LLM files and active task packet |

## Rule results

| Rule | Result and evidence |
| --- | --- |
| Pure contracts and server-owned durable behavior | PASS. `wyrd-spec/src/github_connection.rs` uses serialization/schema/domain types. `wyrd-client/src/github.rs` uses the shared client transport. CLI, Rust SDK, PyO3 and N-API delegate to it. Provider and durable IO are in `GithubApp` and `GithubConnections`. |
| Stable errors and scoped projections | PASS for shape. Errors are derive-backed in `wyrd-spec/src/error.rs`. HTTP uses typed input/output, shared error mapper and scrubbed spans. `mcp/github.rs:35–78` advertises the read broadly and writes according to the existing permission. `service.rs:153–165` uses the existing permission/audit owner. `pg_openapi_contract.rs:304–352` asserts success statuses, schema refs and webhook security. |
| SQL capability signatures and caller transactions | PASS. `queries/github_installations.rs:184–340` and `queries/auth/login_state.rs:415–501` receive `&mut TenantConn<'_>` and never commit or accept raw executors. `service.rs:172,238,334` returns/takes that capability. The approved lookup at `wyrd-sql/src/postgres.rs:265–282` runs one definer statement inside the existing boundary using its private app pool. No production GitHub signature or field exposes `PgPool`, `PgConnection` or `sqlx::Transaction`. |
| SQL statement shape, timestamps and migration | PASS. A new migration adds one table, forced tenant RLS, both uniqueness boundaries and one narrow host/installation-keyed lookup with pinned search path and body-scoped restoration. Queries bind parameters, map named fields and SQLSTATE, and derive coordination times with `statement_timestamp()`. The unchanged raw query check explicitly permits the offline-bundle justification. |
| Exact verified installation and revocation fence | FAIL. See REP-GH-03 and REP-GH-04. |
| Changed Rust item documentation | FAIL. See REP-GH-01. No new production unwrap or lint suppression found. |
| Postgres fixture and journey placement | FAIL. See REP-GH-02 and REP-GH-05. Existing Rust SDK and MCP dedicated lanes explicitly include ignored journeys. Those annotations serve established lane selection. |
| Python and TS projection ownership and declarations | PASS for source shape. Python uses the shared lazy connection/runtime and exposes the class through native registration, package, authored stub and tests. TypeScript uses shared `connect_with`, keeps test-only types in the testing package, and asserts catalog codes. |
| Code generation and tier boundaries | PASS on source shape and recorded generation evidence. New roots originate in the spec example/generator, errors in the derive catalog, and native declarations track N-API exports. Python stubs are authored. No schema JSON, platform gate or incompatible runtime dependency tier was introduced. |
| Published docs and proof discipline | FAIL. See REP-GH-06 and REP-GH-07. The guide otherwise follows the expected goal, sequential steps, failure table and Next shape. Generated references have corresponding source changes. |

## Findings

### REP-GH-01 — VIOLATION — changed Rust items miss mandatory rustdoc

Root `AGENTS.md` and `agent_docs/rust.md` require every new or materially changed item to document relevant workflow and invariants, Errors/Panics where applicable, and cancellation/partial progress for async or durable operations. Concrete gaps include:

- Undocumented module items in `wyrd-client/src/lib.rs:22`, `wyrd-spec/src/lib.rs:19`, `wyrd-cli/src/lib.rs:19`, `wyrd-server/src/components/mod.rs:7`, `components/github/mod.rs:10–12`, `mcp/mod.rs:70`, `wyrd-sql/src/queries/mod.rs:16`, `wyrd-testing/src/lib.rs:7`, `wyrd-sdk-python/src/lib.rs:16` and both TS native `lib.rs` module declarations. Include the new CLI, MCP, server and SQL integration module registrations.
- Undocumented SQL test `HOST` in `pg_github_installations.rs:33` and new associated `TryFrom::Error` in `queries/auth/login_state.rs:372`.
- Missing Panics sections for CLI helpers `github`, `key`, `json`, `failed_with` at `github_journey.rs:21,35,46,56`; server fixture helpers `refused`, `minted` at `pg_github_connection.rs:24,33`; config helper `github_key_file` at `config.rs:6790`; served OpenAPI test at `pg_openapi_contract.rs:300`; and Rust SDK helpers `github`, `deployment`, `actor`, `Actor::connect`, `refused`, `assert_redacted`, `card_count` at `github_connection.rs:24,33,46,69,88,114,122`. They use assertions/expect or call a fixture that panics if setup fails.
- `service.rs:403–533` commits consumption before provider IO. Document that cancellation or provider failure after this commit spends the state and requires starting a new transaction.

Batch the documentation with behavioral corrections. Inspect the changed Rust range including tests and declarations, and run format and lints. This is an explicit merge gate.

### REP-GH-02 — VIOLATION — all new SQL tests can pass without Postgres

`crates/wyrd/wyrd-sql/AGENTS.md` Tests says Postgres tests never silently skip when an environment variable is absent. `pg_github_installations.rs:35–37,129,213,298,400,536` returns successfully from all five tests when `WYRD_DATABASE_URL` is missing, before starting `PgFixture` or asserting anything. A miswired invocation yields five green results without any uniqueness, RLS, revocation or flow-separation proof. Remove the helper and early returns. Keep the repository-managed fixture, run the focused tests with Postgres and prove that a direct invocation without its fixture fails visibly.

### REP-GH-03 — VIOLATION — token authorization loses the installation host

The revision 4 storage amendment and Security Posture GitHub App association distinguish `(host, installation_id)`. The migration and SQL enforce that pair, and `service.rs:570–603` uses it for webhook routing. In `service.rs:176–198,640–668`, the current deployment App/host labels the view and directs the provider request, while the row host is never checked. A deployment that changes to a different GitHub Enterprise server retains the verified slot from the old server. If numeric installation and repository IDs collide, the read advertises the old row as active under the new App and token issuance can grant access based on the previous server's selection. Compare the persisted association host to the configured host before projecting active access or requesting a token. Preserve disconnect so the stale slot can be released. Prove that a mismatched persisted host cannot be presented as the current active installation or initiate a provider token request. This enforces existing provider identity without introducing host-management behavior.

### REP-GH-04 — VIOLATION — revocation lock does not fence token issuance

Wyrd Design Runtime identity/GitHub App association and Security Posture Outbound credentials require an active association for minting tokens. `service.rs:322–342` claims the slot lock prevents token decisions from interleaving. At `service.rs:640–668` token issuance reads without a lock and commits before initiating the provider request. Disconnect can commit in between. GitHub still grants the deployment App access and returns a token for a tenant whose association was released. The sequential refusal at `pg_github_connection.rs:281` cannot expose the interleaving. Use the existing slot lock/conditional revision owner to order issuance and revocation and prevent stale access from producing a usable token. Prove the overlap with deterministic synchronization in the existing App mock and preserve unrelated work.

### REP-GH-05 — VIOLATION — real-server journeys lack the required source markers

Root `AGENTS.md` and `agent_docs/rust.md` require Postgres/server-backed tests to be in `mod pg_tests` or a `pg_*` file. `crates/wyrd/wyrd-cli/tests/github_journey.rs:69–86` starts a bound server at file top level. `sdks/wyrd-sdk-rust/tests/integration/github_connection.rs:147–149,266–268,458–460,545–547` runs a real deployment through the harness at file top level. Neither file qualifies. `TESTING.md:225–228` distinguishes source organization from test selection. Use the existing marker, update the exact selectors, and run both real-server journey lanes to prove discovery.

### REP-GH-06 — VIOLATION — published guide has unproven setup commands

`docs/CONTRIBUTING.md` Publishing requires a guide's code to come from a local example that a journey runs on a real server and its steps to be proved before publication. `docs/src/content/docs/operate/connect-github.md:48–55,64–67` publishes bare `<App ID>`, `<slug>`, `<Client ID>` and `<admin_credential>` placeholders in Bash assignments. The shell parses them as redirections. `github_journey.rs:19–30,88–123` proves the verbs and passes environment through a subprocess fixture. It never runs those snippets or exercises the operator setup step. No example under `examples/` is added or cited. Use valid substitutable assignments and the existing guide example/journey convention to prove the documented setup and CLI path. Run the focused CLI and docs checks.

### REP-GH-07 — VIOLATION — named verification lacks exact focused commands

Root `AGENTS.md`, `TESTING.md:91–111` and `changes/spec-driven-development.md:374–381` require the exact focused command for each named test. Task evidence at `002-github-app-connection.md:229–241` records module regexes, abbreviated `...` commands, and broad suites. For example, the named HMAC vector test is represented as the crypt library suite, and the named OpenAPI contract test has only a module filter. Neither has an executable exact selector. TypeScript nested rules additionally require format/lint/typecheck, but the recorded outcomes cite only N-API, codegen and the journey for that SDK. Run and record focused named proof with fixture wrappers and applicable TS checks. Preserve the disclosed historical absence of RED at line 245. New behavioral remediation should have genuine focused RED evidence.

## Verification limits

`mise run check:tenant-isolation` and `mise exec -- git diff --check 395274d58...3dd1555a0` passed independently. Other runtime, formatting and code-generation evidence is from the immutable source and task record. The token scenarios are source traces. The missing-environment false pass follows from the early returns.
