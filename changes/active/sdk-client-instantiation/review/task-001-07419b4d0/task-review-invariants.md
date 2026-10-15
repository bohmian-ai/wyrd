# TASK-001 independent invariant re-review

Result: **FAIL** — one residual documentation obligation from `FIND-TASK-001-4`; no new runtime, identity, concurrency, discovery, or lifecycle defect established.

## Immutable subject and authority

- Base: `2f1db2194f5630df850b10987fe0f360946a525c`.
- Candidate: `07419b4d060303959384b682ca88e8673c722e63`.
- Prior candidate: `fde7d396a3917964dc344ede2ed09046e179aa66`.
- Reviewed the cumulative base-to-candidate diff, using the latest remediation diff for navigation rather than as the acceptance scope.
- Read the original TASK-001 from the base, current approved specification revision 11, shared subject, prior verdict/validated ledger and relevant prior invariant/concurrency discovery evidence. No current-round reviewer report or implementation completion conclusion was used.
- Governing authority: supplied AGENTS.md; `architecture/agent-rules.md`; `architecture/wyrd-design.md` Client model; doctrine Public surfaces; Bifrost Public surface; relevant client-pattern, Python API/stubs, TypeScript, maintainer-style, and spec-driven-development references. Also read the task-review and applicable Rust/Python and Ponytail review skills; repository-specific rules take precedence over their generic examples.
- Used CodeGraph before code lookup, then expanded its map through source, diff, caller and declaration inspection. Reviewed manifests and canonical mise lane definitions without executing them.
- Revision 10's Platform applicability and revision 11's injected `Environment` proof are accepted authority. This report does not revive process-environment mutation, runner provisioning, saved platform credentials, or preconnection queue redesign requirements.
- Static review only: no tests, builds, generators, formatters, runtime test subprocesses, or source changes. Only this assigned report is written.

## Review Findings

### Critical

None.

### Important

#### INVAR-R2-001 — remaining native insertion rustdoc contradicts first-use connection

Prior ledger ID: **FIND-TASK-001-4**; classification: **VIOLATION**, residual mandatory changed-item documentation closure.

**Location:** `sdks/wyrd-sdk-python/src/bifrost/mod.rs:727` (`Bifrost::insert` rustdoc), especially the unconditional “Non-blocking” claim at line 729 and the connection-free error list at lines 735–737. The actual initializing call is line 747 (`self.handle(py)?`).

**Violated obligation:** Original task owns SDK API documentation and accurate eager/lazy failure timing (REQ-012, TASK-001 portion of AC-006); AGENTS mandatory changed-item substantive rustdoc; prior validated FIND-4 explicitly requires the native `insert` timing/error qualification as well as the public facade prose.

**Producer → state → consumer:** The constructor leaves `LazyConnection<NativeBifrost>` unopened (`mod.rs:628–635`). A first insert reaches `Bifrost::handle` (`:568–588`), whose `LazyConnection::get` call runs the shared Rust connection through `runtime().block_on` with the GIL released (`src/client.rs:126–145`). That connection may perform the required credential exchange and dial gRPC; only then does `insert` enqueue. Consequently first insert can block and can raise the existing configuration, credential, exchange, budget, or transport errors, before bounded queue admission occurs. The native method's own rustdoc still promises unconditional nonblocking behavior and omits those first-use failures.

**Sibling consumers and counterevidence:** Public `_BifrostBase.__init__` and `insert` in `python/wyrd/bifrost/_client.py:654–668,704–727` now accurately describe initial blocking connection/errors. `AsyncBifrost`'s class prose at `:1040–1049` also tells callers to connect first or await an operation before synchronous insert. Native `Bifrost::handle` documents its connection errors. These close the public-facade part of FIND-4 but do not correct the separately required, materially changed native Rust item.

**Consequence:** Maintainers consulting the native insertion contract still receive the wrong blocking/failure boundary for the very behavior added by this task. This is a documentation-only residual; it is not evidence that the approved first-insert runtime behavior is wrong, and it does not justify changing that behavior.

**Smallest safe correction:** Qualify native `insert` as nonblocking **after connection**, state that initial use connects synchronously through the shared handle, and include that accessor's existing first-use error contract. Reuse the accurate public wrapper wording or link to the native connection accessor; no new API, checker, queue, transport, or test harness. This is the unclosed portion of FIND-4, not a new FIND ID.

**Closure proof:** Static comparison of the final native method's rustdoc with its `self.handle(py)?` and enqueue sequence is sufficient. No runtime change or new test is required for this correction. Keep the already corrected public facade documentation and the existing behavioral proof.

### Suggestions

None. No optional refactor or hardening request is proposed.

## Value, state, owner, and consumer tracing

### Selected identity and eager construction

`WyrdClient::from_global` calls `from_environment(Environment::Process)` directly (`crates/shared/wyrd-client/src/client.rs:75–102`). `from_environment` composes the existing `GlobalConfig::load_in`, `ClientConfig::from_environment`, and `with_config` owners. The duplicate `ClientConfig::from_global` composition was removed. Endpoint precedence, saved login selection, credential classification, token cache, and authentication remain owned by the existing config/auth implementations. Python's lazy no-client branches use native `connect`; TypeScript's `clientOf` uses the existing native client construction, which reaches the same config/credential owners. No SDK reads its own endpoint or credential environment variables.

For Cards, Gateway, Principals, and OperatorConnections, ambient connection uses `connect_ambient`; explicit connection uses `client.connected()`. `connected` awaits `access_token` and returns a clone sharing the existing auth/cache and pools (`client.rs:130–132`), rather than creating another identity or validator. Subsequent operations continue through each handle's retained RegistryEngine/client. Bifrost validates queue options, awaits the same connected client, then assembles its existing gRPC transport, QueryClient and WriterPool (`bifrost/facade.rs:147–154,222–249`). QueryClient retains the selected client and list/describe requests use it. No health request or validation probe was added for supplied bearer/fresh cached tokens.

Platform remains separate: ambient selection calls the shared resolver, and `Platform::connect_with` retains the existing bearer-only platform-session exchange and session-bound client (`platform/handle.rs:70–118`). No tenant exchange or new credential source was introduced.

### Python retained local state and typed views

`LazyConnection` retains only the optional native client, an empty OnceLock and a mutex at construction. On first server operation it captures trace context, releases the GIL, acquires the gate, rechecks the OnceLock, connects, and publishes one successful native handle (`sdks/wyrd-sdk-python/src/client.rs:85–148`). The gate is dropped within the detached closure before Python reattaches. Failure returns before publication, so a later operation may retry; after success, operation failures do not clear or re-resolve the handle. No Python object or `Bound` is retained across its async work.

Bifrost's `Binding::Local` is the sole write target until the successful connection is handed back. The connection is built with no target; the first caller then takes the binding lock and moves the current local target into the native active-table owner (`src/bifrost/mod.rs:548–588`). Concurrent local `use_table` before that move changes the same protected Local value; after that move it delegates to native `use_table`. A second first-use caller cannot reinstall the old target because it sees `Binding::Native`. No connection or GIL release occurs while holding the binding lock. All methods receiving a native handle pass through this publication step before using it. Native table/register locking remains bounded around local state, with the network round trip outside the active lock; existing concurrent register/binding identity checks are preserved.

The table getter and setter remain local (`:697–724`), and unopened counters return zero through noninitializing `connected()` (`:934–945`). They no longer select an endpoint or principal. Both public Python facades use this native state. Async server calls and eager AsyncBifrost connection remain in `asyncio.to_thread`; inherited local accessors now perform no network setup. First synchronous insert still initializes synchronously, as approved and now accurately documented publicly; this review requires no alternate queue or async insert surface.

`SharedCards` retains an `Arc<LazyConnection<Cards>>` (`src/state/mod.rs:1663–1684`). The data/model/prompt/workflow getters clone only that pointer (`:1745–1792`). Each view's actual registry methods call `inner.get(py)` before delegating to the existing registry helpers; artifact downloads and Workflow loading use that same connected Cards. A view outliving the parent keeps the deferred owner alive, and a view and parent first used concurrently share the connection gate and OnceLock. This is earned Python lifetime state, not a new registry/transport implementation.

### Language projections, discovery, and migrated consumers

TypeScript keeps private constructors. All four formerly synchronous HTTP factories now await the existing native trace-spawn bridge and shared `connect_with`, carrying the existing typed Wyrd error envelope back to `nativeHandle`. The native future owns cloned native state before suspension; it does not retain an Env/V8 value across the await. Bifrost uses the same existing native connection result, with approved options replacing old constructor variants.

The list operation is one retained-client GET to `/v1/bifrost/tables` (`bifrost/query.rs:317–323`), projected through async and blocking facades, both Python facades, and TypeScript. It returns the existing wire entry via existing JSON conversion, with no schema lookup loop, aggregation, cache, route, filter, or permission change. Python's existing `TableEntry` TypedDict and TypeScript's existing generated `BifrostTableEntry` re-export remain the compact contract permitted by D5.

Cumulative consumer changes migrate CLI, server setup, MCP/testing fixtures, Rust SDK journeys and executable examples to the new async factories or options. The local WyrdState rename keeps local loading before storing an explicit client. Workflow's private `Cards::new` preserves the existing external-ref resolution path without making wholly local loading eager (`workflow/mod.rs:159–172`). Those existing local-loader uses justify retaining a crate-private nonconnecting Cards constructor. The internal gRPC retry fixture now calls private `assemble`, preserving its deliberately delayed/countable mint on the send path instead of changing what that fixture tests. No new durable owner, transport implementation, auth cache, trait layer, dependency, feature, wire contract, or server subsystem is required or added.

## Complete acceptance matrix

All 33 original obligations are mapped below. PASS means source and inspected proof structure support the obligation, subject to the execution limits below. It does not claim this reviewer ran any lane. TASK-002 docsite/README work is excluded from TASK-001 results.

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001 common `connect` on all scoped handles | Shared Cards/Gateway/Principals/operators/Platform pairs; Bifrost async/blocking and language projections | Rust SDK root surface test; Python handle enumeration; TS common-form enumeration | PASS |
| REQ-002 optional client and one ambient resolver | `client.rs:75–121`; Python native lazy branches; TS clientOf/native client | Rust injected-Environment ambient story uses actual shared resolver per spec11; Python/TS environment and saved-only journeys | PASS |
| REQ-003 eager required exchange and persistent transport; bearer/cache exemption | `client.rs:130–132`; `bifrost/facade.rs:147–154`; Platform existing exchange | Shared eager/counting test; language credential-path and no-credential stories; TS unreachable exchange test | PASS |
| REQ-004 options without required client | `BifrostOptions`; Python table/client/byte limit defaults; TS option object | Nondefault options in Rust ambient helper, Python/TS ambient registration | PASS |
| REQ-005 idiomatic sync/async eager form | Rust async pairs and blocking facade; Python sync factories/AsyncBifrost.to_thread; TS awaited Promise factories | Surface references, async eager success and failure source, runtime tests claimed | PASS |
| REQ-006 Python constructors and optional arguments retained | `__new__` signatures; `_BifrostBase.__init__`; eager factories use same argument forms | Python HANDLES surface and lazy/eager stories | PASS |
| REQ-007 no constructor IO; first server use once | LazyConnection; Bifrost Binding; SharedCards/views | Unit `test_local_access_never_connects`; first-operation/lazy journeys; local-state survival journey | PASS |
| REQ-008 eager/lazy parity after successful initialization | Factories create same lazy owner and call initializing accessor once | Python real-server eager/lazy and matching-code failure stories | PASS |
| REQ-009 AsyncBifrost constructor does not block | Argument-only base constructor; eager factory connects in worker | Async constructor unit and eager/lazy AsyncBifrost ambient cases | PASS |
| REQ-010 removed Rust forms, no aliases | Removed constructors; new approved connect pair and renamed loader/describe forms | Rust compile-fail declarations; source search finds old forms only in negative proofs or unrelated owners | PASS |
| REQ-011 TS private constructor; Rust eager-only public construction | TS private constructors retained; nonconnecting Cards::new crate-private | Surface tests and source visibility | PASS |
| REQ-012 SDK API/example guidance accurate; TASK-002 prose separate | Migrated examples/API docs/stubs; native insert documentation remains stale | INVAR-R2-001 / residual FIND-4; check:examples result claimed | FAIL |
| REQ-013 architecture/language authorities describe common form | Wyrd design Client model; client patterns and language reference changes | Static authority comparison | PASS |
| REQ-014 selected retained client/cache, never per-call ambient resolution | Existing native client fields/auth Arcs; Python OnceLock; Platform session client | Rust/Python/TS retained configuration/cache/renewal stories | PASS |
| REQ-015 eager auth without probe; Bifrost owned transport | connected → existing access_token; Bifrost validate/connect/assemble sequence | Shared mock exchange-count test; real refused exchange/invalid bearer stories | PASS |
| REQ-016 auth module and no old-path alias | Rust SDK auth re-export; Python package/native auth registration | Rust compile-fail old path; Python ModuleNotFoundError and export tests | PASS |
| REQ-017 table list on every first-class Bifrost facade | query.rs list; facade/blocking; Python sync/async list; TS native/public list | Root references and three discovery journeys | PASS |
| REQ-018 compact authorized entries, separate on-demand describe | One GET/unchanged entry conversion; no describe fan-out | Real own/built-in/foreign/denied/separate describe assertions | PASS |
| REQ-019 typed existing entry projection | Existing Rust wire entry; Python TableEntry TypedDict; TS generated TableEntry interface alias per D5 | Authored/export/declaration inspection; type/codegen checks claimed | PASS |
| INV-001 capabilities, permissions, server/durable behavior preserved except list | No route/service/wire/auth permission implementation change; local getters repaired | Existing journey migrations, local-state proof, discovery/authorization assertions | PASS |
| INV-002 ambient lookup remains shared Rust-owned | WyrdClient actual resolver; no SDK environment lookup added | Same fixed Environment resolver used in Rust proof; Python/TS shared native resolution | PASS |
| INV-003 supplied/delegated clients remain authoritative | Explicit paths clone same native client/cache; Platform permitted derivation | All three delegated stories compare retained subject attribution and actor chain, preserving permission denial | PASS |
| INV-004 credential redaction | Existing SecretString/auth redacted Debug/error conversion retained; no new secret rendering | Shared eager test excludes secret from debug error; static new-string inspection | PASS |
| INV-005 earned runtime boundaries only | Python LazyConnection/SharedCards/Binding; existing shared runtime; thin N-API await/convert | Owner/caller traces above; manifests unchanged | PASS |
| AC-001 public common/removed surfaces enumerated | Rust root references and compile-fail items; Python/TS enumeration | Existing exact surface test owners; no execution here | PASS |
| AC-002 real environment/saved-only calls, options, Python lazy | Rust actual shared injected resolver and all tenant factories; Platform existing environment branch per spec10/11; Python eager AsyncBifrost included | Rust ambient story, Python ambient and lazy stories, TS ambient story | PASS |
| AC-003 delegated server-observed subject identity | Existing server retained audit queried for subject and delegation_chain actor | Rust/Python/TS delegated assertions inspect subject-filtered retained decisions and compare actor IDs | PASS |
| AC-004 missing credentials/error timing, rejected exchange vs bearer, lazy parity | Native eager and lazy paths share existing errors; no extra bearer validation | Rust approved injected no-credential resolver; shared mock tests; language no-credential/forged/invalid-bearer stories | PASS |
| AC-005 no-network Bifrost/AsyncBifrost constructors | Native constructor stores only arguments; public constructor only calls it | Python no_credentials constructor and local-access tests point at unreachable endpoint | PASS |
| AC-006 TASK-001 docs, stubs, generated declarations | Stubs/declarations/source match; native insert timing/error prose still unclosed | INVAR-R2-001; recorded lint/type/codegen/N-API/example claims; TASK-002 docs lane excluded | FAIL |
| AC-007 once-selected ambient client, cache reuse and renewal | Retained client/auth owner unchanged; no clear/reset per operation | Rust saved-file mutation/renewal story; Python lazy+explicit and TS ambient+explicit mutation stories; existing saved-user renewal tests | PASS |
| AC-008 own/built-in/foreign/denied/list-then-describe journey | Existing query client and server route projected | All three new discovery stories assert own and built-in, omit foreign, assert stable denied code, describe declared fields | PASS |
| AC-009 exact list names/types/compact entry/no catalog compatibility | Async/blocking and Python/TS methods are argument-free; existing entry shape; no schema fields or extra catalog APIs | Surface refs, typed declarations and actual list call source | PASS |

## Prior finding closure

| Stable prior ID | Reassessment | Source/proof basis |
|---|---|---|
| FIND-TASK-001-1 | CLOSED | Native Binding local getter/setter and zero counters avoid initialization; SharedCards views share deferred state. Unit local access and real first-connection binding/view survival tests now assert the correct boundary. No synchronous insert queue redesign required. |
| FIND-TASK-001-2 | CLOSED under approved spec11 | `from_global` directly invokes `from_environment(Process)`; Rust journey no longer implements a private resolver. Tenant environment and saved-only cases, missing credentials and retention use that actual resolver. Python ambient test now awaits eager AsyncBifrost.connect and performs list in both ambient cases. |
| FIND-TASK-001-3 | CLOSED | Rust delegated journey filters retained audit by exact subject and compares actor chain; Python/TS SQL reads filter the subject and assert exact actor IDs/counts after existing flush/retention facilities. Denial checks remain. |
| FIND-TASK-001-4 | PARTIALLY CLOSED; INVAR-R2-001 remains | State stale links corrected; client helper/test and SDK fixture panic docs added; CLI/MCP eager error docs corrected; already-fallible author/investigation helpers propagate errors; public Python insertion timing corrected. Native insert's unconditional nonblocking/error prose is still unchanged. |
| FIND-TASK-001-5 | CLOSED | `saved_user_auth.rs:207–224` keeps real revoke/expiry setup and asserts the eager `cards` error with SavedLoginUnusable; it no longer expects successful connection or reaches an obsolete list step. Managed identity focused run is claimed. Sibling successful/stale-login assertions remain. |
| FIND-TASK-001-6 | CLOSED by approved spec10 | Saved-only applicability now explicitly covers tenant handles; Platform uses existing environment credential and no-credential proof under spec11's resolver mechanics. No saved platform auth source or variant weakening was added. |

## Non-goals and existing-owner reuse assessment

- Local loaders preserve eager local parsing and deferred client/server behavior; only approved Rust names and internal nonconnecting Cards assembly change.
- Existing GlobalConfig/ClientConfig/auth middleware continue to own precedence, identity, exchange, refresh and refusal re-exchange. The new injected resolver method moves the actual shared composition into one place under revision 11; it is not another SDK resolver.
- LazyConnection earns retained state because Python alone adds this lifecycle; OnceLock and a short initialization mutex solve it without a new runtime or transport. SharedCards earns the Arc because independent Python views outlive the parent. Binding earns local state because no native Bifrost exists until actual server use; after publication, the existing native active-table owner is authoritative.
- BifrostOptions replaces requested constructor variants with existing queue/table values. Listing extends existing QueryClient and conversions instead of introducing a catalog/aggregation owner. Existing WriterPool, ClientScope, native active table, query lifecycle and auth remain intact.
- No manifests/lockfiles, dependency/feature boundaries, server routes/wire/error catalogs, permissions/audit behavior, storage state, Platform language projections, Python root exports, UI, or TASK-002 authored docsite/README guidance are changed by the executable work reviewed.

## Open Questions

None affecting this bounded correction. The prior expensive-to-reverse applicability/proof decisions are resolved in approved revisions 10/11.

## Verification Notes and limits

Inspected exact test bodies, selectors and owning mise tasks. The task appendix claims six Rust instantiation journeys, shared-client library checks, four CLI pg cases, Python unit/integration instantiation checks, six TypeScript instantiation journeys, the managed revoked-login identity proof, formatting/lint/type checks and broken-link documentation validation. Earlier evidence claims the broader required scoped lanes. No raw execution transcript was supplied, and this reviewer did not reproduce any pass claim.

The new binding transfer/concurrent first-use behavior is source-traced; the inspected focused tests do not force a delayed first connection concurrent with table mutation or parent/view first use. No concrete race was established, so this is a residual verification limit rather than a demand for a new harness or another finding. Cancellation behavior remains the existing to_thread/native lifecycle behavior and is not redesigned here.

HEAD was rechecked as `07419b4d060303959384b682ca88e8673c722e63`; tracked source diff from that candidate was empty before writing this report. Other new files in the assigned review directory belong to the shared ongoing review and were not read as conclusions.

Skipped execution, TASK-002 prose acceptance, and unrelated hardening. Remaining material item is the residual native insertion rustdoc in FIND-4; runtime confidence remains bounded by the static-only review and supplied verification claims.
