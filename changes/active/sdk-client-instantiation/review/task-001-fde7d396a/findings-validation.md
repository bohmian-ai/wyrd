# Independent findings validation

**Result: SPEC_REVISION_REQUIRED.** There is one unresolved approved-authority conflict (FIND-TASK-001-6), separate from five bounded implementation/proof findings (FIND-TASK-001-1 through FIND-TASK-001-5). No production authentication, queue, transport, or lifecycle redesign is authorized by this validation.

Subject: base `2f1db2194f5630df850b10987fe0f360946a525c`, candidate `fde7d396a3917964dc344ede2ed09046e179aa66`; approved specification revision 9 and original TASK-001 at the base. No prior validated FIND IDs exist for this subject. HEAD was checked at the candidate. Only this report was written; no tests, builds, generators, formatters, source edits, baseline execution, or runtime test subprocesses were performed.

## Validation inputs and method

Read the complete validation contract, immutable subject, approved specification, original task requirements/scenarios, all seven discovery reports, focused follow-up, and claim comparison. The comparison supplied grouping only. Independently checked source and cumulative changes, applicable AGENTS/agent rules, spec-driven development and maintainer standards, Client model, Bifrost public/admission boundary, and language boundary/documentation rules. CodeGraph was used before code lookup and expanded with source/caller searches. Green task-appendix labels are recorded claims, not independently reproduced evidence.

Correction-boundary source inspection included the complete lazy initializer, Python Bifrost construction/local methods, Cards parent and all four view owners and their operations, shared connection/authentication owners, all three instantiation journeys, Platform session and credential selection, saved-login producer/renewal/consumer, and each specifically proposed rustdoc correction body. Existing sibling owners include WyrdState/Workflow local loaders, shared Bifrost table/writer pool, Gateway/Principals/OperatorConnections lazy consumers, HTTP/gRPC authentication, CLI/MCP eager consumers, other saved-login scenarios, and existing delegated read/write/audit coverage. No duplicated resolver, authenticator, buffer, cache, or generic lifecycle abstraction is needed.

## Every proposed claim: disposition

| Proposal | Disposition | Source-backed resolution |
|---|---|---|
| BEH-001 | CONFIRMED | Local Bifrost metadata/binding and Cards view selection call the initializing accessor before a server operation. Consolidated in FIND-TASK-001-1. |
| INVAR-001 | CONFIRMED | Same producer and affected siblings; no separate failure per getter. FIND-TASK-001-1. |
| STANDARDS-001 | CONFIRMED | Python property-IO rule independently applies; GIL release does not move an asyncio caller to another thread. FIND-TASK-001-1. |
| MAINT-001 | CONFIRMED | Bifrost local behavior and public documentation disagree for reachable public facades. FIND-TASK-001-1. |
| SYS-001 | CONFIRMED | A local accessor can stall unrelated coroutines in the SDK process. No server-process outage or fail-closed change follows. FIND-TASK-001-1. |
| CONCURRENCY-001, local binding/properties | CONFIRMED | Same local-access root cause. FIND-TASK-001-1. |
| CONCURRENCY-001, requiring lazy synchronous `insert` to avoid all connection IO | REJECTED | REQ-007 explicitly requires connection at the first operation needing server resources. Unlike binding/count/view selection, initial insertion needs the native writer/transport owner. REQ-009 requires nonblocking construction; REQ-008 explicitly permits connection timing/error differences. The shared bounded enqueue still does not wait on a network send after setup. Requiring preconnection acceptance would introduce a queue/resource/lifecycle decision the change does not approve. Do not turn insert into an awaitable, add preconnection buffering, require eager connect, or change flush/shutdown ownership. Document the first-use setup qualification (FIND-TASK-001-4). |
| BEH-002 | REVISED | Rust proof bypass is confirmed; test-only bounded fixture/runner support is feasible. Separate literal saved-only Platform obligation from tenant proof instead of prescribing a new credential source. FIND-TASK-001-2 and FIND-TASK-001-6. |
| INVAR-002 | REVISED | Rust bypass and Python eager AsyncBifrost omission confirmed. A per-language retention proof does not require every handle/eager/lazy combination: Python's ambient lazy handles and TS ambient handles already exercise retained configuration. Require the missing public Rust ambient path and Python eager async success, not a combinatorial rewrite. FIND-TASK-001-2. |
| Follow-up: Rust ambient proof feasibility | CONFIRMED | Existing process-start settings, protected saved configuration, configured server binding, tenant seeding and credential hash/storage owners suffice with bounded test support. No unsafe runtime environment mutation, re-executed test process, public injection constructor, dependency or alternate auth implementation is necessary. FIND-TASK-001-2. |
| Follow-up: saved-only Platform applicability | CONFIRMED as authority conflict | No current saved source yields the bearer variant Platform accepts. D4/INV-001 preserve that contract while AC-002 literally demands success in this case. FIND-TASK-001-6; do not silently qualify or implement it. |
| Follow-up: Python eager AsyncBifrost proof omission | CONFIRMED | `HANDLES` excludes it; lazy success and eager no-credential unit coverage do not prove eager real-server environment/saved success. FIND-TASK-001-2. |
| BEH-003 | CONFIRMED as proof gap | Authority intersection is proven; exact server-observed subject attribution is not asserted in any language's instantiation journey. Existing engine audit coverage proves a different boundary and does not replace the required per-language contract. FIND-TASK-001-3. |
| BEH-004 | CONFIRMED | Both WyrdState links name the removed Bifrost constructor. Consolidated with mandatory documentation closure in FIND-TASK-001-4. |
| MAINT-002 | REVISED | Mandatory panic/failure documentation omissions confirmed. Consumer `# Errors` demands for local accessors are withdrawn when FIND-TASK-001-1 restores infallibility. Do not perpetuate unintended connection failures in getter documentation. FIND-TASK-001-4. |
| STANDARDS-002 | REVISED | Every specifically listed changed item was source-checked; omissions/stale failure descriptions are real. Batch with the other fixes. Already-fallible helpers should propagate the newly introduced connection error through their existing Result rather than add an unnecessary panic. FIND-TASK-001-4. |
| STANDARDS-003 | CONFIRMED | Revocation/expiry causes eager `SavedLogins::cards` to return refusal, then the consumer's success expectation panics before its old list assertion. FIND-TASK-001-5. |
| Security report: no changed security regression | CONFIRMED within reviewed scope | Explicit clients retain auth identity; Platform session plane and server authorization remain unchanged; listing is one existing authorized HTTP operation. This does not waive AC-003 proof or the saved-only authority conflict. |
| Concurrency/system counterclaims: once-only publication, failed-attempt retry, retained resources, cancellation | CONFIRMED within source limits | `LazyConnection::get` double-checks under the GIL-detached gate and stores only success. Existing async IO calls use to_thread. No additional race, cancellation, recovery, or resource-loss finding is established. Missing stress/cancellation tests alone do not justify another finding. |

## Final deduplicated ledger — bounded findings

### FIND-TASK-001-1 — Local Python access prematurely initializes the connection

- **Sources/status:** BEH-001, INVAR-001, STANDARDS-001, MAINT-001, SYS-001, local portion of CONCURRENCY-001; **CONFIRMED**.
- **Classification / severity:** INCORRECT / REGRESSION / VIOLATION; Important.
- **Obligation:** REQ-007 (first operation needing the server), INV-001, Python API/stubs prohibition on hidden property IO. REQ-008's initialized behavior remains sound; REQ-009 constructor timing itself passes.
- **Exact evidence:** `sdks/wyrd-sdk-python/src/bifrost/mod.rs:656–684,892–900` calls `handle(py)` from `use_table`, `table`, `dropped`, and `producer_count`. `handle` at :554–566 builds options from retained arguments and calls shared eager connect. `sdks/wyrd-sdk-python/src/state/mod.rs:1726–1781` calls `cards(py)` in all four typed-view getters, then stores a connected Cards clone. Their producers are the argument-only constructors (:587–598 and :1693–1697); the shared sink is `src/client.rs:120–138`, synchronously driving connection in `py.detach`.
- **Reachable consequence:** `Bifrost(table=cfg).table`, `.use_table(other)`, unopened counters, and `Cards().prompt` resolve credentials/network and can fail before any server operation. `AsyncBifrost` inherits synchronous forwarding at `python/wyrd/bifrost/_client.py:677–737,1027`, so local inspection/binding can stall its event-loop thread. Unit `FIRST_USE` at `tests/unit/test_sdk_client_instantiation.py:23–29` currently enshrines the wrong trigger. Initialization also selects ambient authority too early.
- **Minimum safe correction:** Keep unopened local Bifrost binding and zero counters local, and have derived Card views share their parent's deferred connection until an actual registry operation. Reuse the current lazy owner and shared Rust connect, retaining one selected client and successful native handle. Preserve preconnection table changes for eventual initialization and all postconnection native table/writer-pool semantics, including concurrent access consistency. No second resolver/transport or per-view independent connection is needed. Do not extend this to preconnection row buffering or queue redesign. Remove accidental fallible getter shapes/docs when local work is infallible.
- **Ladder:** Cannot delete local capabilities (INV-001). Existing constructor options and shared lazy/native binding owners supply the needed state; ordinary shared ownership is sufficient. No new dependency/framework is earned.
- **Focused closure:** Existing Python unit module must prove each local operation/view is usable with absent credentials and unreachable endpoints, while an actual server operation still returns the eager factory's stable error. Include local AsyncBifrost accesses on its event-loop thread, and a real-server first-use case showing the preselected table and shared parent/view client survive initialization. No artificial host load or delayed connection machinery is required to prove that no connection is attempted by local access. Commands P1/P2 below select existing owners; any specifically named added test must have its exact focused command recorded.

### FIND-TASK-001-2 — Required public ambient entry points lack real journey proof

- **Sources/status:** BEH-002, INVAR-002, follow-up tenant feasibility/Python async claims; **REVISED**.
- **Classification / severity:** MISSING verification; Important. This is not a proven production resolver failure.
- **Obligation:** AC-002, no-client portion of AC-004, public ambient portion of AC-007; original Scenarios 4/6.
- **Exact evidence:** Rust journey `sdks/wyrd-sdk-rust/tests/integration/sdk_client_instantiation.rs:50–58` manually resolves Fixed Environment, :106–128 only calls `connect_with`, :146–193 opens the environment/saved/Platform stories through explicitly built clients, :201–222 refuses inside the helper without opening any handle, :331–383 retains explicitly constructed Cards/operators. Shared public factories actually reach Process via `WyrdClient::connect_ambient`/`from_global` (`client.rs:73–88`, `config.rs:133`, `environment.rs:28–31`). Python `tests/integration/test_sdk_client_instantiation.py:41–47,91–105` omits eager AsyncBifrost; :111–121 covers only lazy async listing. The eager async public factory is independently reachable at `_client.py:1036–1063`.
- **Reachable consequence:** Tests pass if public no-client factory wiring breaks while manual resolution/explicit factories remain correct. Python eager async worker wiring/options can regress without its successful ambient journey noticing.
- **Minimum safe correction:** Drive actual exported Rust no-client factories, including async/blocking Bifrost with supported options and applicable Platform environment/no-credential cases, through real calls under process-start environment-only and saved-only tenant scenarios. The negative scenario must invoke those factories themselves. Reuse ambient-created handles for the saved-config-change retention proof; retain explicit-client coverage. Extend the existing Python ambient scenario to await eager AsyncBifrost with no client/options in both cases and make a real call. Do not require every eager/lazy handle combination for AC-007. Literal saved-only Platform is reserved to FIND-TASK-001-6.
- **Feasibility/owners:** Existing nextest process-per-test isolation (`.config/nextest.toml:1–17`), environment-owning mise/Postgres wrapper (`mise.toml:686–689`), protected `GlobalConfig::save_local` (:106–148), server configured binding (`WyrdTestServerBuilder::with_bind_addrs_for_test`, server.rs:4348), `PgFixture::seed_additional_tenant_with_uuid` (:275), and current API-key parser/hash/insert workflow (`WyrdTestServer::mint_machine_principal`, server.rs:3676–3720) are sufficient owners. Bounded fixture/runner support may provision startup-supplied credentials/known endpoints with isolated configuration and socket use. It must not add a production auth path, unsafe runtime environment mutation, test self-reexecution, or a second ambient resolver. Python already owns the needed fixture/configuration setup.
- **Ladder:** Delete the manual resolver as substitute acceptance proof; reuse the production factories and existing fixture/runner facilities. No new harness or dependency is justified. Ordinary bounded test support is not a material product decision.
- **Focused closure:** R1–R3 and P2 below. Run distinct ambient startup cases through their owning lane; preserve exact discovery/selectors if the scenarios are split. A broken public ambient factory must make these journeys fail. No whole journey aggregate is required for this remediation.

### FIND-TASK-001-3 — Delegated journeys do not assert server-observed subject attribution

- **Sources/status:** BEH-003; **CONFIRMED**, despite the invariant/security reports' broader source PASS.
- **Classification / severity:** MISSING verification; Important.
- **Obligation:** AC-003 and original Scenario 5 expressly require server attribution showing the delegated principal in each language.
- **Exact evidence:** Rust instantiation journey :287–321 observes a successful Card read and operator denial; Python instantiation journey :153–173 and TS journey :123–133 observe table denial followed by actor success. These establish authority intersection but never compare an observed subject ID. Existing Python Bifrost delegated test (`tests/integration/bifrost/test_bifrost_e2e.py:286–312`) and TS `bifrost-write.test.ts:224–251` similarly check allowed reads/denied writes. Existing server journey at `crates/wyrd/wyrd-testing/tests/bifrost/server/query.rs:1410–1452` does inspect retained audit subject/actor attribution, but is not a per-language connect journey.
- **Reachable consequence:** A language path could preserve restricted permissions while recording the wrong principal and still satisfy the current assertions. No actual attribution corruption is asserted: selected delegated auth still reaches the existing verified caller/audit owner.
- **Minimum safe correction:** Extend each existing representative delegated connect journey with an allowed operation and explicitly compare its server-observed principal with the known subject, retaining actor-versus-subject denial checks. Reuse current server audit/evidence reads and Bifrost SQL (the existing server test demonstrates the retained audit fields and synchronization), or an existing public returned attribution. No new authentication endpoint, permission model, audit implementation, durable contract, or fixture framework is needed. Do not require all handles or prescribe private fixture structure.
- **Ladder:** Existing retained attribution is the source of truth; reuse it rather than decode a token locally as a substitute or add a new server contract. Bounded support through the current testing owner is allowed if needed.
- **Focused closure:** R4/P3/T1 below. Each must compare server-produced subject identity for the actual delegated handle operation, rather than only successful/denied authorization or the token's client-side claims.

### FIND-TASK-001-4 — Required changed-item Rust documentation is incomplete or stale

- **Sources/status:** BEH-004, MAINT-002, STANDARDS-002; **REVISED**.
- **Classification / severity:** VIOLATION; mandatory AGENTS §16 hard gate, batched with substantive fixes.
- **Obligation:** AGENTS §16 / agent-rules rustdoc requirement; REQ-010/012 API-documentation closure. A green Clippy result cannot waive this rule.
- **Validated locations and minimum correction:**
  - `crates/shared/wyrd-client/src/state.rs:541,552`: both intra-doc links name removed `Bifrost::connect_with_config`; the body at :557–565 calls `connect_with` and options. Replace the links/explanation with the actual constructor/options contract; no alias.
  - `crates/shared/wyrd-client/src/client.rs:535,551`: new `client_for` has fallible construction `expect`s; new eager/cache test has connection expectations/assertions. Both lack `# Panics`. Describe actual construction/test assertion conditions, preserving their useful assertions.
  - `sdks/wyrd-sdk-rust/tests/integration/support/mod.rs:111`: materially changed `Deployment::cards` now awaits eager connect and expects success but has only a name-repeating doc line. Explain its fixture role, connection and panic condition.
  - `examples/rust/client_request.rs:19`: changed fallible eager main has no item rustdoc or `# Errors`; module prose does not satisfy the item rule. Document its request workflow and config/exchange/request failures.
  - `crates/wyrd/wyrd-cli/src/card.rs:677` (`build_cards`) and `src/workflow.rs:388` (`ambient_cards`): errors describe only client assembly, while bodies now propagate eager exchange errors. Document the actual eager connection/error boundary.
  - `crates/wyrd/wyrd-mcp/tests/bifrost/mcp/connectivity.rs:166` (`principals`): documented errors stop at binding/client construction, but eager connect now propagates refused/unreachable exchange. Include that condition.
  - `crates/wyrd/wyrd-mcp/tests/bifrost/mcp/query/investigation.rs:58` (`EvidenceInvestigation::start`) and `sdks/wyrd-sdk-rust/tests/integration/principal_roles.rs:71` (`author`): already-fallible helpers add `.expect("handle connects")`. Reuse their existing Result/error channel to propagate the new connection error and document it, instead of keeping an unnecessary connection panic. Preserve all subsequent setup/registration/authorization assertions. If a real invariant panic remains, document it.
  - Python native Bifrost `use_table/table/dropped/producer_count` at :656/:680/:892/:898 lack Errors docs only because this change incorrectly made local work fallible. FIND-TASK-001-1 should restore local behavior; **do not retain those consumer Errors-doc demands**. The four Cards getter connection-error paragraphs similarly become stale when views defer initialization and must describe the corrected local selection. First-use native `insert` (:688–711) and public base/AsyncBifrost prose (`_client.py:640–643,699–725,1027–1032`) should qualify synchronous bounded insertion with the approved initial lazy connection/error timing; do not claim setup is always IO-free or redesign it to make that sentence true.
- **Evidence/consequence:** Full bodies confirm these are changed/new operation contracts, not unrelated old undocumented items. Maintainers currently get removed API links or incomplete failure/panic/timing behavior. Rule explicitly blocks missing substantive touched-item rustdoc.
- **Ladder:** Delete stale links and accidental failure descriptions; reuse existing error propagation in already-fallible helpers. Add only accurate documentation for the resulting behavior, with no checker, compatibility shim, refactor, or runtime test solely for prose.
- **Focused closure:** Source review each listed final signature/body/doc pair. D1 below checks shared links. R5 checks the existing eager test only if its body changes; existing affected-helper tests run only if error propagation changes executable code. Formatting/lint checks cover the resulting write set; no new broad test aggregate is required for prose.

### FIND-TASK-001-5 — Revoked saved-login journey still expects connection success

- **Sources/status:** STANDARDS-003; **CONFIRMED**.
- **Classification / severity:** REGRESSION; Important.
- **Obligation:** Preserve migrated consumers/regression proof; REQ-003/015 eager required refresh; task consumer closure and AGENTS §§11–12.
- **Exact producer → consumer trace:** `sdks/wyrd-sdk-rust/tests/integration/saved_user_auth.rs:204–210` revokes the refresh chain and expires the saved access token. `HumanSso::revoke_saved` (`human_login.rs:304`) revokes that record's refresh token; `expire_saved_access` (:404–411) saves an expiry one minute in the past. `SavedLogins::cards` (:100–102) now awaits `Cards::connect_with`; shared Cards `handle.rs:219–225` calls `client.connected`, which calls access_token (`client.rs:103`). The renewable branch `auth.rs:840–850` mints through `SavedLoginSource::mint` (`saved_login.rs:474`) and `SavedLogins::renew` (:324–362); refusal maps to SavedLogin unusable. The test's `.expect("bob's login resolves")` at :215 therefore panics before `.list(...).expect_err(...)` at :216–218.
- **Reachable consequence:** The identity journey selected at `mise.toml:831` fails under the required eager behavior. Its omission from the generic SDK journey lane does not make it dormant. No auth production defect is implied.
- **Minimum safe correction:** Assert `WYRD_CLIENT_401_SAVED_LOGIN_UNUSABLE` on the eager connection refusal itself, preserving real revocation/expiry setup and the stable code. Update the test and `SavedLogins::cards` rustdoc for eager refresh timing. Other sibling saved-login tests should keep their successful-connect/permission/renewal assertions. TS's migrated revoked-login scenario already follows this timing.
- **Ladder:** Delete the obsolete success expectation and unreachable list step; reuse the existing Result and refusal assertion. No auth changes or test weakening.
- **Focused closure:** R6 below with the existing managed identity setup. This static trace establishes the defect; runtime reproduction is not claimed.

## Authority-blocking ledger item

### FIND-TASK-001-6 — Saved-only Platform acceptance contradicts preserved credential applicability

- **Sources/status:** unique focused follow-up claim, plus Platform scope in BEH-002/INVAR-002; **CONFIRMED**.
- **Classification:** approved-authority conflict / MISSING satisfiable acceptance; **SPEC_REVISION_REQUIRED**, not a bounded production fix.
- **Exact authority:** revision 9 AC-002 requires *each in-scope handle* to succeed against WyrdTestServer in both environment-only and saved-configuration-only cases; scope/D4 explicitly include Platform. REQ-002 preserves the existing resolver, INV-001 forbids capability/server behavior changes, D4 preserves platform-session exchange without tenant exchange, and the task prohibits new authentication paths.
- **Source evidence:** `Platform::connect` (`platform/handle.rs:70–72`) uses the shared `WyrdClient::from_global`. `connect_with` (:94–101) accepts only `ResolvedCredential::BearerToken` before the existing platform-session exchange (:109–118). Shared `ClientConfig::resolve_credential` (`config.rs:229–252`) returns Renewable for saved human login or resolves the credentials-file floor. That floor (`transport/credential.rs:366–370`) always constructs ApiKey; it does not classify file text as a presented bearer. `GlobalConfig::save_local` (:106–148) writes `[default].api_key`; ClientSection has no saved platform bearer source. The environment bearer path can supply the correct platform credential; saved endpoints plus environment bearer are mixed, not saved-only credentials.
- **Consequence:** No existing saved-only credential path can pass Platform's unchanged local variant check. More fixture provisioning or changing ports cannot satisfy the literal AC. A test that silently substitutes `connect_with`, an environment bearer or new saved source would misstate the approved obligation.
- **Required decision:** Explicitly approve whether AC-002 is qualified by existing credential-source applicability (saved-only success for tenant handles; Platform succeeds through its existing ambient platform credential/session path), or approve a new persisted platform credential source with its production security/ownership contract. The former requires a specification acceptance qualification; the latter is a new credential/persistence decision. This review selects neither without approval. D4 alone cannot silently erase AC-002, and AC-002 alone cannot authorize a forbidden new source.
- **Ladder:** Existing mechanisms have been exhausted: saving a tenant key, human login, or platform string in `[default].api_key` still yields rejected variants. Do not weaken Platform's bearer check, reclassify the shared tenant floor, add credential aliases, or route it through tenant exchange. No executable remediation/closure command is decision-complete until that applicability decision is approved. Environment/no-credential Platform proof remains bounded under FIND-TASK-001-2.

## Reconciled acceptance results

These are static implementation/proof assessments, not independent passing test results. Pending TASK-002 docsite/README work is not attributed to TASK-001.

| Obligation | Reconciled result / evidence |
|---|---|
| REQ-001–006 | Source PASS: approved forms, options, eager shared connection and language async/sync projections. Required direct ambient proof remains FIND-2; Platform saved-only acceptance is FIND-6. |
| REQ-007 | FAIL, FIND-1: constructors are IO-free, but local consumers initialize too early. |
| REQ-008 | PASS after successful native initialization; same successful owner reused. Local timing problem is REQ-007, not a second identity/lifecycle defect. |
| REQ-009 | PASS: constructor is nonblocking; eager AsyncBifrost runs in to_thread. No additional lazy insert architecture obligation is inferred. |
| REQ-010–011 | Source PASS public removal/visibility; state rustdoc closure fails FIND-4. Compile-fail doc execution remains an evidence limit. |
| REQ-012 | TASK-001 SDK API/rustdoc closure FAIL FIND-4; TASK-002 docsite/README acceptance remains deferred to its owner. |
| REQ-013–019 | Source PASS: authority text, retained client/cache/session, auth rename, single compact typed list with separate describe and no extra server behavior. |
| INV-001 | FAIL local capability regression FIND-1; also constrains the FIND-6 decision. No changed server permission/durable behavior found. |
| INV-002–005 | Source PASS: shared resolver, supplied/delegated identity retained, secret redaction, earned language boundaries. AC-003 proof still fails separately. |
| AC-001 | Source-backed public-surface coverage; compile-fail docs not independently executed. |
| AC-002 | FAIL FIND-2; Platform literal saved-only portion requires spec decision FIND-6. |
| AC-003 | FAIL proof FIND-3; preserved production auth is counterevidence to an implementation-corruption claim, not exact attribution proof. |
| AC-004 | FAIL Rust public missing-credential proof FIND-2 and local Python trigger FIND-1. Existing exchange-versus-bearer tests otherwise source-supported. |
| AC-005 | PASS source/tests inspected: constructor performs no IO. |
| AC-006 | TASK-001 docs FAIL FIND-4; authored stubs/generated declarations statically align and lane passes are claimed. TASK-002 scope remains its own work. |
| AC-007 | FAIL Rust public ambient proof FIND-2. Python ambient lazy and TS ambient retention plus explicit cache/renewal stories are useful; do not demand every combination. Existing post-expiry saved-login proof remains relevant; the revoked-login consumer must be fixed (FIND-5). |
| AC-008–009 | Source/test structure PASS: real list/own/foreign/denied/separate describe and typed compact projections; runtime lane results not independently reproduced. |
| Other non-goals | Source PASS within inspected cumulative change: local loaders remain deferred; no new wire/route/catalog/permission/cache/dependency/public Platform language surface or compatibility form found. |

## Focused closure commands and setup

These are recommendations, not executed checks. R1–R4 require the existing repository-managed Postgres lifecycle and migrations; supply each ambient case's client settings **before nextest starts** through the existing runner owner. No test may spawn itself or mutate Rust's shared process environment. Fixture implementation may refine scenario names; if so record and run their exact discovered selectors and keep all cases in the owning lane.

```bash
# R1–R4: within scripts/postgres/with-test-postgres.sh after mise run db:migrate:all:inner
mise exec -- cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=sdk_client_instantiation::ambient_connect_opens_every_handle)'
mise exec -- cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=sdk_client_instantiation::connect_without_a_credential_fails_eagerly)'
mise exec -- cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=sdk_client_instantiation::connected_handles_reuse_the_client_cache_and_renew)'
mise exec -- cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=sdk_client_instantiation::delegated_connect_uses_the_subject_principal)'

# R5: only if executable shared eager-test code changes
mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(=client::tests::server_handles_connect_eagerly_and_share_the_client_auth_path)'

# R6: within the managed identity setup described below
mise exec -- cargo nextest run --locked -p wyrd-ai --test integration --run-ignored=all -E 'test(=saved_user_auth::revoked_login_is_refused)'

# P1: from sdks/wyrd-sdk-python (current unit module, including corrected first-use assertions)
mise exec -- uv run python -m pytest -q tests/unit/test_sdk_client_instantiation.py
# P2/P3: same directory, after py:setup and within managed Postgres + db:migrate:all:inner
mise exec -- uv run python -m pytest -q -m integration tests/integration/test_sdk_client_instantiation.py::test_ambient_connect_opens_every_handle
mise exec -- uv run python -m pytest -q -m integration tests/integration/test_sdk_client_instantiation.py::test_delegated_connect_uses_the_subject_principal

# T1: from sdks/wyrd-sdk-ts/wyrd, after ts:build / ts:build:testing,
# within managed Postgres + db:migrate:all:inner
mise exec -- pnpm exec vitest run tests/integration/sdk-client-instantiation.test.ts -t 'delegated connect uses the subject principal'

# D1: shared API links, no runtime server required
mise exec -- env RUSTDOCFLAGS='-D rustdoc::broken_intra_doc_links' cargo doc --locked -p wyrd-client --no-deps
```

R6 needs the existing identity service preparation in `mise.toml:713–831`: managed Postgres/migrations, fresh Keycloak/Dex, seeded users/realms, and the issuer settings `WYRD_KEYCLOAK_ISSUER=http://localhost:8180/realms/wyrd-test` and `WYRD_DEX_ISSUER=http://localhost:5556`. Run its exact command while those services are up, using that established preparation. A plain SDK unit lane cannot prove it. Do not prescribe a full identity aggregate as focused remediation merely to obtain setup. Any additional specifically named behavioral test must include its exact focused command in implementation evidence.

Apply only format/lint/type checks appropriate to the final correction write set (`fmt`/`lints`; Python checks and authored stub/type checks when changed). No new checker, dependency, full gate, or complete user-journey aggregate is required by this validation; whole-change aggregates belong to change review.

Skipped execution and unrelated hardening as assigned. Remaining risks are the explicit Platform acceptance decision, the confirmed local timing/regression defects, and unexecuted focused closure proof; source-correct paths and recorded PASS labels do not close those obligations.
