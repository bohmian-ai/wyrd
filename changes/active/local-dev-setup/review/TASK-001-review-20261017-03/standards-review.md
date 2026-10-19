# Repository standards review

**PASS.** No material repository-rule finding is proposed.

## Subject and method

Reviewed the cumulative range `bc3bf25e3f78b0c429110350a3de4d58290e632f...6542f50252d12ba1fb80dda24e7ed2dd8b90111a` in this worktree. HEAD still matched the candidate at completion. The latest remediation range located changes; it did not replace the cumulative diff. Read the approved spec/original task and both previous validated ledgers, verdict/remediation context and recorded verification. Current-round peer reports were not read. No CodeGraph index exists.

This is repository-rule review, not task acceptance or a separate complexity/design audit. Source inspection includes production signatures, surrounding consumers and proof bodies; reported green commands are supporting evidence, not independent executions. Only this report was written.

## Authority coverage

All rows also apply `AGENTS.md`, `architecture/agent-rules.md`, the reference router, and `languages/spec-driven-development.md`, `languages/implementation-execution.md`, and `languages/maintainer-style.md` under `architecture/references/`.

| Changed surface / coverage | Applicable complete authority and routed references |
|---|---|
| `wyrd-spec/src/gateway/{deployment,mod}.rs`: apply/request/response validation, re-exports | `wyrd-design.md`, `wyrd-doctrine.mdx`; references `doctrine/architecture-constraints.md`, `architecture/patterns.md`, `languages/rust-core.md`, `languages/errors.md`, `languages/agent-harness.md` |
| `wyrd-client/src/{global_config,config,gateway,gateway_credential}.rs`: protected DSNs, saved settings, restricted mutation transport and consumers | Same design/doctrine/architecture/Rust/error authorities; full `wyrd-security-posture.md`, `operations/deployment-and-release.md`; `languages/testing-workflows.md` |
| CLI `server/dev.rs`, `terminal.rs`, `gateway.rs`, `mcp/{mod,hosts}.rs`, platform credential visibility and lib registration | Same complete design/doctrine/security/operations, architecture/Rust/error/testing references; `AGENTS` §§3–6, 9, 11, 16 |
| Gateway server service/routes, HTTP registration, server MCP catalog/dispatch | Same design/doctrine/security, architecture/Rust/errors/agent-harness/testing references; SQL capability, authenticated tenancy, nonblocking audit and generated-contract rules |
| CLI PG/installed-binary/composite journeys; server PG/OpenAPI and MCP discovery/connectivity/gateway tests, including new helpers/constants and catalog rustdoc | `AGENTS` §§11/16, agent-rules; complete Rust/testing/agent-harness/error and execution references, security/operations fixture boundaries |
| Python installed local-development tests, PTY/input/restart helpers and response-dropping proxy; fresh Python/TS/Rust consumers | `AGENTS` §§7–8/11/16; complete `languages/python-api-and-stubs.md`, `languages/typescript-guide.md`, testing/maintainer/execution references; security/process boundaries |
| `Cargo.toml`/CLI manifest/lock, `mise.toml`, `scripts/check_ts_package.mjs` | `AGENTS` dependency/feature/gate rules; Rust/testing/TypeScript references; existing package inventory and task conventions |
| Setup, quickstart, gateway, troubleshooting, MCP reference; in-tree gateway README/document; security/operations additions and active packet | Design/doctrine/security/operations, typed surface alignment and secret rules; agent-harness/spec/execution/testing references |

No PyO3/N-API implementation, authored stub or generated declaration changed; their parity obligations were checked for applicability rather than inventing binding changes. No UI or analytical-engine behavior changed. Bifrost paths are existing server settings passed unchanged to the installed owner, so the analytical domain references are not routed as a separate implementation concern. Sibling repositories are excluded by the fixed review scope; the in-tree optional sibling-document test remains reviewed.

## Rule results and source evidence

| Applicable rule | Result and source evidence |
|---|---|
| Foundational contracts remain IO/async/PyO3-free; typed source-owned schemas and stable errors | PASS — `ProviderDeploymentsApply::validate` is synchronous pure validation; typed DTOs derive serde/schema/OpenAPI support and retain `deny_unknown_fields`. Existing `GatewayContractError`/`WyrdError` projections are reused; no parallel error catalog or hand-edited generated file enters the diff. |
| Durable behavior/server ownership and restricted client mutation | PASS — CLI `AuthoringApply::resolve` only projects local references into existing write-only secret types. `CredentialWriter::apply_deployments` reuses shared transport; public SDK `Gateway` gains no managed-secret mutation. `GatewayAdministration::apply_deployments` owns durable work. |
| Actual SQL capability types and caller-owned transactions | PASS — service helpers at `service.rs:688,726` accept `&mut TenantConn<'_>` and never commit/rollback; the operation owns its connection and commits once at its final fallible step. Existing SQL credential/deployment/share-lock/revoke/delete functions also accept `TenantConn`; no raw production pool, caller transaction or manual tenant read predicate was introduced. Server imports its owning SQL surface, with no Vala cross-tier reach-through. |
| Authenticated tenancy, scope/domain authorization and nonblocking audit | PASS — new HTTP/MCP paths call the same administration owner. `audited` authorizes through existing `audit::authorize` before acquiring tenant SQL; shared outbox staging is unchanged. MCP catalog uses `gateway_write`, typed apply schema and effect metadata; dispatch calls the same server method. No tenant/principal authority is accepted from the document. |
| Secrets/protected files/effective sealing identity | PASS — global dev DSNs use `CredentialsFile` lock/document/replace and secret return types; ordinary settings remain separate. Existing key conflict protection remains. `local_sealing_key`/`supplied_sealing_key` reuse `read_secret_file`, preserve supplied effective bytes owner-only with no-clobber persistence and supply that file to every child; wire secret types redact. Local-env values are held in memory and not expanded into authored files. |
| Cohesive owners, narrow async boundary and cancellation | PASS — retained lifecycle belongs to `DevServer`, configuration to `GlobalConfig`/`CredentialsFile`, terminal attributes to `EchoGuard`. Validation/config construction remain synchronous; new async work awaits IO or composes IO. Biased interruption precedes concealed input and the owned server is settled afterward. Terminal facilities support both required Unix OS families; no new target-OS feature or procfs dependency remains. |
| Rustdoc on materially changed items, including tests/helpers/fields/constants | PASS — apply DTOs/methods/private write helpers and CLI/config/terminal additions have intent/invariants/error docs. Prior journey helpers/constants and `resource_names`/`apply_document` have substantive panic/role docs. Latest `tenant_slugs` (`pg_tests.rs:314–338`) explains operator visibility and panics. The catalog test (`server/src/mcp/gateway.rs:632–645`) now has concrete panic conditions for names/order/annotations/effects/schema claims. Mechanical default-field propagation does not introduce a new behavioral test contract. |
| Test tier/runtime/fixture integrity | PASS — public paths are exercised by installed CLI, fresh language callers, real Postgres and real HTTP/MCP journeys; PG-dependent Rust work stays in PG/journey lanes. No Python/Node runtime is initialized in Rust tests. Python tests remain top-level functions and public client consumers. `tenant_slugs` uses a transaction-local operator session before cross-tenant fixture inspection; its raw pool is private external test-binary setup, not a production library capability. |
| Latest identity proof is genuine and does not weaken checks | PASS — `saved_key_config` at `dev.rs:1304` sets the exact credential with an empty fixed environment/default config: no selector, saved login or disk cache can replace it (`ClientConfig::from_environment` defaults cache to memory). Normal `saved_client` remains separate. PG proof builds B's actual token with the shared signing fixture, saves it via `SavedLogin::from_token`, and attempts A's invalid-on-B retained key; unchanged-file/listener assertions remain. Operator reads test absence of the specific requested slug instead of vacuous RLS-hidden counts. |
| Failure diagnosis and proof changes preserve assertions | PASS — R1 records the premature ready-summary read diagnosis and independent diagnostician confirmation. `converse` now reads through the existing final stop-guidance line, preserving the gateway-summary assertion and deadline. The new listener-only restart uses this helper and verifies updated saved endpoints, no offers, retained key and readable resources. No skip, ignore, retry or assertion relaxation was added to clear that failure. |
| Real SQL failure and uncertainty coverage | PASS — P4 performs earlier managed rotation then a later existing FK-rejected provider change, comparing full retained rows/envelopes/deployments and decrypting the old secret. `LosingProxy` reads the actual server apply response before dropping it; CLI/docs/503 guidance admits uncertain commit and proof reads committed state/reapplies. Existing audit accounting remains. |
| Dependency/features/manifests/tooling | PASS — sole manifest edge adds existing workspace `wyrd-utils` to the CLI for its existing secret-file reader; no new dependency family/feature or client SQL edge. Lock matches. Mise adds exact owning-runtime/managed-PG tests without an all-feature test union. TS inventory adds the package's real README while preserving the rest of its allowlist; it does not broaden a boundary glob or disable checks. |
| Docs/examples/surface parity | PASS — in-tree JSON remains secret-free and uses typed local-env authoring; two-step and array semantics remain documented. Setup/recovery guidance names existing recovery authority and config-home separation, plus listener-only resume. MCP reference matches the registered tool/permission. Served OpenAPI proof checks typed request/response and refusal schemas; runtime MCP proof is independent of codegen. |
| Verification workflow and recorded evidence | PASS within source-audit limits — original task/R1/R2 carry exact named-test commands and managed-Postgres wrappers, focused runtime proofs and formatting/lint/boundary/codegen/docs/Python checks. R2 records Linux-only execution and the valid-login/operator-read corrections. Broad integrated proof remains change-review work; no unexecuted command is represented here as independently passed. |

## Prior closure re-established from current source

| Prior ID | Current source closure |
|---|---|
| FIND-TASK-001-1 | Resolved `inputs.interactive` reaches `mcp::offer_local`; explicit hosts remain supported; PTY unattended proof retains host-file assertion. |
| FIND-TASK-001-2 | No-dev retained key refuses in resolution; exact-key preflight runs before `save_settings`/provision. Empty fixed config blocks saved-login/ambient/cache substitution; ordinary post-setup precedence is retained. |
| FIND-TASK-001-3 | Effective generated/file/inline sealing bytes retained; every child gets retained file; actual provider-call restart and conflicting-key refusal remain. |
| FIND-TASK-001-4 | Generic uncertainty wording matches commit-before-answer; response-drop proof checks settlement, durable readback and reapply. |
| FIND-TASK-001-5 | Signal future is polled before input; dropping concealment restores echo; INT/TERM PTY proof remains. |
| FIND-TASK-001-6 | Real FK-aborted composite rotation and old-secret decryption proof remain in P4. |
| FIND-TASK-001-7 | All previously cited helper/constants and the cumulative catalog test now contain required docs; new latest helper is documented too. |
| FIND-TASK-001-8 | `free_bind` uses `TcpListener::bind("127.0.0.1:0")`/`local_addr`, with obsolete procfs/range logic removed. |
| FIND-TASK-001-9 | Shared `configured` producer uses retained key plus saved endpoint, not endpoint equality; both offer consumers use it; real PTY changed-listener proof is present. |

## Findings and limits

Local material proposals: **none**. Overall repository-standards result: **PASS**.

No Cargo/build/test/lint/codegen commands were run by this reviewer. Recorded Linux verification was inspected; macOS/arm64 execution and broad integrated suites were not independently checked. No source edit, external repository operation, commit, merge, push or deployment occurred.
