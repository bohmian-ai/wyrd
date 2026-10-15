# TASK-001 / R3 independent behavior acceptance review

Result: **PASS**. Proposed material findings: **none**.

## Subject and method

Reviewed original base `dac4c340f835676adc29f3dcc01f98e1d3d51465` to
candidate `1cf675c58d01cba0e714102c75cb7d89e7a678b5`, under approved
`changes/active/agents-trace-processor/spec.md` r2 and original
`tasks/TASK-001-framework-agnostic-telemetry.md`. The R1/R2/R3 packets, prior
verdicts and validated finding history were inputs to closure hypotheses, not
substitutes for current source. The latest `d088519ea..1cf675c58` patch located
the remediation; it did not narrow acceptance. The authorized main merge
`1b769759dfd92a82e60082fe80460166bdd860f8` and imported
`cb29781ab6f633700f9e47befddb1f325fd8b732` were distinguished from task changes
using the supplemental contrast, without replacing the original base.

CodeGraph exploration preceded source navigation. Expanded its results through
Python and TS state setup/lifecycle and callback callers, stock exporters,
Run correlation, real framework builders and query consumers, the shared error
owner, Gate's three signal paths, trusted attribution producer, Scribe logical
identity/fence consumers and the existing outbox/native siblings. Read actual
source and cumulative diffs; no current peer report or implementation-chat
conclusion was consulted. No build, test, installation, index or source edit was
performed. This report is the only owned output.

Governing inputs: AGENTS.md, agent rules, reference router, spec-driven
development and maintainer style; active Wyrd design/provider/observation
identity sections, doctrine and applicable Bifrost identity/ingest authority;
telemetry observations, testing workflows, Python API/stub and TS guidance;
docs CONTRIBUTING/PRODUCT. Stronger repository and architecture authority was
applied before private task mechanics. Owner-authorized Python floor/dev-group
changes and main integration were not treated as unsolicited drift.

## Caller-to-result and existing-owner proof

| Path / owner | Source inspected and observed behavior | Existing alternative / reuse assessment |
|---|---|---|
| Python managed setup | `src/state/mod.rs::start_telemetry` uses the state's shared client, per-state GIL-aware gate and existing OnceLock; `otel.py::_start_telemetry` refuses an installed provider, releases the losing provider and returns the managed handle only after the global install wins | Existing setup/exporter and native client resolution are extended. Neither a separate global registry nor framework auth path is added. `lock_py_attached` is the installed PyO3 mechanism for this interpreter boundary |
| TS managed setup | `index.ts::WyrdState.startTelemetry` checks before and after the import await, then installs and publishes synchronously; `otel.ts::startTelemetry` releases a refused provider | Existing owner suffices; the second same-state check closes the actual suspension seam, with no initialization Promise registry |
| Owned providers / Run | Python `install_run_correlation`, `_RunCorrelationProcessor`, `_enter_run`, `_exit_run`; TS `runCorrelationProcessor`, shared `runCorrelation`, `Run.scope` | Stock context and processors retain sampling/resource/other-exporter ownership. Python marker rolls back when registration fails; TS construction-time factory returns the existing stateless processor. Run stores correlation, not lifecycle |
| Managed lifecycle | Python `flush/shutdown -> flush_telemetry -> _ManagedTelemetry.force_flush`; TS `flush/shutdown -> #flushTelemetry -> #settle`; both reach existing native state drain regardless of provider outcome | Native Bifrost cannot flush a foreign-runtime OTEL provider. The Python handle earns retained attempt/deadline state because the pinned stock synchronous processor ignores its wait bound; it does not replace export or add an export/retry queue. TS retains the stock configured provider bound. Shared `TelemetryFlushFailure` owns catalog precedence once |
| OpenAI callbacks | Python and TS `WyrdAgentsTraceProcessor` start ordinary global-provider spans, store mapped open spans, choose known parent/root/current ancestry, capture current Run at start, read completion metadata, and clear interrupted work on shutdown | This is the specifically approved runtime bridge. Pydantic/ADK remain native OTEL; no broad adapter registry or schema normalization. No auto-registration; exporter/provider lifecycle stays elsewhere |
| Credentials | Python `_session` calls selected `client.access_token()` for each request; TS `config` provides per-export async headers. Gateway builders use existing `http_auth`/`httpAuth`; direct model builders use independent provider credentials | Shared Rust client token/refresh and ambient resolution remain authoritative. No setup bearer copy, second refresh/cache, token tenant selection or per-framework auth helper |
| Real execution / query | Python `frameworks/app.py` builds Pydantic `OpenAIChatModel`, ADK `Gemini`/`InMemoryRunner`, OpenAI stock Chat Completions model; TS journey builds stock OpenAI model. Local HTTP fixtures return real tool/stream/usage responses. SDK `trace_timeline`/`traceTimeline` and drift queries read server results | Existing WyrdTestServer/Postgres and SDK/query fixtures reused. Application enclosing spans are not substitutes for framework model spans: model/tool operation counts and metadata are asserted separately |
| OTLP publisher identity | `AuthContext::attribution -> delegated_attribution` selects verified subject and current actor. `gate/mod.rs:133-154::otlp_batch_id` binds tenant, both IDs, table, logical digest and accepted correlation digest; `dispatch_canonical:959-1003` sends that trusted pair to Scribe | The existing implicit-ID owner is corrected once. All three decoded signal methods call it. `LogicalIdentity`/`correlation_data_identity` remain the existing digest owners; Scribe's managed-column exclusion and memory/durable replay fences are unchanged. Native supplied batch IDs and outbox IDs remain separate existing owners; outbox already binds the pair |
| Rust attribution proof | `negative.rs:386-548` uses real delegation, exports identical content across HTTP/gRPC and groups SDK query rows by subject/actor/Run; each group must have every expected marker exactly once | Existing OtlpJourney, public token/delegation helpers and query forwarding reused. The seven-file OTLP sibling sweep finds no obsolete row read of `principal_id`; other occurrences of principal identity in auth are not removed schema fields |

The actor correction does not trust an OTLP attribute as publisher. Authorization
still happens before projection/dispatch. HTTP and gRPC feed the same canonical
owner, as do trace/log/metric projections; request ID, refreshed token bytes,
wall-clock metadata and historic delegation hops are not identity inputs. The
native path still retains caller-supplied batch identity and first-acceptance
semantics. No compatibility replay mode, new durable guarantee or unrelated
delivery redesign is inferred from this correction.

## Complete acceptance matrix

Verification below means inspected test assertions plus committed historical
execution records and the explicitly scoped root-run evidence recorded below.
This reviewer performed no execution. `U`/`T`/`J`/`K` names refer
to the original task's exact selectors, with Python J paths moved into the
existing `otel/frameworks` owner as recorded in the packet.

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-010 bridge fidelity and safe callbacks | Both processor bodies preserve provider-generated IDs and known hierarchy, valid timestamp conversion/fallback, late allowlisted GenAI identity/usage/error, start-time context; duplicate live start/end and shutdown handling; Python lock | U4/T4 inspect actual parent IDs/timing/late data/unknown spans/secret exclusion/interruption; Python thread test; real OpenAI J/K paths | PASS |
| REQ-011 generic native instrumentation and optional exact correlation | Existing stock exporters and state setup; native Pydantic/ADK builders, explicit OpenAI bridge; generic no-Run path; current trusted publisher pair now bound in Gate identity | J1/J2 parameterize three actual Python frameworks; K1/K2 actual OpenAI; J3/K3 uncorrelated; Rust publisher readback extension | PASS |
| REQ-012 idempotency, refusal, owned-provider preservation and sampling | Python gate/OnceLock plus installed-provider check and losing resource cleanup; TS postawait recheck/refusal cleanup; owned construction paths preserve application configuration | Python setup overlap/repeat/application/other/lost-race and retry cases; T1/T2; owned provider correlation and J2/K2 | PASS |
| REQ-013 bounded flush then native drain, honest failure and preserved provider | Python covering Future attempt within one call deadline; TS stock bound; both state methods always attempt native stage; shared catalog error precedence; no provider shutdown in state methods | U3/T3 false/timeout/failure/native-only/dual-stage cases; real stock held-export and stale-result regression; J1/K1 live readback, held span and postshutdown export | PASS |
| REQ-014 standard fidelity/context/sampling and qualified content policy | Native OTLP retained; correlation processor adds only Run/Card. Bridge allowlist omits content. Docs qualify upstream content controls and tested versions | Owned resources/sampler and nested/concurrent/context regressions; J3/J4/K3/K4; existing OTLP fidelity owner tests retained | PASS |
| REQ-015 complete docs and lasting authority | Observe contains all four concrete framework recipes, install prerequisites, owned setup, ambient readback, lifecycle errors, sampling/privacy/add-vs-replace/version limits. Connect/Understand links and lasting design updated; draft remains true | Procedure compared to actual builders/journeys; historical docs generation/check and dependency-import closure records | PASS |
| REQ-016 real framework evidence and lane closure | Real models/tools/local endpoints and real server/managed Postgres; Python directory and TS file included in Bifrost lane; standard Python integration collection includes frameworks installed in dev | J1–J5/K1–K5 selectors cover provider paths, live flush, error/stream/scope/sampling/authorization; historical positive collection counts and runs; runtime-local injections confined to allowed cases | PASS |
| REQ-017 merged ambient/shared-client/stock-auth reuse | Selected state client shared with exporter; direct provider key separate; gateway stock auth reused; Python public code exposed at existing subprocess boundary without resolver duplication | U5/T5 token freshness; shared transport regressions retained; J5/K5 local upstream isolation and exact missing-credential code. Python parent now compares exact structured JSON | PASS |
| INV-001 authentication owns publisher/tenant; Card claims authorized | Gate uses verified Attribution for both ID and Scribe frame; tenant from auth, Card projection scope unchanged; query tenant owner retained | Viewer/foreign-tenant framework negatives; new delegated/direct replay readback; same-pair unit convergence | PASS |
| INV-002 excluded machinery and preserved Rust/log/metric support | No new runtime/discovery/patching/adapter/transport/export queue/durable protocol. Existing shared Gate correction covers all signal siblings; native explicit-ID and log/metric exporters retained | Cumulative diff/owner audit; existing OTLP sibling sources; updated Rust publisher query uses current schema without alias | PASS |
| INV-003 optional enrichment preserves results/exceptions and payload safety | Correlation hooks swallow enrichment failures; Run exit does not suppress; bridge maps only metadata/type/status; lifecycle error cause excludes raw messages | Existing enrichment/Run exception tests; callback secret sentinels, safe Responses usage, actual tool/model failures/privacy journeys | PASS |
| INV-004 explicit lifecycle ownership | Setup never adopts another provider. Managed state retains only its own provider; owned provider application-flushed/shutdown; Run creates/ends/flushes no span | U1/T1 refusal; owned-never-flushed unit; J2/K2; active/global continuation J1/K1 | PASS |
| AC-001 real Python3/TS OpenAI model/tool export and readback | Actual framework builders, stock model clients and SDK query paths | J1/J2 three-framework matrix and K1/K2; exact model/tool count and ancestry assertions | PASS |
| AC-003 relevant checks, artifacts, lanes and docs | Catalog source projection, native error helper declarations, strict generic package checker, framework dependencies/selection; settlement test now has Panics contract | Historical task/R1/R2/R3 static/type/package/docs/format and focused records; root owns current verification. No source-established remaining red proof | PASS |
| AC-007 bridge fidelity/start identity/optional import | Processor implementations and structural TS root input types; Python safe direct Responses counters retained | Callback/thread/optional-import cases; packed generic consumer checker and actual framework journeys | PASS |
| AC-008 repeat setup and owned settings/export | Same-state Python/TS synchronization seams closed; refused provider preserved; public correlation installation | Setup/retry/resource/sampling units plus owned real readback | PASS |
| AC-009 live flush visibility/failure/drain/active-global survival | Managed path exports before native stage, keeps native writer open on flush, closes it on shutdown, never closes installed global provider | Python parent reads first report while child waits before continuing; TS first read precedes second work; held span and postshutdown turn; U3/T3/deadline regression | PASS |
| AC-010 no-Run/nested/concurrent/enclosing/error/stream attribution without duplicates | Native contexts and open-span maps retain start identity; verified publisher pair prevents distinct actors collapsing at implicit fence | J3/K3 exact Run assignment; J1/K1 ancestry and logical counts; callback changes-of-scope; Rust five publisher/Run groups and exact marker sets | PASS |
| AC-011 sampling/content/denied persistence/tenant isolation | Stock samplers, upstream controls, metadata allowlist, unchanged server authorization | J4/K4 unsampled and denied empty queries, foreign Card rejection and tenant-specific readback, secret absence across full SpanRecords | PASS |
| AC-012 usable current docsite and coherent machine/architecture docs | Concrete constructors, complete direct-import dependency prerequisites, current tabs/links/frontmatter/draft, query/readback and lifecycle caveats | Recipe-to-builder inspection and historical generated/docs proof; no hand-authored generated workaround | PASS |
| AC-013 ambient gateway, credential refusal and exporter freshness | Existing shared ambient client and stock auth integration; public child WyrdError handling at module boundary | J5/K5 real gateway/readback/upstream-key checks; Python exact `WYRD_CLIENT_401_NO_CREDENTIALS` JSON; U5/T5 per-export tokens and existing transport regressions | PASS |

## Constraints, non-goals and regression boundaries

| Constraint / non-goal | Evidence | Result |
|---|---|---|
| No Wyrd runtime adoption or manually authored framework internals | Real framework runs emit asserted model/tool spans; only enclosing application spans are authored in proof | PASS |
| No Pydantic/ADK or Rust framework adapter | Native Python instrumentation is configured in recipes/test apps; Rust generic OTLP remains existing surface | PASS |
| No route/table/migration/Card kind/new durable payload | Task production extension is runtime glue, approved catalog errors/factory and existing Gate identity correction; authorized main schema integration distinguished | PASS |
| No bespoke transport/retry queue/token cache/auth helper | Stock OTLP exporters and existing client/auth owners remain; managed Python Future tracks flush completion only | PASS |
| Preserve global provider after state shutdown; application-owned lifecycle retained | State paths never invoke provider shutdown; bridge shutdown ends mapped work only | PASS |
| No universal redaction or capture despite sampling | Docs disclose upstream residual tool definitions/schema and sampler drops; native path retains upstream output | PASS |
| Preserve optional framework boundary and generic consumer | Runtime optional imports/structural TS declarations, optional peers and Python extra; dev test dependencies do not force production frameworks | PASS |
| Preserve publication scope and source-driven docs | Existing draft flags retained; generators remain existing owners | PASS |
| No new server permission model or native batch retry semantics | Gate still authorizes before dispatch and forwards verified attribution; Scribe fences and native supplied-ID producer unchanged | PASS |
| No developer credential/config use in test proof | Isolated config homes, test-managed keys, ambient competing credential cleanup, no live provider/OIDC credentials | PASS |

## Prior-finding closure

| Stable finding | Current source closure |
|---|---|
| FIND-TASK-001-1 | TS `AgentsSpanData`/`AgentsSpan`/`AgentsTrace` are consumed-field structural types with no optional Agents import; package checker compiles a strict clean root consumer; real SDK assignability test retained |
| FIND-TASK-001-2 | Observe now constructs each framework/ADK runner and session/tool and query readback; TS installation includes trace/log/metric exporters, context manager, core/API and trace SDK |
| FIND-TASK-001-3 | Python live app prints after flush and waits while parent queries; TS first read precedes continuation. Both exercise second native/framework work, span held across flush and global export after shutdown |
| FIND-TASK-001-4 | Python `_gen_ai` reads payload-free `ResponseSpanData.usage`, keeps zero and falls back per-counter to response usage without guessing model |
| FIND-TASK-001-5 | `_ManagedTelemetry` bounds caller waiting and retains only one in-flight stock flush; native drain remains attempted after timeout |
| FIND-TASK-001-6 | TS rechecks state after dynamic import before synchronous install/publication |
| FIND-TASK-001-7 | Python snapshots attempt count at entry; an older attempt is joined within the same deadline but cannot itself satisfy the later call; existing real-stock stale-settlement test covers flush and shutdown |
| FIND-TASK-001-8 | Python per-state `MutexExt::lock_py_attached` serializes construction with an under-gate recheck and existing OnceLock/GC ownership; failures do not poison successful retry semantics |
| FIND-TASK-001-9 | Gate hashes subject and current actor and forwards the same verified pair. Unit distinguishes `(S,S)`, `(S,A)`, `(S,B)` and same-pair convergence; real journey replays direct/delegated content across transports |
| FIND-TASK-001-10 | Shared settlement regression `state.rs:3953-3995` has substantive Panics section matching assertions/expectations; no behavior change |
| FIND-TASK-001-11 | Python app catches public `WyrdError`, prints its actual code, then reraises; parent requires nonzero exit and exactly the no-credentials code. Unexpected failures remain failures and successful gateway assertions remain |
| FIND-TASK-001-12 | Rust attribution query/extraction/rustdoc use `sub_principal_id`, `act_principal_id`, `run_id`; all seven OTLP siblings swept for obsolete reads; exact row-marker/replay/Run assertions retained |

## Proposed material findings and limits

Empty ledger: no source-supported MISSING, INCORRECT, DRIFT, VIOLATION or
REGRESSION remains under this behavioral audit. No correction or new guarantee
is proposed. Standard OTEL/platform and existing Wyrd owner comparisons above
do not establish a deletable replacement workflow. Python's documented stock
export-refusal logging behavior and TS Chat Completions' absent upstream usage
are not silently promoted into stronger task requirements.

Historical packets contain positive focused/runtime and static execution
records. Current [verification.md](verification.md) records root-run exact
derived-ID and settlement tests (one pass each), TS units (five passes), Python
focused runtime selectors (eight passes), strict clean packed generic consumer
PASS, and the real environment-backed command PASS. The latter selected and
passed the direct/delegated HTTP/gRPC Rust attribution journey, all three
managed Python frameworks plus exact ambient gateway/credential-code case
(four passes), and the managed TS OpenAI live-flush journey (one pass; four
unselected). This is now dynamic proof of finding 9/12 closure as well as
source proof, and preserves the distinction from the preceding review's red
schema query. Broad format/lint/type/generated/docs/aggregate results remain
historical, not newly executed. Final change review still
owns full capability/shared-infrastructure proof. This PASS is this independent
behavior discovery result, not the orchestrator's final verdict or permission
to merge. Candidate identity was rechecked before delivery.

Skipped own test execution and unrelated imported-main certification. Risk:
best-effort in-memory/exporter abrupt-exit loss and documented upstream capture/
flush limitations remain; no broader durability guarantee is claimed.
