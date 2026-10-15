# Independent invariant review

Overall result: **PASS**. No material finding proposed. This is the independent
source/acceptance review; the orchestrator must reconcile fresh verification
separately before issuing the task verdict.

## Immutable subject and authority

- Original base: `dac4c340f835676adc29f3dcc01f98e1d3d51465`.
- Candidate: `d088519ea846aba8b2a77c24b2af2491ad2b3936`.
- Previous candidate: `295446005b18604030bf376de6209098a28886f7`.
- Authorized main merge: `1b769759dfd92a82e60082fe80460166bdd860f8`, importing
  `cb29781ab6f633700f9e47befddb1f325fd8b732`.
- Approved authority: `changes/active/agents-trace-processor/spec.md` r2,
  original TASK-001, both historical remediation packets and their validation
  ledgers. Historical conclusions were treated as hypotheses.
- Repository authority: AGENTS.md, agent rules, active Wyrd design (client,
  runtime identity, observation identity and provider/lifecycle sections),
  doctrine, applicable Bifrost ingest/query boundaries, reference router,
  spec-driven development, maintainer style, PyO3/Python/TypeScript,
  testing/error and telemetry references, docs CONTRIBUTING/PRODUCT.

CodeGraph was used before source navigation. The cumulative original range,
its changed-file map, task-versus-imported-main contrast and latest remediation
diff guided inspection. I read no current peer discovery report and ran no
install, build or test lane. HEAD was checked at entry and after source review;
both were the candidate above. Only this report is authored by this reviewer.

## Values, authority and lifecycle traced

**Identity producer to sink.** `WyrdState` keeps its existing shared client's
lazy selection; Python setup obtains `self.inner.client()` and TS obtains
`#native.client()`. The managed provider retains an exporter configured with
that client, not a copied bearer. Python `_session.authenticate` and TS
`otel.ts::config` request its current access token on every export. Existing
`http_auth`/`httpAuth` remain the distinct stock gateway transport. Framework
direct-provider keys do not pass through either OTLP helper.

Run scope produces only optional correlation: Python `_enter_run`/
`_exit_run`, `_scope_stack` and `_RunCorrelationProcessor.on_start`, TS
`Run.scope` and `runCorrelation.onStart`. The bridge selects an existing
framework parent or current OTEL context at start. Its `_open`/`#open` stores
the actual OTEL span, so completion reads final metadata without selecting a
new Run. Python's lock covers duplicate-start detection and publication;
ending removes one mapping, shutdown clears retained mappings and ends each
unfinished span with a constant status. TS callback bodies do not interleave
across an await. A nonrecording OTEL span remains nonrecording.

At the imported-main seam, `gate/auth.rs::AuthContext::attribution` uses
`wyrd_runtime::delegated_attribution` (`request_context.rs:60`): the verified
subject remains the principal, the last verified actor is selected, and a
direct publisher acts for itself. Gate canonical and native dispatch pass that
pair independently of client correlation. `gate/attribution.rs::CardRegistry`
resolves only unbound writers' asserted Cards through a tenant connection;
bounded writers retain signed scope. Scribe
`execution_lanes.rs::authorized_card_uids:1011` validates every non-null Card
before stamping. `stamp_correlation_columns:1085` and `managed_arrays:1225`
preserve null optional correlation and stamp non-null subject/actor values
from trusted context. The shared managed-column policy includes both publisher
columns; audit's None policy preserves its server-authored content instead of
overwriting it. Neither SDK adds another principal/tenant resolver.

**Provider producer to consumers.** Python `PyWyrdState::start_telemetry`
(`state/mod.rs:191-219`) serializes initialization with the installed PyO3
`MutexExt::lock_py_attached`, rechecks the existing OnceLock under that gate,
and publishes only a successful handle. Waiting detaches from Python; the
gate owns no Python data. `__traverse__` and `__clear__` continue to visit/take
the OnceLock and do not acquire this gate. Failed setup can retry. A separate
state still loses global registration and `_start_telemetry` shuts its loser
down. TS's post-import recheck precedes the synchronous install and assignment,
closing same-state overlap without another provider registry.

Python's `_ManagedTelemetry.force_flush` (`otel.py:238-263`) now snapshots the
started-operation count at entry and uses one monotonic deadline. An already
running operation cannot produce this call's success merely by settling late:
the call must observe an operation started after entry. Waiting for an older
operation and waiting for the covering operation spend the same budget. Only
one pending Future/worker exists. No worker join, exporter queue, retry policy,
or provider shutdown is added. Both public consumers, `flush` and `shutdown`
(`state/mod.rs:295-320`), capture the OTEL outcome before attempting the
existing native drain. `flush_telemetry:525` classifies False/TimeoutError and
type-only failures, and shared `TelemetryFlushFailure::settle:2156` retains
OTEL precedence and the native stable code. TS `#flushTelemetry`, `#settle`,
`flush` and `shutdown` implement the same order using the stock JS provider's
configured force-flush timeout. Application-owned providers never enter either
state's retained-provider field.

## Complete obligation matrix

Verification below names inspected assertions and committed evidence, not new
executions by this reviewer. Original TASK-001 and R1/R2 evidence contain the
exact commands/counts; current `verification.md` additionally records the root's
fresh 16 Python focused cases, five TS units, clean package/documented-import
checks, 13 Python framework journeys and five TS framework journeys, all passing.

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-010 explicit optional OpenAI bridge, faithful and safe callbacks | Python `otel.py:354-509`; TS `index.ts:2493-2715`: structural inputs, start-time spans, final allowlisted attributes, duplicate/end/shutdown handling | Python `test_agents_callback_lifecycle`, TS `agents callback lifecycle`; real OpenAI branches of J/K journeys | PASS |
| REQ-011 generic managed/owned instrumentation; optional correlation | Existing setup/exporters and correlation processors; native framework builders in Python `frameworks/app.py:150-229`; generic unscoped spans need no Run | All-framework managed/owned and uncorrelated edge assertions; TS equivalent | PASS |
| REQ-012 ownership, idempotence, retry and sampling | Python setup gate plus OnceLock and loser shutdown; retry-safe install marker; TS post-import recheck, public construction-time correlation factory | Python setup repeat/overlap/other-state/application/lost-race and missing-extra retry; owned-provider unit/journeys; TS setup/owned cases | PASS |
| REQ-013 bounded managed flush before native drain; error precedence and provider survival | Python managed operation coordination and both binding consumers; TS lifecycle methods; shared failure settlement/catalog | Python real-stock deadline/stale-operation cases and lifecycle parametrization; TS failure cases; live managed journeys in both runtimes | PASS |
| REQ-014 OTEL fidelity, context, sampling and content | Stock OTLP exporters; context-only enrichment; bridge metadata allowlist; native controls in recipes/builders | Owned correlation/resource/sampling tests, explicit-context thread regression, real nested/concurrent/stream/error/privacy journeys | PASS |
| REQ-015 executable docsite and lasting authority | Observe install/construction/readback/owned-provider sections; Connect/Understand/Quickstart links; `wyrd-design.md:696-708`; draft retained | R2 exact-dependency import probe and docs generate/check claims; recipe objects matched to actual builders and entrypoint imports | PASS |
| REQ-016 real framework evidence and selection | Actual framework agents, local scripted HTTP providers, real server fixtures and SDK readback; explicit owner/Bifrost selection in mise | Python J1-J4 each parameterize all three frameworks, J5 gateway; TS K1-K5; live first read precedes shutdown | PASS |
| REQ-017 existing ambient auth only | One selected shared client for state/exporter; existing stock-client auth for gateway; isolated test config; no bearer copied at setup | Exporter freshness tests, existing gateway transport tests, Python/TS gateway telemetry and no-credential assertions | PASS |
| INV-001 authenticated tenant/publisher; authorized optional Card | Gate verified attribution and dispatch, Scribe Card authorization and server stamping; shared client identity unchanged | Viewer/other-tenant framework journeys; imported SDK protected-publisher coverage; producer-to-sink inspection above | PASS |
| INV-002 excluded protocols/runtimes/queues/adapters; Rust/log/metric preservation | Task-owned production changes stay in SDK glue, two approved errors and shared error projection; stock log/metric factories remain | Diff/manifest/lane audit; generic no-peer package check; existing signal/Rust lanes remain selected | PASS |
| INV-003 enrichment does not suppress application errors or copy payloads | Run scope failure handling; start-time identity and allowlisted final bridge data; fixed interruption and type-only lifecycle errors | Existing failure-enrichment/exit tests, callback secret sentinels, native content/privacy journeys and model/tool error execution | PASS |
| INV-004 explicit provider ownership; no adoption or Run lifecycle | Retained provider is only one created by that state's setup; Run scope changes context only | Application-provider-never-flushed test, owned journeys, same/other-owner startup cases, active/global survival checks | PASS |
| AC-001 real Python three-framework and TS OpenAI round trips | Python framework builders and readback; TS actual Agents/OpenAI/tool runner and SDK timeline query | Committed positive J1/J2 and K1/K2 counts; inspected two-model/one-tool and ancestry assertions | PASS |
| AC-003 relevant checks and lane closure | Public stubs, generated catalog/native projections, package checker and explicit mise selection; dev framework dependencies | TASK/R1/R2 recorded format/lint/types/codegen/deps/package/docs and focused results; no fresh claims here | PASS |
| AC-007 bridge timing/hierarchy/model/usage/status/start identity and optional imports | Both callback owners; Python direct Responses usage with per-counter fallback; TS consumed-field declarations | Callback tests, installed-framework type assignability, root no-peer noEmit/runtime checker, real journeys | PASS |
| AC-008 managed idempotence and owned-provider preservation | Python attached mutex/OnceLock; TS post-await recheck; existing stock provider construction/composition | Python overlapping setup and retry, TS overlapping setup; resource/sampler/other-exporter correlation tests and both owned journeys | PASS |
| AC-009 live visibility, drain on failure, active/global survival | Both lifecycle consumers preserve native drain after OTEL result and leave provider open | Python parent queries report before stdin continuation; TS queries after state.flush before second work; held-span/post-shutdown export plus runtime-local failure tests | PASS |
| AC-010 uncorrelated/nested/concurrent/enclosing/error/stream fidelity | OTEL-native context propagation and retained started span; framework edges use actual model/tool execution | Python/TS edge journeys; Python sibling Card scope unit tests; exact identities and root ancestry in successful turns | PASS |
| AC-011 sampling/privacy/denied/tenant boundaries | Native content switches, bridge allowlist, stock sampling, existing server authorization | Three Python sampling/privacy/auth cases and TS corresponding journey; denied export reads empty; other tenant cannot persist this Card's spans | PASS |
| AC-012 usable recipes and agreeing authority/machine docs | All four recipes define tool/agent/provider or runner/session/message, use ambient Wyrd state and authenticated readback; every TS otel direct peer is in command | Source recipe/import reconciliation; R2 clean documented-dependency module-load and docs checks | PASS |
| AC-013 ambient/gateway/refusal/freshness reuse | Existing shared token owner, Python HttpAuth and TS httpAuth; separate external-provider key | J5/K5 assert local upstream sees provider authorization and no Wyrd headers; unit export freshness and no-credential errors | PASS |

## Constraints, non-goals and complexity check

| Constraint/non-goal | Assessment |
|---|---|
| No new server route, durable payload, table, migration, Card kind or authorization subsystem for this task | PASS. The task contrast against imported main adds none. Main's separately authorized attribution changes are reviewed at affected seams, not mislabeled task drift. |
| No framework discovery, monkey-patching, registry, universal instrument-all Wyrd API, Pydantic/ADK adapter or Rust framework adapter | PASS. Framework setup is application code using stock APIs; only explicit OpenAI callback translation remains SDK glue. |
| No replacement transport, export queue, retry policy/token cache or public timeout option | PASS. Standard OTLP exporters retain batching/retry. Future/thread coordination bounds waiting at the existing lifecycle owner and stores only one in-flight attempt. The operation counter closes a demonstrated freshness hole rather than adding delivery machinery. |
| Preserve native startup/closed-state errors and native writer lifecycle | PASS. Both runtimes always call their existing native operation; error settlement retains a native-only error unchanged and otherwise records its code. No native drain rewrite appears. |
| Preserve global shutdown compatibility and application-owned configuration | PASS. State shutdown flushes its managed provider and closes only the native writer. Explicit owned setup retains native provider configuration/lifecycle. |
| Optional production frameworks/dependencies; owner-directed Python floor/dev selection | PASS. Agents extras/optional TS peer do not impose root import/declaration edges. Ordinary development framework installation and Python >=3.11/abi3 alignment are recorded owner-directed choices, not an independently invented production framework dependency. |
| No secrets/raw payloads in newly projected metadata or errors | PASS. Bridge field allowlists exclude content and raw errors; stock native content remains subject to documented upstream controls and server permissions. |
| No manufactured fixes by skip/ignore/weakened checking; no draft publication | PASS. Required frameworks are positively selected; removal of the separate MCP lane accompanies ordinary integration collection. Observe keeps its draft flag. |

The existing owners are sufficient. Python's `_ManagedTelemetry` earns its
retained Future and lock because the pinned synchronous stock flush cannot
honor the required caller deadline by itself; replacing callbacks with an
executor would change correlation/availability. The R2 counter is the minimum
additional state needed to distinguish an older drain from a call-covering
drain, with no retry backlog. The Python setup gate reuses PyO3's installed
GIL-aware mutex mechanism and preserves GC ownership. TS correlation exposes
the existing stateless processor, and the package checker extends the current
clean consumer rather than creating another harness. No deletable replacement
workflow or optional feature was found.

## Prior finding closure

| Stable ID | Independent current disposition |
|---|---|
| FIND-TASK-001-1 optional TS declaration edge | CLOSED: consumed structural types at `index.ts:2493-2531` remove the framework declaration dependency; checker compiles root APIs and processor without optional peers and with declaration checking enabled. |
| FIND-TASK-001-2 complete executable recipes | CLOSED: all execution objects are constructed, and Observe's TS install row now includes logs and metrics exporters required by the shared OTEL module. No module redesign needed. |
| FIND-TASK-001-3 missing live-flush proof | CLOSED: Python first readback occurs while child is waiting after public flush; TS readback precedes second work and shutdown. Active spans, writer continuation and later global export are separate assertions. |
| FIND-TASK-001-4 safe Responses counters | CLOSED: direct usage is read even with response=None; per-counter fallback preserves zero and does not invent a model/provider. |
| FIND-TASK-001-5 Python flush deadline | CLOSED: bounded Future wait permits native drain while one daemon provider attempt remains held; ordinary synchronous callbacks remain usable. |
| FIND-TASK-001-6 overlapping TS setup | CLOSED: same-state recheck immediately after import precedes synchronous installation/publication without another await. |
| FIND-TASK-001-7 obsolete successful Python flush | CLOSED: a result is accepted only from an attempt numbered after call entry. `_stale_flush` holds the actual stock drain-to-settlement interval and requires late completed spans before either public call returns. |
| FIND-TASK-001-8 concurrent Python same-state initialization | CLOSED: check/install/publication are serialized with the attached mutex and rechecked OnceLock; GC traversal remains lock-free over retained Python ownership. |

## Verification and coverage limits

The cumulative range contains 288 files because it includes the authorized main
merge and historical review artifacts. I inspected the original task's entire
obligation set and production/test/doc/package owners, and followed imported
publisher attribution, Card authority, managed schema, native ingestion and
SDK readback/lifecycle seams. This is not an audit of all unrelated imported
Verifier, registry, provisioning or migration behavior. No task-facing seam
regression was established.

The R2 packet records 16 focused Python cases, repeated deadline/setup runs,
612 Python unit cases, three managed framework journeys and relevant static/
docs checks. Earlier packets record the complete framework matrices, TS
package/type proof and static checks. These are evidence claims consistent
with inspected source. On final read, current `verification.md` records fresh
passing focused runtime, package/import and full framework-file proof as listed
above, with no skipped journey cases. Broad static results remain committed
implementation evidence, not fresh review executions. Integrated broad gate
selection remains change-review work as the task specifies. This reviewer did
not run tests, installs, builds, external providers, OIDC, or cloud storage.

Known bounded limitations remain explicit and are not new findings: Python's
stock exporter can log a refused export while its provider reports successful
flush; TS Chat Completions 0.20 supplies no generation usage; a TS Run does
not restamp an already-active enclosing span; timeout does not cancel export;
abrupt process loss has no new delivery promise. Historical missing RED runs
are not retroactively fabricated.

No proposed findings; no corrective code or additional mechanism requested.

Final HEAD guard: `d088519ea846aba8b2a77c24b2af2491ad2b3936`, unchanged.

Skipped: own test execution and unrelated imported-main audits. Risk: broad
integrated aggregate closure remains the subsequent change review's responsibility.
