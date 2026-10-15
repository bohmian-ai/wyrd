# Independent structured Ponytail validation

Recommendation: **FIX_REQUIRED**. Retain one revised, bounded finding, **FIND-TASK-001-4**. Prior FIND-TASK-001-1, -2, -3, -5 and -6 are closed in source under approved specification revision 11. No new FIND ID, runtime redesign, authentication source, or specification decision is required.

## Subject and completeness

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/sdk-client-instantiation`.
- Base: `2f1db2194f5630df850b10987fe0f360946a525c`.
- Prior candidate: `fde7d396a3917964dc344ede2ed09046e179aa66`.
- Candidate: `07419b4d060303959384b682ca88e8673c722e63`.
- Authority: approved `changes/active/sdk-client-instantiation/spec.md`, revision 11; original TASK-001 at the base, interpreted under that revision; AGENTS and agent rules; applicable Client model, doctrine, Bifrost public boundary, Rust documentation, spec-driven-development and maintainer rules. Task appendices supply verification claims, not acceptance conclusions.
- Read all seven current discovery reports, `claim-comparison.md`, prior verdict and validated ledger, and the complete task-review skill. Reviewed the cumulative source change rather than only the latest remediation; prior review records in the cumulative range are evidence, not production contracts. CodeGraph preceded code navigation, followed by diff, complete proposed-correction bodies, callers, siblings and relevant test/source inspection.
- All seven discovery reports are present and cover their assigned surfaces. Their agreement did not substitute for validation. No material claim conflict requires another discovery agent. The system report's public-Python documentation closure and other reports' native-Rust documentation finding concern different items and are compatible.
- Static only: no tests, builds, generators, formatters, runtime test children or baseline runs. Only this assigned report was written. HEAD and tracked source were checked against the candidate before reporting and remained unchanged.

## Rule reconciliation

The skill excludes optional wording, placement, style preferences and unrelated old debt from blocking findings, but explicitly says: **“Missing rustdoc on a changed item stays blocking per AGENTS.md.”** AGENTS §16 requires substantive rustdoc on materially modified functions, including tests/helpers, `# Errors` on fallible functions, and `# Panics` where panics remain. Agent rules and Rust Core repeat this hard gate. Therefore inaccurate first-use operation contracts and missing required error/panic contracts at the changed items below remain acceptance failures even without a new production runtime bug. They are one continuation of FIND-4, not separate style findings or a new style-only audit round.

The gate does not authorize documenting untouched functions, fields or modules, rewriting adequate prose, documenting fictitious failures, or treating a pure loader/describe/module rename as a material workflow change. Those expansions are rejected. An async eager factory replacing an infallible handle assembly changes the operation/failure boundary and is included; its specific fixture credential still determines which failures are reachable. In particular a presented bearer is not exchanged or dialed by HTTP-only connection (`auth.rs:850–852`), so missing documentation on a bearer-based test does **not** prove a newly reachable connection panic.

## Every current proposal and site

All retained rows belong to FIND-TASK-001-4. Lines refer to the immutable candidate.

| Discovery proposal / exact site | Disposition | Independent source evidence and minimum correction |
|---|---|---|
| BEH-R2-001; INVAR-R2-001; CONCURRENCY-R2-001; native insertion portion of STANDARDS-R2-001: `sdks/wyrd-sdk-python/src/bifrost/mod.rs:727–750`, `Bifrost::insert` | **CONFIRMED** | The unchanged paragraph at :729 says unconditionally “Non-blocking”; :735–737 names only Card UID/no-table/queue-full errors. The materially changed body calls `handle(py)?` at :747 before enqueue. `handle` (:568–588) calls `LazyConnection::get` (`src/client.rs:126–145`), which blocks on the shared connection with the GIL released. Qualify nonblocking enqueue as after connection, explain synchronous first-use setup, and include its existing connection errors. Public `_client.py:704–723` already supplies the correct model. No executable change. |
| MAINT-R2-001 and STANDARDS-R2-001: `crates/wyrd/wyrd-mcp/tests/bifrost/mcp/verification.rs:171–208`, `CardInvestigation::start`, new expect :178 | **CONFIRMED** | Returns `Result<Self, McpJourneyError>` and propagates surrounding startup/bootstrap/registration/MCP failures, but now panics on eager connection. `api_key` (:78–87) produces an ApiKey, so exchange refusal/unreachability is reachable. Sole caller :415 propagates its Result. Propagate this connection error through that existing Result, following `EvidenceInvestigation::start` (`query/investigation.rs:68`); existing “connection failures” docs can be clarified to include required exchange. Preserve registration and shutdown. |
| Same sources: `verification.rs:987–1047`, `judging_server`, new expect :1021 | **CONFIRMED** | Same ApiKey producer; whole helper returns Result and uses `?` for startup, registration, deployment and bootstrap. Caller :1094 also uses `?`. Newly added connection panic has no `# Panics` and bypasses this channel. Propagate the connection error and include it in existing Errors prose; preserve mock judge, fixture, deployment, request and credential. |
| Same sources: `crates/wyrd/wyrd-testing/tests/bifrost/oracle/workflow.rs:127–327`, `prove_forwarded_query_settles`, new expect :159 | **CONFIRMED** | Public API key is provisioned at :147–149; the helper returns `Result<(), JourneyError>`, propagates other IO, and reports failed claims as errors. The sole caller (:109–111) adds the TerminalCause to that Result's diagnostic. Connection panic bypasses that context and has no panic contract. Use the existing Result channel for eager connection and describe connection/setup errors. Keep every terminal-cause, sibling-availability, ownership, metric and recovery assertion and existing cleanup. |
| STANDARDS-R2-001: `verification.rs:433–552`, `an_agent_discovers_starts_and_observes_a_manual_run`, new expect :440 | **CONFIRMED** | The changed test connects an ApiKey eagerly, returns Result, and contains direct tool/identity/idempotence/permission assertions. Errors prose exists, but required Panics prose does not. Add its actual setup/assertion panic contract. No new runtime test or helper extraction. Keeping this test's setup expect is acceptable when documented; it is not an extra helper failure channel to redesign. |
| STANDARDS-R2-001: `crates/wyrd/wyrd-mcp/tests/bifrost/mcp/gateway.rs:68–290`, `agent_administers_redacted_gateway_configuration`, new expect :83 | **REVISED** | The whole changed test has scenario prose but neither Errors nor Panics sections despite Result-returning IO and assertions. Add those actual contracts. **Reject** any implication of newly reachable exchange/dial failure at :83: `http_client` (:32–46) creates `BearerToken`; `connected` → `bearer` returns it without network. Document returned bootstrap/configuration/HTTP/MCP/shutdown failures and remaining assertions, not fictitious eager authentication IO. |
| STANDARDS-R2-001: `crates/shared/wyrd-client/tests/integration/cards_transport.rs:187–235`, `list_uses_typed_query_parameters_and_an_empty_get_body` | **REVISED** | Has meaningful scenario prose but no Panics section despite setup expectations and shape/body assertions in the materially changed factory workflow. Add the actual setup/list/query-shape/assertion panic conditions. `client` (:160–177) uses a fixed BearerToken; reject a claimed new exchange/transport failure from its connection expect. No behavioral rewrite. |
| Same source: `cards_transport.rs:239–263`, `uid_delete_uses_kind_qualified_path_and_preserves_idempotence` | **REVISED** | Same credential counterevidence and changed factory boundary. Missing required Panics contract is real; describe setup/delete and method/path/body assertions. Do not add exchange failures or alter the idempotence test. |
| Same source: `cards_transport.rs:267–297`, `server_problem_code_and_status_survive_registry_boundary` | **REVISED** | Same fixed bearer. Missing Panics contract is real; describe setup, expected refusal and code/status/details/request assertions. No new failure injection or auth claim. |
| STANDARDS-R2-001: `crates/shared/wyrd-client/tests/integration/pg_auth_e2e_against_fixture.rs:47–123`, `wyrd_client_authenticates_via_wyrd_access_token_header` | **CONFIRMED**, narrowed | The changed test has no item rustdoc at all, notwithstanding comprehensive module documentation. Add concise scenario/workflow and Panics rustdoc covering setup/exchange/request failures and header/permission assertions. It explicitly fills the API-key cache at :77 before eager connection :86; do not imply that connection necessarily exchanges again. Preserve the Authorization-only negative control. |
| STANDARDS-R2-001: `crates/wyrd/wyrd-server/tests/integration/pg_card_registration_route.rs:370–509`, `client_registration_saga_returns_active_receipt` | **REVISED** | Changed connection sequence :408–414, existing one-line scenario, many setup/registration/artifact/SQL expectations and receipt assertions, no Panics section. Add actual setup/IO/assertion panic contract. Its `registry_client` (:98–112) uses BearerToken; reject a new exchange-failure assertion at connect. Preserve the full Active/artifact verification workflow. |
| STANDARDS-R2-001: `crates/shared/wyrd-client/tests/integration/startup_image_journey.rs:147–225`, `startup_image_write` | **CONFIRMED** | Changed eager Card connection and Bifrost option composition are material setup operations. Existing scenario prose omits Panics despite configured client/registration/gRPC/write/drain/state-file expectations and assertions. Add those actual panic conditions; no image lifecycle change or new test. |
| Same source: `startup_image_journey.rs:232–262`, `startup_image_verify` | **CONFIRMED** | Changed eager Card connection and Bifrost construction; scenario prose lacks Panics for saved-state parsing, client/artifact/read/drain failures and persisted-content assertions. Add that contract, preserving restart/read proof. |
| STANDARDS-R2-001: `crates/wyrd/wyrd-testing/src/bifrost/write.rs:58–77`, `BifrostWriter::connect` | **CONFIRMED** | Existing Errors text (:54–57) ends at credential resolution/client construction/gRPC dial. New `Bifrost::connect_with` calls `client.connected()` before native assembly (`facade.rs:147–154`), so required exchange can now refuse/fail at this helper. Add that condition to Errors. Keep client ownership, card attribution, queue tuning and writer lifecycle unchanged. |

No current system/security finding requires disposition beyond their explicitly empty ledgers; their no-new-regression conclusions are supported within the inspected source boundary. No unique proposal has been dropped because it lacked corroboration.

### Prior FIND-4 sites already corrected

| Prior proposed site | Disposition now |
|---|---|
| `wyrd-client/src/state.rs:541,552` removed constructor links | **REJECTED as residual**: both now reference `Bifrost::connect_with` and options; body matches. |
| `wyrd-client/src/client.rs:563,585` new helper/eager cache test | **REJECTED as residual**: meaningful operation and Panics docs now present; test still counts the exchange and preserves bearer/refusal assertions. |
| Rust SDK `support/mod.rs:115`, `Deployment::cards` | **REJECTED as residual**: fixture role, eager exchange and Panics now documented. |
| `examples/rust/client_request.rs:24`, main | **REJECTED as residual**: workflow and Errors now describe config/credential/exchange/request failures. |
| CLI `card.rs:679`, `build_cards`; `workflow.rs:388`, `ambient_cards`; MCP `connectivity.rs:166`, principals | **REJECTED as residual**: existing Errors now cover eager exchange; bodies propagate it. |
| MCP `query/investigation.rs:58`, start; Rust SDK `principal_roles.rs:73`, author | **REJECTED as residual**: connection now uses `?`, with error docs, preserving subsequent behavior. |
| Native Python Bifrost local getters/table mutation and four Cards views | **REJECTED as error-doc demands**: corrected operations are infallible/local; adding connection Errors would reintroduce an inaccurate contract. Current local-state docs match source. |
| Public Python base/insert/AsyncBifrost timing | **REJECTED as residual**: `_client.py:641–668,704–723,1040–1050` qualifies initial connection and synchronous insertion; native insert is the remaining distinct site above. |

## Final deduplicated ledger

### FIND-TASK-001-4 — Complete the changed connection operation/error documentation

- **Status:** REVISED; prior stable ID preserved.
- **Sources:** BEH-R2-001, INVAR-R2-001, STANDARDS-R2-001, MAINT-R2-001, CONCURRENCY-R2-001.
- **Classification/severity:** VIOLATION; Important, mandatory changed-item rustdoc gate. No new production runtime defect is asserted.
- **Violated obligation:** AGENTS §16 / agent-rules required operation, Errors and Panics rustdoc; task-owned SDK API documentation in REQ-012 and AC-006. Passing Clippy/broken-link checks cannot establish semantic documentation accuracy or waive missing sections.
- **Evidence and consequence:** Native first insert's producer is an unopened LazyConnection; its consumer initializes before bounded admission, contradicting its method contract. The three ApiKey fixture helpers turn the new eager Result into an undocumented panic, bypassing existing Result propagation (including the Oracle caller's terminal-cause context). The listed changed tests lack required panic/error contracts; the writer helper omits its newly possible exchange refusal. Exact evidence and counterevidence are in the complete site table above.
- **Decision-complete correction boundary:** One batch across exactly the retained sites above. Reuse the native connection accessor's documented error contract and already-correct public Python first-use wording. Propagate the new connection Result in the three fallible helpers, using their existing error aliases and the nearby corrected `EvidenceInvestigation::start`/`author` pattern. Complete the actual changed test contracts and the existing writer Errors section. Do not create an error type, connection helper, checker, fixture, new test or abstraction. Do not infer auth IO for bearer/fresh-cache test cases.
- **Preserved behavior:** Initial synchronous insert is permitted to connect; once connected it retains existing bounded queue/refusal semantics. No preconnection buffering, awaitable insert, admission/lifecycle change, lock redesign, cancellation guarantee, auth probe, saved Platform source, or runtime owner migration. Preserve existing cleanup, shutdown, all assertions, shared resolver/client/cache, and the already closed prior fixes.
- **Closure criteria:** Every retained final item has accurate substantive operation/failure/panic docs. The three fallible helpers return the eager connection error through their existing Result instead of unwrapping it; subsequent IO, assertions and cleanup remain. Source review explicitly checks these pairs, rather than using a green lint as proof. No documentation expansion to untouched or pure name-only migrations.

## Prior closure independently reassessed

| Stable ID | Source-backed assessment |
|---|---|
| FIND-TASK-001-1 | **CLOSED**. Native Bifrost `Binding` and methods `mod.rs:548–604,697–724,934–945` keep local binding/read/count access noninitializing. Connection is made without a stale table snapshot, then transfers the latest Local value under the same binding lock before a server caller proceeds. `SharedCards` (`state/mod.rs:1663–1794`) keeps parent/four views on one Arc-backed LazyConnection; selection does not initialize. Unit local-access and real-server local-state survival assertions inspect the correct boundary. No established lost-update/GIL deadlock: initialization releases GIL and binding lock is not held over dial; all first server callers perform handoff. |
| FIND-TASK-001-2 | **CLOSED under approved11**. `WyrdClient::from_global` (:75) directly delegates to `from_environment(Process)` (:94), the actual composition of existing GlobalConfig/ClientConfig/auth owners; duplicate `ClientConfig::from_global` and private journey resolver are removed. Rust ambient, missing-credential and retained-saved-config journeys call this one injected resolver, then actual shared handle factories. Python ambient journey (:95–113) awaits eager AsyncBifrost and real listing in both environment/saved modes. No process-global mutation, runner provisioning or fixed-port proof is required. |
| FIND-TASK-001-3 | **CLOSED**. Rust delegated journey (:276–367) retains permissions, makes an allowed Bifrost query, reads subject-filtered retained audit and requires the known actor chain. Python (:180–211) checks two eager/lazy delegated operations and two known-actor rows. TS (:123–149) checks its subject-filtered retained row and actor. Nonempty exact comparisons replace client-only identity inference; all preserve subject denial/actor permission. Existing retained audit/flush owners are reused. |
| FIND-TASK-001-4 | **PARTIAL**. Corrected earlier sites above remain closed; the single revised ledger item contains all confirmed residual sites. |
| FIND-TASK-001-5 | **CLOSED**. `saved_user_auth.rs:207–224` still actually revokes the saved refresh chain and expires access, then asserts `WYRD_CLIENT_401_SAVED_LOGIN_UNUSABLE` on awaited connection itself. The obsolete successful connection/unreachable list sequence is gone. Helper :96–106 documents eager renewal and refusal. Sibling success/renewal/permission cases remain. |
| FIND-TASK-001-6 | **CLOSED by approved10/11**. AC-002 explicitly limits saved-only coverage to existing tenant credential sources and approves injected Rust proof. Platform `handle.rs:70–118` preserves bearer-only platform-session exchange, bypasses tenant `connected`, and gains no saved source. Rust ambient case opens its environment-selected platform session; common injected empty-chain proof rejects before a handle opens. Do not reopen those approved choices. |

## Compact acceptance reconciliation

This maps all original task IDs under approved11. PASS is a static implementation/inspected-proof conclusion, with execution limits below; no claim of a newly executed successful test follows.

| Obligations | Result and evidence |
|---|---|
| REQ-001–005 | **PASS**. Shared Cards/Gateway/Principals/operators/Platform factories; async/blocking Bifrost/options; Python eager sync/async; TS awaited native factories. Each uses the shared client/auth/transport owner. |
| REQ-006–009 | **PASS**. LazyConnection argument-only construction, successful once-publication, Local/Native binding and shared Cards views; Python local/first-server-operation and eager Async proof. First insertion setup remains permitted. |
| REQ-010–011, REQ-016 | **PASS**. Removed public forms/old principal module; crate-private Cards construction retained only for local/offline owners; SDK negative/public-surface checks; TS private constructors. Pure local-loader renames preserve deferred behavior. |
| REQ-012 | **FAIL, bounded FIND-4**. Executable examples and API/stub migrations otherwise match; native insertion operation contract remains inaccurate. TASK-002 docsite/README prose is excluded. |
| REQ-013–015 | **PASS**. Current Client model/language references encode the vocabulary; shared `connected` uses existing access_token/cache and retained clones; Bifrost validates options then connects existing transport; no extra bearer probe. |
| REQ-017–019 | **PASS**. One typed GET on retained QueryClient; async/blocking/Python/TS projection of existing compact entry; separate describe, no fan-out/catalog/cache. |
| INV-001–005 | **PASS**. Server/wire/permissions/identity/secret-owner boundaries remain; local regressions corrected; language wrappers own only earned runtime lifetime/state. No new dependency/feature/server route/storage owner. |
| AC-001 | **PASS static**. Public shape, removal and export proofs inspected; no new runtime execution of compile-fail docs. |
| AC-002/004/007 | **PASS under approved11**. Actual injected single-resolver Rust cases; Python environment/saved eager/lazy and Async; TS ambient success; preserved eager exchange versus bearer errors and selected-client retention/renewal. Shared mock explicitly counts exchanges, while language journeys prove endpoint retention and renewal. No combinatorial expansion required. |
| AC-003 | **PASS source proof**, prior FIND-3 closed above. |
| AC-005 | **PASS source proof**. Python no-credential/unreachable endpoint local construction/access tests and source path are IO-free. |
| AC-006 | **FAIL TASK-001 portion, FIND-4**. Stubs/declarations statically match, generated checks are recorded claims; mandatory Rust contracts remain. TASK-002 owns docsite/README acceptance. |
| AC-008–009 | **PASS source proof**. All language discovery journeys register own/foreign tables, assert built-ins/tenant exclusion/permission refusal, then describe declared fields separately; source/declarations preserve compact typed shapes. |

## Focused closure proof and verification limits

For documentation-only sites, review final docs against their complete bodies and existing connection paths. No runtime test is needed solely for prose. For the three helper `?` corrections, use these existing exact journeys, with the setup from `mise.toml:512–518,640–646`; no new test or broad suite is required. Commands are recommendations and were **not executed** during this static review:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc \
  "mise run db:migrate:inner && mise exec -- cargo nextest run --locked \
  -p wyrd-mcp --test mcp -P journey --run-ignored=all \
  -E 'test(=verification::pg_tests::agent_discovers_service_and_component_cards)'"

scripts/postgres/with-test-postgres.sh -- bash -lc \
  "mise run db:migrate:inner && mise exec -- cargo nextest run --locked \
  -p wyrd-mcp --test mcp -P journey --run-ignored=all \
  -E 'test(=verification::pg_tests::verification_tool_continues_trace_context)'"

scripts/postgres/with-test-postgres.sh -- bash -lc \
  "mise run db:migrate:inner && mise exec -- cargo nextest run --locked \
  -p wyrd-testing --test oracle -P journey --run-ignored=all \
  -E 'test(=workflow::workflow_forwarded_query_settles_before_the_run_ends)'"
```

Target/module identities are established by the Cargo test declarations and `tests/bifrost/{mcp,oracle}/main.rs`. Use `mise run fmt`, `mise run lints` and `git diff --check` for the resulting Rust-only write set. No Python/type/codegen/declaration rebuild is necessary if remediation stays within these Rust docs and three test-helper error paths. Full user-journey aggregates belong to integrated change review, not this bounded remediation.

The task evidence records focused/grouped Rust/Python/TS instantiation, local-state, delegated-audit, shared-client, CLI and managed identity checks plus format/lint/type/broken-link checks. Actual relevant assertions were inspected; raw transcripts were not supplied and results were not independently reproduced. Abbreviated/grouped command records do not establish exact focused execution for every named check. Current source establishes no additional security, resource-loss, concurrency, recovery or persistent-state defect; deterministic simultaneous-first-use/cancellation/fault execution was not supplied, and its absence does not justify a new harness or finding.

Skipped execution, unrelated hardening and TASK-002 prose acceptance as assigned. Remaining risk is the mandatory FIND-4 operation/error documentation gap and dependence on recorded verification results; no production runtime redesign is recommended.
