# Independent structured Ponytail validation

Recommendation: **FIX_REQUIRED**. Four bounded findings remain. No specification revision or new production orchestration is required.

## Subject, scope, and independence

Reviewed cumulative `cb5656b2b913dbd88a8f6736bf4a0baaf998960c..741e42ed97ac3ff9638217bbcd4f0c0583e6d464`, approved `changes/active/local-server-bootstrap/spec.md` revision 7, and original `tasks/TASK-004-local-dev-journey.md`, including cleanup and corrected final evidence. Reviewed the separate draft-docsite range `2044e161e1a2020f5bb87cc6a36ee893ab311340..aecdaa05229d3571dedd362e8344fc28da5808a5` through Git. HEAD remained the candidate and tracked source remained unchanged. There is no CodeGraph index.

Inputs include both implementation reports, standards, maintainer, system, security and persistence reports, focused follow-up, claim comparison, subject map, and verification report. Authority includes AGENTS, agent rules, design/doctrine, security posture, deployment/release, reference router, spec-driven development, maintainer style, and testing rules. This fresh validator inspected actual changed owners and surrounding callers; neither consensus nor a green command supplies source proof. Only this report was written. No broad tests were repeated.

Calibration: this is an attached local-development command using external Postgres. Ordinary process loss, mismatched saved endpoints, and declared loopback constraints matter. Hostile local endpoint attestation, automatic restart, another supervisor, and new durable fixture machinery do not belong in this remediation.

## Every discovery claim reconciled

| Discovery claim | Validation | Source-based disposition |
| --- | --- | --- |
| BEH-001 | REVISED → FIND-TASK-004-1 | Shared database across fresh warehouse/config lifetimes really fails. Revise the proposed per-case database machinery to existing dedicated-lane routing, preserving all required driver coverage. |
| SYS-001 | CONFIRMED → FIND-TASK-004-2 | The child is not observed while the cancellable host prompt is unanswered; the existing lifecycle owner must keep observing it. |
| SEC-001 | REJECTED | A hostile local impersonator can win the released listener and receive first-use root exchange in principle. No hostile co-user resistance is required by this clarified loopback development task; existing socket ownership has no installed-process handoff to reuse. An inherited-listener contract or endpoint-attestation protocol would introduce an unapproved boundary rather than close an ordinary task failure. |
| SEC-002 | REVISED → FIND-TASK-004-3 | Explicit saved `grpc_url` really overrides derivation after HTTP changes. Combine with SEC-003 at the missing effective local gRPC connection boundary. |
| SEC-003 | REVISED → FIND-TASK-004-3 | The ordinary gRPC default is already loopback. The actual failure is inherited non-loopback overrides and saved destination mismatch, not a hypothetical unsafe default. |
| MR-001, REPO-004-1 | CONFIRMED → FIND-TASK-004-4 | One rustdoc correction across four existing symbols; preserve bodies. |
| Empty invariant/persistence ledgers | Reconciled | Protected persistence, equal-key retries, server issuance and shared locking pass. Their broad lifecycle/client acceptance misses the specific prompt-exit and effective gRPC paths established below; an empty report does not negate those paths. |

The focused follow-up resolved SEC-001 sufficiently for a scope-grounded rejection. It correctly distinguishes source-level possibility from required local-host threat protection. That rejection is not an assertion that plaintext readiness authenticates a process. No additional follow-up or unresolved authority decision is needed.

## FIND-TASK-004-1 — Installed bootstrap drivers enter an incompatible shared-database lane

**Status:** REVISED. **Source ID:** BEH-001. **Classification:** REGRESSION. **Obligation:** AGENTS §§11–12; TASK-004 installed-runtime proof and cleanup must retain usable owning verification.

**Locations:** `sdks/wyrd-sdk-python/tests/integration/test_local_development.py:311–381`; `mise.toml:1204–1209` and `:607–612`.

The full `installed_local_cli` body creates and later deletes a different working directory, warehouse and configuration per driver, consumes the same inherited `WYRD_PLATFORM_DATABASE_URL`, and always asks for tenant `local`. `test_installed_cli_bootstrap` supplies four parametrized callers. The Postgres wrapper owns one database per invocation, not per parametrized case. Ordinary `py:test:integration:inner` and `test:bifrost:journey:python:inner` select this module and therefore all four drivers inside one already-migrated lifecycle. The first bootstrap leaves catalog/tenant state tied to its subsequently deleted directory. The next server correctly refuses catalog locations outside its new warehouse; even simply retaining storage would leave an active tenant whose one-time key lives in the first removed config. A consumer-side readiness change cannot fix that producer lifetime.

Behavior discovery's tracing-enabled Python/interrupt combined run returned **1 passed, 1 failed**, with real server `DataInvalid` / outside-active-warehouse diagnostics and CLI exit 69 before the terminal prompt. Source independently establishes that failure; the separately wrapped focused lane's success is entirely consistent with it.

**Selected minimum correction:** keep these installed-bootstrap drivers exclusively under the existing `test:local-dev:journey` lifecycle owner, whose four commands already supply one wrapper per deployment and whose TypeScript command additionally owns package installation. Route only those drivers out of ordinary shared-database Python integration/story selection, as the candidate already does for the Rust saved-bootstrap case. Retain all four explicit focused selections and all three SDK verification dependencies on this lane. This is required fixture ownership, not permission to delete/ignore a failing case or to weaken assertions. Document the routing with the existing lane descriptions. Do not add a Python unmigrated-database API, database cleanup implementation, standalone runner, fake credentials, or extra wrapper owner. Ordinary local-development SDK tests stay in their existing lanes.

**Sibling/consumer coverage:** four Python driver parameters, nested Rust/Python/TypeScript saved-file tests, packed Node command, Python ordinary integration and Bifrost story selectors, SDK aggregate dependencies. Existing `UnmigratedDatabase` correctly isolates native CLI tests; existing Python session WyrdTestServer correctly owns ordinary SDK cases. Neither should be repurposed to provision this first-use CLI fixture.

**Focused closure proof:** collect the owning Python shared-lifecycle selectors and verify the four drivers are routed to their dedicated lane while ordinary tests remain selected. Run the ordinary module with the corrected shared-lifecycle selector through the existing wrapper; it must select actual ordinary tests and pass. Run the unchanged exact Python and interrupt driver commands separately through their existing wrappers to demonstrate retained enabled coverage and process assertions. Preserve the Rust and packed TypeScript command entries and aggregate dependency edges; rerun a changed driver only if its body changes. No full SDK, Bifrost or change-wide journey aggregate is required for this routing fix.

## FIND-TASK-004-2 — Server exit is hidden by an unanswered optional host prompt

**Status:** CONFIRMED. **Source ID:** SYS-001. **Classification:** INCORRECT. **Obligation:** REQ-005 explicitly requires reporting server exit; REQ-010 requires a ready usable result.

**Locations:** `crates/wyrd/wyrd-cli/src/server/dev.rs:172–185`, particularly `mcp::offer_local(...).await` at 180; `mcp/mod.rs:223–258`; `mcp/hosts.rs:684–731`.

Read the complete `DevArgs::run`, all DevServer lifecycle methods, `offer_local`, both selection readers and shared rendering/grammar. After fresh HTTP authentication, the retained serving child can exit while `prompt_local_selection` awaits terminal readability. The outer select watches interruption and overall run only. `DevServer::wait` is first reached after the offer completes and the final ready message prints. Therefore an ordinary child shutdown leaves the CLI waiting indefinitely for user input while its deployment is down; a later answer can cause a false ready message before the exit error. The existing PTY scenario establishes that this is an actual first-use phase, not a dormant path.

**Selected minimum correction:** keep the existing DevServer's child-exit observation active during the optional offer, cancel the pending offer when the child exits, and return the existing redacted exit error through normal stop/reap. Retain standalone MCP installation unchanged. Do not add restart, polling infrastructure, another supervisor, SDK guards, or new persistent state. The guard belongs at the child lifetime owner because only it retains this resource.

**Sibling/consumer coverage:** native `ServerCommand::Dev` and Python/Node delegates share this Rust owner; `ready`, `wait`, and `stop` already provide observation and cleanup. MCP standalone selection has no child to supervise. Server subsystem supervision cannot observe the parent prompt.

**Focused closure proof:** use the existing installed-command terminal fixture to reach the detected-host offer, leave it unanswered, terminate the retained serving child, and assert bounded CLI failure without input or final ready output, secret-free recovery, settled processes, and retained saved files. Preserve existing prompt Ctrl-C exit-130 and decline assertions. Extend the existing exact `[interrupt]` driver to prove the additional child-loss path; no new harness is needed. Run its focused wrapper command below. A safe retry must use the same database/config/warehouse rather than another fresh deployment.

## FIND-TASK-004-3 — Local setup establishes HTTP but leaves gRPC deployment settings inconsistent

**Status:** REVISED. **Source IDs:** SEC-002, SEC-003. **Classification:** INCORRECT / VIOLATION. **Obligations:** REQ-007 safe local inputs, REQ-008/AC-001 fresh SDK usability, and INV-003's explicit prohibition of remote plaintext.

**Locations:** `server/dev.rs:105–126,151–161,483–514,515–568`; `crates/shared/wyrd-client/src/global_config.rs:104–142`; actual consumers `config.rs:159–170`, `bifrost/grpc.rs:98–105,262–281`. Ordinary server configuration: `wyrd-server/src/config.rs:2754–2758,3042–3052`; actual public bind/serve: `app/server.rs:299–315,696–703`.

Two concrete paths share one cause: the local deployment connection is treated as HTTP-only. First, `GlobalConfig::save_local` edits only `client.http_url`. `ClientConfig::from_environment` prioritizes an explicit saved `client.grpc_url` over environment and `grpc_endpoint_for` of the new HTTP origin. A previously configured deployment remains the gRPC destination after local setup succeeds. Bifrost's gRPC transport consumes precisely that config and attaches the newly minted local bearer and frame data. Second, `DevServer::command` overrides only HTTP and inherits public `WYRD_GRPC_BIND`, including `0.0.0.0:50051`. The ordinary server accepts and serves that plaintext public listener. This is a real configured-input path forbidden by INV-003, not a claim about the safe default: ordinary gRPC defaults to `127.0.0.1:50051`.

The same source also explains why current randomized-gRPC fixture runs do not close this boundary: all saved-file SDK assertions register/read Cards over HTTP. Without a saved explicit endpoint, the shared helper derives the HTTP host with fixed port 50051, while those fixtures start gRPC on another random port. The fresh HTTP check proves no gRPC usability.

**Selected minimum correction:** select the effective public gRPC bind in the existing CLI local-input owner: preserve a valid nonzero loopback `WYRD_GRPC_BIND`, otherwise use the ordinary loopback default, and reject invalid/non-loopback overrides before server setup. Supply that selected bind to all owned child invocations so a server-config file cannot bypass local selection. Through the existing locked `GlobalConfig::save_local` / `CredentialsFile` transaction, save its actual gRPC destination alongside the HTTP endpoint. Updating `client.grpc_url` is updating related deployment connection state; preserve all unrelated settings, comments, saved logins, keys and cache semantics. Use existing socket/URL and shared transport configuration mechanisms; add no file format, client transport, auth contract, server config loader, or normal-server boot change. This is one boundary/correction, not two independent security projects.

**Producer and sibling coverage:** the new CLI's selected binds produce child configuration and persisted destinations. Its sole production save caller is `save_local_tenant`; other callers are the existing local-save unit test. Fresh Rust/Python/TypeScript use shared ClientConfig and Bifrost transports. SavedLogins save/remove/renew and API-key cache persistence retain the common directory lock and current documents, so extending endpoint edits must preserve those siblings. No language-specific guard or bearer validation change belongs here.

**Focused closure proof:** extend the existing native CLI first-run case with stale explicit saved gRPC configuration, a custom loopback gRPC port, and a fresh saved-file Bifrost gRPC write/read against the started deployment. Assert actual destination alignment and preservation of unrelated config; no credentials/data may go to the stale destination. Extend the existing failure/retry case with non-loopback gRPC input rejection before issuance, without secret output. Cover server-config-file non-loopback input by asserting the CLI's selected local override governs the child. Retain default success. Run the two exact native cases and existing local-save preservation unit test below; do not run all Bifrost or SDK journey suites.

## FIND-TASK-004-4 — Four mandatory rustdoc contracts are missing

**Status:** CONFIRMED. **Source IDs:** MR-001, REPO-004-1. **Classification:** VIOLATION. **Obligation:** AGENTS §16 and agent rules explicitly make fallible/panic documentation mandatory for private helpers; the review skill explicitly retains that blocker.

**Locations:** `mcp/hosts.rs:734` (`show_hosts`) and `:749` (`show_question`), `server/dev.rs:330` (`DevServer::ready`), `wyrd-cli/tests/pg_tests.rs:235` (`LocalDev::environment`).

The two complete output helper bodies return `io::Result` and propagate writes/flush without `# Errors`. Both synchronous install and cancellable local readers call them. `ready` explicitly expects a retained child without `# Panics`; `start` establishes that invariant immediately before its sole call. The fixture helper expects a UTF-8 path without `# Panics`; `client` and `stop` call it. The discovery union is correct and contains one required documentation boundary, not four rounds.

**Selected minimum correction/proof:** document existing write/flush errors and lifecycle/path panic invariants at all four items. Preserve signatures/bodies and existing behavior. Read final rustdoc against bodies and run `mise run fmt`; behavior tests or new guards would add no proof for these comments. Batch this correction with the implementation findings.

## Exact focused commands for remediation

Use the repository toolchain and existing managed setup. These commands name current tests; extend those cases for their diagnosed gaps rather than replacing their assertions.

```bash
WYRD_LOG=info scripts/postgres/with-test-postgres.sh -- mise exec -- cargo nextest run --locked -p wyrd-cli --test cli -E 'test(=pg_tests::server_dev_first_run_is_client_ready) | test(=pg_tests::server_dev_repeat_and_partial_failure_preserve_state)'
mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(=global_config::tests::local_setup_preserves_content_and_existing_key)'
WYRD_LOG=info scripts/postgres/with-test-postgres.sh -- mise exec -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q -m integration 'tests/integration/test_local_development.py::test_installed_cli_bootstrap[interrupt]'
WYRD_LOG=info scripts/postgres/with-test-postgres.sh -- mise exec -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q -m integration 'tests/integration/test_local_development.py::test_installed_cli_bootstrap[python]'
WYRD_LOG=info scripts/postgres/with-test-postgres.sh -- mise exec -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q -m integration tests/integration/test_local_development.py -k 'not test_installed_cli_bootstrap'
```

For the routing correction, the last command must match the corrected ordinary lane's selection, and collection must establish that ordinary cases actually run. Preserve the exact Rust and packed-TypeScript driver entries already in `test:local-dev:journey`; they remain required, not newly duplicated remediation commands. Apply format/lint and narrow Python checks to actual changed files; do not demand broad aggregates or repeat unchanged generated/docs checks. These are closure recommendations, not claims that this validator ran a fixed candidate.

## Reconciled obligations and verdict

| Obligation group | Reconciled status |
| --- | --- |
| Installed command selection, retained compatible release, server-owned migration/setup and ordinary serving | PASS |
| External Postgres/storage defaults and macOS memory | PASS, except effective public gRPC selection in FIND-3 |
| Tenant identity checks, terminal-only root disclosure, redacted failures, protected locking/atomic writes, retry key/cache preservation, partial progress recovery | PASS |
| Saved deployment usability across fresh shared SDK consumers | FAIL: FIND-3; HTTP Cards proof passes |
| Foreground lifetime, cancellation and exit reporting | FAIL: FIND-2; ordinary Ctrl-C/download/prompt cancellation proof passes |
| Detected host selection, harmless decline, shared installer/proxy and authenticated MCP read | PASS apart from child lifetime while offering |
| Cleanup and installed-runtime verification ownership | FAIL: FIND-1; dedicated individually isolated lane passes |
| Mandatory touched-item rustdoc | FAIL: FIND-4 |
| Current/draft documentation, retained draft status and absent Quickstart prerequisite handling | PASS |
| Non-goals and existing-owner reuse | PASS; proposed corrections preserve them |

No prior FIND IDs exist. All seven discovery reports and the focused follow-up are complete. Their proposed FAIL/PASS labels are reconciled by the claim ledger, not counted as votes. No remaining finding needs new architecture, security protocol, fleet availability, persistence or resource-ownership semantics, so **FIX_REQUIRED** is the appropriate verdict.

Available proof: orchestrator reran the focused journey successfully (140.92 seconds), and behavior reviewer reproduced the combined-lifecycle regression with tracing. Production Python, Linux startup, static checks and draft docs evidence are retained as recorded task proof; the validator did not silently claim a rerun. Tracked source was immutable throughout.

Skipped duplicate broad suites, live official-release downloads, forced terminal-write failure and hostile listener exploitation. Draft accessibility does not render draft pages; retained findings have the source and focused proof limits stated above.
