# TASK-001 independent invariant review

Result: **FAIL**. Two bounded proposed findings: `INVAR-001` and `INVAR-002`.

## Immutable subject and method

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/sdk-client-instantiation`.
- Base: `2f1db2194f5630df850b10987fe0f360946a525c`.
- Candidate: `fde7d396a3917964dc344ede2ed09046e179aa66`.
- Authority: approved specification revision 9; original TASK-001 read with `git show BASE:changes/active/sdk-client-instantiation/tasks/TASK-001-unify-sdk-handle-instantiation.md`; AGENTS.md; architecture/agent-rules.md; Client model in architecture/wyrd-design.md; doctrine; applicable Bifrost boundary; language references; spec-driven-development and maintainer-style references.
- Read the shared subject and independently expanded its map through CodeGraph before source searches. Read the cumulative diff and current source, including SDK boundary owners, generated declarations, consumer migrations, and new language journeys. No other discovery report or implementation completion conclusion was used.
- Static review only. No tests, build, generator, formatter, or runtime child processes were executed. Available green-lane claims are not independent execution evidence.
- HEAD was rechecked and remains the candidate. Only this assigned report was written.

## Findings

### Important — INVAR-001: local Python access initializes the server connection

Classification: **INCORRECT / REGRESSION**.

Obligations: REQ-007, INV-001; architecture/references/languages/python-api-and-stubs.md requires avoiding hidden IO in property accessors and says the first *server operation* initializes the handle.

Exact changed locations:

- `sdks/wyrd-sdk-python/src/bifrost/mod.rs:662` (`use_table`), `:682` (`table`), `:893` (`dropped`), `:899` (`producer_count`).
- `sdks/wyrd-sdk-python/src/state/mod.rs:1728` (`Cards.data`), with identical sibling calls at `model`, `prompt`, and `workflow` getters.
- Shared cause: `sdks/wyrd-sdk-python/src/client.rs:135` blocks on the connection when these local consumers call `LazyConnection::get`.

Producer-to-consumer evidence:

1. The constructor deliberately leaves the native connection unset and retains initial table/options at bifrost/mod.rs:587–598.
2. Bifrost's local table getter and setter nonetheless request `handle(py)`; the same is true of counters whose correct unopened value is zero. `handle` calls shared Rust `Bifrost::connect` / `connect_with` (554–565).
3. `LazyConnection::get` releases the GIL but synchronously blocks the calling OS thread on credential exchange and gRPC establishment (client.rs:129–136). GIL release does not free an asyncio event-loop thread.
4. Public `_BifrostBase.table`, `use_table`, `dropped`, and `producer_count` forward synchronously to those native entries (_client.py:677–737). `AsyncBifrost` inherits them (_client.py:1027), while its contract specifically says the inherited local operations perform no IO (1030–1032).
5. Cards similarly connects merely to return a typed registry view; the view selection itself is local. The sibling getters all cross the same boundary before any `get`, `list`, or other registry operation.

Reachable consequence: `b = Bifrost(table=table); b.table`, `b.use_table(other)`, or `Cards().data` can now read configuration, exchange credentials, dial a transport, and fail despite no server operation being requested. `AsyncBifrost().producer_count` or `.use_table(table)` can synchronously stall unrelated event-loop work. Constructing before configuring the environment and inspecting/selecting local state can prematurely bind the handle to the old identity; REQ-007 selects it on first server use.

Minimal root correction: keep local state access and typed-view selection outside the initializing connection accessor. Reuse the existing retained initial table/options for unopened Bifrost; read/swap that binding locally until initialization and then delegate to the existing native binding owner. Unopened counters are zero. Let Cards views retain the parent's same lazy connection rather than materializing a connected Cards merely to obtain a view; initialize on an actual view operation. Preserve once-only connection identity and eager factories. Do not add another transport, authentication cache, endpoint resolver, or general lifecycle framework.

Focused closure proof: with no usable credentials, both Bifrost facades can inspect/swap their table and read zero counters; Cards can obtain every typed view. No connection should occur until an actual server operation. A real first operation must still resolve exactly once and preserve the selected binding/client. An asyncio heartbeat alongside these local accesses should remain responsive without using a synthetic load test. The existing unit test's `FIRST_USE` list (test_sdk_client_instantiation.py:24–26) currently expects configuration errors from `producer_count` and `.data`, explicitly pinning the wrong boundary; update that expectation with the correction.

Counterevidence considered: constructors themselves perform no IO, all true async server operations use `asyncio.to_thread`, and successful native connections are shared by OnceLock. Those properties are correct but do not make property access a server operation. `insert` also calls the initializing accessor (bifrost/mod.rs:707); this report does not prescribe a new async insert API or a second buffer, because the approved first-server-needed-operation contract needs separate care there. The definite local-access defect above is independently correctable.

### Important — INVAR-002: the required no-client journeys bypass the public Rust connection entry points

Classification: **MISSING verification**, with an additional omitted Python async eager path.

Obligations: AC-002, no-credential part of AC-004, ambient-path portion of AC-007; AGENTS.md §11 requires real SDK → real server user journeys for the shipped capability. AsyncBifrost is explicitly in scope.

Exact locations:

- `sdks/wyrd-sdk-rust/tests/integration/sdk_client_instantiation.rs:50`, `:106`, `:146`, `:180`, `:201`.
- `sdks/wyrd-sdk-python/tests/integration/test_sdk_client_instantiation.py:41`, `:99`, `:111`.

Evidence and consequence:

- Rust's test-only `ambient(Environment)` reconstructs a client by invoking `GlobalConfig::load_in` and `ClientConfig::from_environment` itself. `open_every_handle` accepts the resulting explicit client and calls only `connect_with` (106–124). Both the environment and saved-only stories use that helper, and Platform likewise calls `connect_with` (180). None drives any public no-client `connect` entry point.
- The Rust no-credential story fails in that test resolver (201 onward), before constructing any handle, so it would pass if a handle's no-client entry point stopped propagating that error. The retention story also starts with `ambient(...)` then explicit `connect_with`, and only constructs Cards/OperatorConnections, so it does not prove the required ambient handle path remains selected after configuration changes.
- Python's eager real-server enumeration omits AsyncBifrost. Its only actual successful AsyncBifrost use in this file is the lazy `AsyncBifrost().list_tables()` story at 111–112; `await AsyncBifrost.connect()` has only the no-credential unit scenario. Thus the eager async ambient path, including saved-only resolution/options, is unproven.

The source implementation generally looks correct: Rust ambient methods do call `WyrdClient::from_global`/`connect_ambient`, and the Python async eager factory passes through the shared native factory in a worker. This is not a claim that those paths currently misresolve. It is a mandatory acceptance gap: the test suite remains green if the public everyday entry points are broken while explicit-client behavior is retained, precisely the regression this task must prevent.

Minimal testable root correction: exercise actual public no-client `connect` entry points in isolated environment-only and saved-only real-server journeys. Use the existing process/CLI test infrastructure when Rust environment isolation is required; do not replace the public entry points with a hand-built resolver or expose a new production injection surface. Include Platform and Rust blocking Bifrost where applicable, and actual no-client failure at each handle factory. Extend the existing Python async scenario to await `AsyncBifrost.connect` in both ambient cases and make a real call with supported options. Reuse those same connected ambient handles for the configuration-change/cache/renewal assertions rather than creating an explicit substitute. No source architecture change or new test framework is required.

Counterevidence considered: Rust public-surface function references do prove the symbols exist; the fixed-Environment resolver avoids unsafe shared process-environment mutation; the TypeScript journey does exercise public no-client factories; Python lazy AsyncBifrost coverage exists. These are useful proofs, but none supplies the missing public Rust and Python async eager journeys.

## Requirement / acceptance / invariant matrix

`PASS` below means supported by static source and inspected proof structure; it does not certify execution of claimed lanes. TASK-002's prose work is excluded explicitly.

| Obligation | Implementation evidence | Verification evidence / limit | Result |
|---|---|---|---|
| REQ-001 common connect for every scoped handle | Shared handle connect pairs, Python factories, TS async statics, Rust Platform | Three public surface test owners | PASS |
| REQ-002 optional client/shared ambient resolution | Rust connect_ambient/from_global; TS clientOf; Python LazyConnection native branches | Production path correct; required direct Rust ambient journeys absent (INVAR-002) | PASS |
| REQ-003 eager exchange/refresh/transport, bearer/cache exemption | client.rs:103–105 access_token; Bifrost facade.rs:147–155; auth bearer existing branches | Shared wiremock credential test; language credential stories | PASS |
| REQ-004 construction options without required client | BifrostOptions; Python optional table/client/byte limit; TS options object | Python/TS ambient options registrations; Rust surface/call sites compile by claim | PASS |
| REQ-005 language IO async/sync forms | Rust async handles/blocking facade; Python AsyncBifrost.to_thread; TS Promise native factories | Async eager success not journey-tested; pure local async access regression INVAR-001 | PASS |
| REQ-006 Python constructors retained, optional arguments | Native __new__ signatures; pure-Python base and factory signatures | HANDLES unit enumeration | PASS |
| REQ-007 initialize on first server operation only, once | OnceLock is correct, but local properties and view getters invoke connection accessor | Unit FIRST_USE incorrectly treats local access as server use | FAIL |
| REQ-008 initialized eager/lazy equivalence | Same Python native connection function, native handle used thereafter | Python eager/lazy real-server and same-code failures | PASS |
| REQ-009 AsyncBifrost constructor does not block | _BifrostBase.__init__ creates argument-only native wrapper | Async constructor unit scenario; no IO constructor bodies | PASS |
| REQ-010 obsolete Rust forms removed without aliases | Handle diffs remove from_env/with_client/option-specific connects | Rust compile_fail docs and surface refs; execution of compile_fail docs not independently evidenced | PASS |
| REQ-011 TS private constructors; no Rust public lazy constructors | TS private constructors retained; Rust new is crate-private | TS static enumeration, Rust source and compile-fail docs | PASS |
| REQ-012 SDK docs/examples use approved forms, primary no-client | SDK/API docs, executable examples, public auth rename | check:examples claimed; TASK-002 owns docsite/README prose | PASS |
| REQ-013 architecture/language rules state unified model | Client model paragraph, patterns and language-reference changes | Static authority inspection | PASS |
| REQ-014 one retained selected client and cache | Shared handles retain cloned Arc auth/pools; Python stores successful handle; TS native fields; Platform retains session client | New retention stories + existing saved-login/renewal/auth tests; ambient Rust proof gap separately recorded | PASS |
| REQ-015 HTTP eager exchange without probe, Bifrost persistent transport | connected -> access_token; no added health route; assemble dials gRPC | Wiremock tenant/platform request counts; bearer stories | PASS |
| REQ-016 auth module, no principals alias | Rust SDK re-export renamed to auth; Python package/native registration rename | Python import failure and Rust compile-fail old path | PASS |
| REQ-017 list on every async/blocking/language Bifrost facade | QueryClient.list_tables, async/blocking facades, Python two list methods, TS listTables | Three discovery journeys + Rust public references | PASS |
| REQ-018 unchanged compact authorized entries, separate describe | One GET request_json; boundary serde conversions; no describe fan-out/cache | Existing wire type + registered-table, foreign-tenant, denied principal and describe stories | PASS |
| REQ-019 typed existing discovery projection | Rust BifrostTableEntry; existing Python TableEntry TypedDict; TS existing generated BifrostTableEntry as TableEntry | Source declarations and codegen/typecheck claims; no new wire model | PASS |
| INV-001 capabilities/server/permissions unchanged except list | Server service/routes untouched; existing native capabilities reused | Pure local access now makes IO and fails (INVAR-001) | FAIL |
| INV-002 ambient resolution remains shared Rust owner | All SDK ambient paths reach shared resolver; no boundary environment lookup | Shared resolver tests; Python/TS ambient journeys | PASS |
| INV-003 explicit/delegated identity remains authoritative | connected clones selected client's auth; all language factories forward it; Platform existing derivation retained | Different subject/actor permissions in each language's delegation story | PASS |
| INV-004 no credentials in errors/repr/Debug | Existing SecretString/SecretBearer, redacted client auth; no new secret interpolation | Shared credential test explicitly excludes secret; existing auth debug tests | PASS |
| INV-005 foreign runtime boundaries only; shared connect | Python LazyConnection retains initialization state; N-API only awaits owner operations; no transport reimplementation | Full boundary-source inspection, dependency checks claimed | PASS |
| AC-001 enumerate common/removed forms | Rust symbol references/compile_fail, Python HANDLES, TS constructor static enumeration | Optional args confirmed from source; test lanes claimed | PASS |
| AC-002 real no-client each handle/environment+saved/options | Python sync/TS paths real; Rust only explicit-client helper; Python async eager omitted | INVAR-002 | FAIL |
| AC-003 delegated client/server subject | All supplied clients retained; real server permits subject reads/refuses subject-forbidden operations | Each language tests subject-vs-actor authority; existing attribution journeys remain | PASS |
| AC-004 missing credentials at public connect; exchange-vs-bearer; lazy code parity | Production shared errors preserved | Rust no-credential test stops at private ambient helper, never public connect; INVAR-002 | FAIL |
| AC-005 no-IO Python Bifrost constructors without server | Argument-only native new and pure-Python constructor | no_credentials unit scenarios; no socket/exchange/dial path in constructor | PASS |
| AC-006 docs/stubs/types/generator checks | SDK stubs edited; native-generated declarations align; authority/examples updated | Claimed lint/type/codegen/N-API/check:examples; docsite check deferred to TASK-002 | PASS |
| AC-007 ambient selected once/cache reuse/renewal | Production retained auth owner remains correct; Platform session path unchanged | Rust uses explicit clients and only Cards/operators; Python eager ambient and Async path absent from retained-handle story; INVAR-002 | FAIL |
| AC-008 list own/built-in/foreign/denied/separate describe | Single retained query-client request in each language | Three new real-server discovery stories inspect these outcomes | PASS |
| AC-009 exact method names/types/compact/no catalog aliases | Exact facade declarations; existing typed wire projection; no schema field in entry type | Rust public references + generated/native/typecheck claims and real server list return usage; no new catalog APIs | PASS |

## Non-goals and regression boundaries

| Boundary | Evidence | Result |
|---|---|---|
| Local loaders retain semantics, only Rust `_with` spelling changes | state.rs rename preserves from_path then client OnceLock; workflow/mod.rs uses crate-private Cards::new for ref hydration, avoiding eager connection for wholly local bundles; binding migrations only rename call | PASS |
| WyrdClient construction outside scope | Public constructor/resolver behavior unchanged; new connected/connect_ambient are private helpers composing existing access_token | PASS |
| Run/Observe/WorkflowCards remain derived capabilities | Call-site rename in observation tests; no new constructors or capability logic | PASS |
| TableConfig remains an inert value with approved describe pair | table.rs only describe_with/describe rename and shared ambient from_global | PASS |
| Platform remains Rust-only and platform-authenticated | Existing platform session body unchanged; Python/TS gain no Platform projection | PASS |
| No new wire/server behavior/permissions/audit/durable state | No server route/service/wire type change; main/CLI/test consumers rename connection calls | PASS |
| No new dependencies/features/transports/token caches | Manifests and lockfiles absent from diff; foreign wrappers use shared auth/runtime/transport | PASS |
| No compatibility/catalog/filter/pagination/client-discovery cache | Old names removed; list is zero-argument direct query client projection; existing schema cache predates this task | PASS |
| UI and authored docsite/README guidance remain TASK-002 | UI not changed; executable example and reference owners changed here | PASS |

## Expanded ownership / reuse map and Ponytail assessment

- **Credential identity:** producer is ClientConfig/ResolvedCredential; WyrdClient retains Arc<AuthMiddleware>; connected calls access_token but does not replace auth or pool identity. HTTP and gRPC consume the same auth owner. Renewal and force_refresh remain in auth.rs:840 and :903. Creating a new auth cache or connection validator would be incorrect; the candidate correctly reuses this owner.
- **Platform sibling path:** platform/handle.rs:94 reads the selected bearer, performs the existing platform-session exchange, and binds a session client through with_session. It never invokes tenant connected. Adding the ambient wrapper does not change this exception.
- **HTTP-only handles:** Cards retains RegistryEngine; Gateway/Principals/operators retain their client. New entry points merely select/eagerly initialize it. Private Cards::new is earned by existing offline local loaders, not a compatibility API.
- **Bifrost read/write identity:** QueryClient::new clones the selected WyrdClient (shared auth and pools); BifrostGrpcTransport::connect uses its existing gRPC connection/auth. WriterPool and ClientScope remain existing owners. BifrostOptions replaces three constructor variants with one approved value; no speculative builder or configuration layer was added.
- **Python lifecycle:** std OnceLock plus one mutex earns its existence because Python alone has lazy construction and competing first callers. Existing Rust WyrdState uses OnceLock for its own client, but importing that bundle owner into all SDK handles would be unrelated coupling. Reusing the same runtime/trace bridge and shared native connect is appropriate. The new state is reasonable; applying its initializing accessor to local consumers is the defect, rather than a need for another lifecycle abstraction.
- **TS lifecycle:** clientOf uses the selected public client, native factories clone its shared Rust state, await owner connect_with, and project typed native handle/error results. Existing NativeBifrostConnection/nativeHandle shape justifies equivalent connection results for HTTP handles. No alternate resolver or generic result framework is necessary.
- **Discovery:** existing QueryClient is the narrow HTTP owner. The single GET, serde projection, and existing list/describe wire models are sufficient. No new aggregator, authorization layer, filtering object, schema prefetch, or durable catalog is earned or added.
- **Consumer closure:** CLI/server/testing/MCP consumers adopt connect_with and await required exchange, table options move into BifrostOptions, local loaders migrate `_with` spelling, and TS users await previously synchronous handle factories. No additional server owner was changed to compensate for client construction.

## Verification notes and limits

Inspected production paths, the three new journey files, public-surface tests, corresponding existing auth/token and saved-login coverage, consumer changes, authored stubs, generated declaration changes, and relevant mise task definitions. The candidate's claimed lane results were treated as claims. Tests can be well-formed and green while bypassing the required entry point, as INVAR-002 demonstrates.

No baseline execution, runtime reproduction, build, lint, code generation, or focused test was performed. No source edits were made. Compile-fail documentation execution is not established by `mise run test:wyrd-sdk`, whose command is `cargo nextest run --locked -p wyrd-ai --lib`; this is a residual evidence limit, not an additional defect claim. Existing saved-login journeys cover post-expiry refresh, while the new 35-second token stories wait six seconds and exercise proactive refresh inside the existing 30-second skew. This report does not infer broken post-expiry behavior from that choice.

Independent validation should confirm the two proposed findings against source and preserve the minimal correction boundaries. No optional refactors or unrelated hardening are requested.
