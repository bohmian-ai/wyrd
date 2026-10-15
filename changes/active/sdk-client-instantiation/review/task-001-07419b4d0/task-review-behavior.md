# TASK-001 independent behavior re-review

Overall: **FAIL** — one remaining portion of the previously validated changed-item documentation finding. No new runtime defect found.

Subject: cumulative base `2f1db2194f5630df850b10987fe0f360946a525c` → candidate `07419b4d060303959384b682ca88e8673c722e63`. HEAD was checked before and after inspection and stayed at candidate. Source remained immutable; only this assigned report was written. This independent discovery review did not read another current-round reviewer report or rely on implementation completion conclusions.

Authority: approved spec revision 11, original TASK-001 at base, current AGENTS and agent rules, spec-driven-development and maintainer-style references, Wyrd Client model (`architecture/wyrd-design.md:254`), doctrine, and Bifrost public surface (`architecture/bifrost-design.md:1347`). Prior verdict, validated findings, behavior report, and focused follow-up were read as hypotheses and scope boundaries. Revisions 10/11 expressly supersede the prior process-environment proof prescription and impossible saved-only Platform case. TASK-002 owns docsite and README prose.

## Proposed finding

### BEH-R2-001 — VIOLATION: native lazy insert still documents unconditional nonblocking behavior

- Stable prior item: remaining subitem of **FIND-TASK-001-4**, not a new runtime finding.
- Exact location: `sdks/wyrd-sdk-python/src/bifrost/mod.rs:729–737` (`Bifrost::insert` rustdoc); actual initial connection at `:747`.
- Violated obligations: REQ-012 SDK API documentation closure, AC-006's accurate eager-failure qualifications, and AGENTS §16/agent-rules mandatory accurate operation and error rustdoc on materially changed Rust items. The prior validated FIND-4 explicitly required qualifying native insert's first-use timing.
- Source-local producer → consumer evidence: `Bifrost::__new__` (`:628`) stores `LazyConnection` with no initialized native handle; public `_BifrostBase.insert` (`python/wyrd/bifrost/_client.py:709`) invokes this native insert. Its `self.handle(py)?` (`src/bifrost/mod.rs:747`) reaches `LazyConnection::get` (`src/client.rs:128`), which drives the shared connect under the released GIL. That can synchronously resolve files, exchange credentials, and dial Bifrost before the bounded channel send. Yet native method documentation says “Non-blocking” unconditionally and lists only row/correlation, missing-table and queue-full errors, omitting the connection errors now propagated there.
- Reachable consequence: the changed native API contract misdescribes the first call made on an uninitialized Python handle, including whether it can block and fail for configuration/exchange/dial. This is a documentation acceptance failure; **no runtime failure beyond the approved semantics is asserted**.
- Counterevidence: public Python `_BifrostBase.__init__` (`_client.py:659`), `insert` (`:712–733`), and AsyncBifrost prose (`:1043`) now correctly state first-use blocking and connect errors. The shared Bifrost type prose also describes lazy setup. Those are correct but do not correct the materially changed Rust method's own unconditional contract, specifically retained in prior FIND-4.
- Minimal correction: qualify the existing native insert paragraph as nonblocking **once connected**, and include the initial shared connection's errors, as its public Python counterpart already does. Do not change insert behavior, introduce buffering, add an await form, or redesign admission. No runtime test is warranted for this prose-only correction; source comparison against `handle` and the already corrected public prose suffices, with the existing applicable format/lint checks.
- Severity/calibration: mandatory changed-item documentation closure only. All substantive runtime findings below are closed. The task-review skill says not to create a style-only remediation round; the orchestrator/independent validator must preserve that workflow constraint when reconciling this residual item.

## Independently expanded navigation and behavior traces

CodeGraph was used before source lookup (`WyrdClient/from_global/from_environment/connect_ambient/LazyConnection`, Python state/Bifrost owners, TS native owners), then the cumulative diff and current bodies were inspected. The map was expanded beyond the provided starting points:

| Caller path | Ownership and resulting behavior |
|---|---|
| Rust ambient HTTP handles | Cards `handle.rs:188`, Gateway `gateway.rs:67`, Principals `principals/handle.rs:49`, OperatorConnections `operator_connections.rs:58` → `client.rs:111` → `from_global:75` → single `from_environment:94` → `connected:130`. Existing auth owner performs exchange/refresh and retains one client/cache. |
| Rust explicit and Bifrost paths | Four HTTP `connect_with` forms call `connected`; Bifrost `facade.rs:123/147` validates queue then connects and assembles the existing transport. Blocking `blocking.rs:47/62` drives the same async implementation. No validation probe or second transport owner added. |
| Platform | `platform/handle.rs:70/94` selects ambient/supplied client, retains its existing bearer-only variant check and platform-session exchange. It never calls tenant `connected` before that exchange. Spec10 excludes a saved-only credential source; spec11 allows the shared injected resolver proof. |
| Python lazy Cards and views | `state/mod.rs:1652–1794`: one `SharedCards(Arc<LazyConnection<Cards>>)`; selection clones the shared owner without connecting. Registry operations call its native connection. The four views do not create independent identities/caches. |
| Python lazy Bifrost | `bifrost/mod.rs:520–603`: `Binding::Local` owns preconnection table. `handle` first connects, then transfers the current binding under its lock; local table/use_table serialize against that move. Counters read a connected handle if available or zero. Failed connect stores nothing. First-use callers share `LazyConnection`'s gate and successful OnceLock; no local table edit is overwritten by a stale pre-dial snapshot. |
| Python other handles | Gateway, Principals, OperatorConnections use the same LazyConnection/native shared factories. Public Bifrost/AsyncBifrost constructors hold arguments; Async eager and awaited calls use `asyncio.to_thread`. Native insert retains the approved initial synchronous connection boundary; no buffering redesign is inferred. |
| TS | `index.ts:1599/1892/1982/2785` factories await native connection results; native Cards/Gateway/Principals/operators retain the selected native client and call shared connect_with. Bifrost list `index.ts:1360` → native `lib.rs:1172` → shared Bifrost/QueryClient. Constructors remain private. |
| Discovery | `bifrost/query.rs:318` directly GETs the existing `/v1/bifrost/tables` using retained HTTP client; facade `:1033` and blocking `:213` forward it. Python TypedDict `TableEntry` (`_client.py:1342`) and TS alias of wire `BifrostTableEntry` project the existing fields. No describe fan-out/cache introduced. |
| Local loaders and consumers | `state.rs:490/561`, `workflow/mod.rs:140/162`, TableConfig `table.rs:176/195`: approved name migrations preserve local loading and existing registry resolution where external refs already require it. CLI command helpers await eager forms and propagate errors; server setup preserves Platform exchange. Rust/TS executable examples and MCP/testing consumers migrate constructors without new server routes or permissions. |

The new Python local binding and shared view state earn their existence at the foreign-runtime boundary: they preserve offline local access while retaining one native connection. They reuse existing native table mutation and Registry owner after initialization; moving these semantics into an alternate SDK transport or a new server owner would duplicate existing behavior. The Rust resolver addition removes the duplicate ClientConfig composition and is expressly approved by revision 11. No unearned new dependency, cache, catalog, authentication source, or persistent owner was found.

## Acceptance matrix — all 33 IDs

Every PASS is a static source-backed assessment, not an independently reproduced check result. Abbreviations: R = `sdks/wyrd-sdk-rust/tests/integration/sdk_client_instantiation.rs`; P = `sdks/wyrd-sdk-python/tests/integration/test_sdk_client_instantiation.py`; T = `sdks/wyrd-sdk-ts/wyrd/tests/integration/sdk-client-instantiation.test.ts`. Verification evidence means actual assertions inspected; reported executions are separately qualified below.

| Obligation | Implementation evidence | Verification evidence inspected | Result |
|---|---|---|---|
| REQ-001 common connect | Shared HTTP forms and Platform/Bifrost forms in navigation map; Python and TS factories | Rust surface `src/lib.rs:203`, Python unit HANDLES, TS unit construction enumeration | PASS |
| REQ-002 optional client, one ambient chain | `client.rs:75/94/111`; Python missing-client closures call shared factories; TS existing clientOf uses shared native resolution | R:120, P:95/116, T:77 environment/saved cases; spec11 injected Rust resolver | PASS |
| REQ-003 eager qualified exchange/transport | `client.rs:130`; Bifrost facade:147; Platform:94 | Shared eager/cache test `client.rs:585`; R:213, P:166, T:103 distinguish refused exchange/bearer | PASS |
| REQ-004 options without client | BifrostOptions facade:65; ambient connect:123; Python/TS optional table/byte budget | R:91/120 options with approved injected client; P:95 and T:77 public no-client options | PASS |
| REQ-005 language sync/async idiom | Rust async and blocking projection; Python sync plus AsyncBifrost to_thread; TS Promise factories | Surface tests and current signatures | PASS |
| REQ-006 Python constructor alongside factory | Python six HANDLE signatures optional; public Bifrost/Async signatures | Python unit `test_handles_expose_only_the_common_construction_forms`; P:95/116 | PASS |
| REQ-007 IO-free constructor; server-needed first use | LazyConnection:new/get; Bifrost Local binding; SharedCards typed views | Python unit `test_local_access_never_connects`, constructors tests; P:116/132 | PASS |
| REQ-008 eager/lazy equivalent after successful first use | Same native connect and retained native object; one connection shared across typed views | P:132,166,180,214 compare operations/errors/retention | PASS |
| REQ-009 Async construction does not block | `_BifrostBase.__init__` only native storage; AsyncBifrost.connect uses to_thread | Python async constructor unit; P:95 eager async and P:116 lazy async | PASS |
| REQ-010 obsolete forms removed | Handle old public names removed, Cards::new private, D1 loader/describe rename | Rust compile_fail docs `src/lib.rs:44–117`; Python/TS public-surface tests | PASS |
| REQ-011 Rust no public lazy construction / TS private constructors | Shared handle declarations and TS private construction | Rust surface/compile-fail coverage and TS source | PASS |
| REQ-012 TASK-001 API docs/examples use common forms | Examples migrated; stubs updated; native insert still has inaccurate first-use rustdoc | BEH-R2-001; TASK-002 docs-site/README excluded | FAIL |
| REQ-013 architecture/language rules | `wyrd-design.md:288`; Python reference:84; TS reference:184; patterns:150 | Cumulative authority diff and current text | PASS |
| REQ-014 one retained client/cache | Shared connect clones share underlying pools/auth; LazyConnection stores successful native handle once; Platform retains session client | R:379, P:214, T:152 mutate config then use handles/renew | PASS |
| REQ-015 HTTP handles eagerly exchange, no health probe | `connected` calls only existing access_token; Bifrost additionally dials existing channel | Shared mock test counts exactly one key exchange across HTTP handles and no bearer exchange; credential journeys | PASS |
| REQ-016 auth module rename, no alias | Rust `src/lib.rs:132`; Python auth module and native registration | Rust compile_fail old import; Python importlib rejects wyrd.principals; migrated consumers | PASS |
| REQ-017 exact list method all surfaces | Shared facade:1033/blocking:213, Python sync:1006/async:1269, TS:1360 | Rust surface, Python Bifrost tests and R:432/P:236/T:174 | PASS |
| REQ-018 unchanged compact entries, lazy describe | QueryClient:318 direct list; no schema fetch loop; separate describe preserved | R:432, P:236, T:174 list names then describe fields | PASS |
| REQ-019 typed existing wire projection | Rust Vec<BifrostTableEntry>; Python existing TableEntry TypedDict; TS wire alias TableEntry | Current wire fields/declaration/surface source; no new wire definition | PASS |
| INV-001 capabilities/server/durable behavior preserved | No new server implementation or wire/permission behavior; local getters restored; list only new capability | Local regression proof and table authorization/tenant journeys | PASS |
| INV-002 ambient owner stays shared | Single WyrdClient::from_environment; Python/TS call native shared resolution | Direct resolver and boundary trace; R/P/T ambient journeys | PASS |
| INV-003 supplied/delegated identity preserved | Explicit factories use selected client; Platform existing derivation retained | R:276, P:180, T:123 now inspect server retained subject and actor chain | PASS |
| INV-004 secret redaction | No new secret rendering; errors use existing typed mapping; handle debug stays redacted | Shared eager test asserts refused secret absent from Debug; existing client redaction paths | PASS |
| INV-005 boundary-only language code | Python lazy local state/threads and TS Promise result conversion reuse native owners | Source call paths; no SDK transport copy | PASS |
| AC-001 enumerated surface/removal tests | Rust exported symbol references + removed-form compile_fail docs; Python/TS public classes | Actual surface tests inspected; doc-test execution not independently observed | PASS |
| AC-002 ambient per-language journeys | R:120 uses spec11 single injected resolver for env/saved and Platform env; P:95 includes eager Async; T:77 | Real WyrdTestServer calls for each tenant handle; Platform saved-source exclusion honored | PASS |
| AC-003 server-observed delegated principal | Delegated Bifrost allowed query plus retained audit principal filter and actor comparisons | R:276–366, P:180–211, T:123–149 retain permission denial checks too | PASS |
| AC-004 missing credentials, lazy first use, credential distinctions | Shared resolver returns stable missing code; explicit eager completes required exchange; Python constructors defer | R:182/213, P:151/166 plus async missing unit, T:92/103; spec11 governs Rust proof | PASS |
| AC-005 Bifrost/Async no-IO construction | Constructors perform no config read/dial | Python no-credential/unreachable local test and async constructor test | PASS |
| AC-006 TASK-001 API/stub/declaration accuracy/checks | Authored stubs/current native exports align; TS generated declaration signatures align; native insert timing remains inaccurate | BEH-R2-001; relevant lint/type/codegen results recorded as claims; docs:check deferred to TASK-002 | FAIL |
| AC-007 ambient once/cache/renewal | Retained native owner and cache with shared AuthMiddleware | R:379 saved files altered then retained handles reused/renewed; P:214/T:152 endpoint/key altered then handles reused; shared mock counts exchanges | PASS |
| AC-008 authorized compact discovery journey | Existing list route and retained-client auth; separate describe | R:432/P:236/T:174 own user/built-in/foreign exclusion/RBAC denial/fields | PASS |
| AC-009 names/types/no catalog/schema | Exact list names and compact wire projections; separate describe remains | Public-surface and generated-source checks; no list_catalog/list_schemas added | PASS |

REQ-012/AC-006 FAIL is confined to the one SDK Rust documentation subitem above. It does not claim pending TASK-002 authored docs are absent TASK-001 behavior.

## Prior finding closure

| Prior ID | Current exact source and relevant proof | Assessment |
|---|---|---|
| FIND-TASK-001-1 local Python use initializes transport | Bifrost `src/bifrost/mod.rs:564–585,695–724` transfers only on server operation; local getters/use_table/counters do not call get. Cards `src/state/mod.rs:1666–1794` shares one lazy owner across views. Python unit local-access and P:132 exercise offline locals then first server use/retention. | CLOSED |
| FIND-TASK-001-2 ambient proof / eager Async missing | `client.rs:75–101` has one resolver, from_global delegates to from_environment(Process). R:120/182/379 now directly uses that resolver under spec11. P:98–109 awaits AsyncBifrost.connect and list in both ambient cases. | CLOSED under approved spec11; no superseded startup/process-env proof demanded |
| FIND-TASK-001-3 delegated attribution | R:336–366 filters retained audit on subject ID and asserts actor chain; P:198–211 and T:134–149 read authorized retained audit with subject binding and compare actor IDs. All preserve subject-denied/actor-allowed behavior. | CLOSED |
| FIND-TASK-001-4 rustdoc/errors/links | state.rs:541/552 corrected to connect_with/options; client.rs:563/581 panic contracts added; support/mod.rs:110 documents eager panic; client_request.rs:18 Errors added; CLI card.rs:668/workflow.rs:384 document exchange; MCP connectivity.rs:164 and investigation.rs:56 document exchange and propagate; principal_roles.rs:69 uses Result; Cards getters/public Python first-use prose corrected. Native insert src/bifrost/mod.rs:729–737 remains unconditional. | PARTIALLY CLOSED; BEH-R2-001 only remaining proposed item |
| FIND-TASK-001-5 revoked login test expects connect success | saved_user_auth.rs:201–224 preserves real revoke+expiry then asserts error from awaited cards connection, code WYRD_CLIENT_401_SAVED_LOGIN_UNUSABLE. Helper :96 documents eager renewal. | CLOSED; managed Keycloak/Dex focused run is recorded, not independently executed |
| FIND-TASK-001-6 Platform saved-source conflict | Spec10/11 AC-002 explicitly excludes Platform saved-only; Platform source :70/94 still bearer-only/session exchange; R:155–173 uses injected ambient platform credential, no-credential resolver R:182. | CLOSED by approved authority; no saved platform source required |

## Counterevidence and excluded proposals

The initial lazy insert can block until shared connection succeeds. This remains expressly approved; it is not a request for preconnection queueing, a new async insert, or moving network admission into the writer. Local AsyncBifrost getters now remain local, so the previously reachable event-loop blocking regression there is gone. Failed lazy initialization retries rather than caching a failure; no second resolver is introduced. Platform bearer/session behavior is preserved and saved-only Platform success is no longer an obligation. An injected Environment that actually runs WyrdClient::from_environment is approved Rust acceptance proof; demanding runner variables, fixed ports, provisioned keys, or process-global mutation would resurrect rejected scope.

The source supports connection retention, but token equality alone is weaker than an exchange counter. The shared mock test explicitly counts key exchanges across the HTTP handles, while per-language journeys prove selected endpoint retention and later renewal. Taken together this is credible bounded regression evidence; no additional harness or every-handle/every-credential combination is required. No optional improvements are proposed.

## Verification notes and limits

Static only, as assigned. No tests, builds, generators, formatters, runtime children, or baseline runs were executed. Current TASK-001 evidence records six Rust instantiation journeys, 263 shared-client library tests, four CLI managed-postgres cases, 18 Python unit plus eight integration cases, six TS instantiation cases, managed revoked-login proof, formatting/lints/type checks, and broken-link rustdoc. Those pass labels are claims, not reproduced results; actual relevant assertions and selectors were inspected. Some recorded commands are grouped or abbreviated (notably CLI selectors), so this report cannot independently establish exact focused execution completeness. Earlier whole-task lanes and generated checks remain recorded evidence, not a fresh runtime guarantee.

The remaining proposed correction is prose-only and needs source verification against the existing native handle path and public Python documentation, not a new test or broader aggregate. HEAD rechecked as `07419b4d060303959384b682ca88e8673c722e63`; no source drift observed.

Skipped execution and TASK-002 docsite/README acceptance. Residual risk is unexecuted verification and the one inaccurate native first-use contract; no new runtime correctness finding was established.
