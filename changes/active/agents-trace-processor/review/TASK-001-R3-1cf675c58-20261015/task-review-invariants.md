# Independent invariant review

Overall result: **PASS**. Proposed material findings: **empty**.

Subject: original TASK-001 under approved SPEC-agents-trace-processor r2,
including R1, R2, R3 and the authorized imported-main seams. Original base
`dac4c340f835676adc29f3dcc01f98e1d3d51465`; candidate
`1cf675c58d01cba0e714102c75cb7d89e7a678b5`. HEAD was checked at entry and
before writing this report and remained the candidate. This is an independent
source audit, not an implementation conclusion or a vote on peer reports.
No current peer report was read.

## Authority and scope

Applied AGENTS.md, architecture/agent-rules.md, the reference router,
spec-driven-development, maintainer-style, testing-workflows, errors,
PyO3 boundaries, telemetry-observations, Wyrd doctrine and the applicable
identity/observation/provider portions of the Wyrd and Bifrost designs. The
approved spec, original task and historical remediation/validation records
were treated in that authority order. CodeGraph was used before source
navigation for the callback, lifecycle, Gate identity, attribution and Scribe
identity owners. The original cumulative diff and the supplemental
imported-main contrast were inspected; the latter did not replace the base.

The authorized main merge is not unsolicited task drift. Its subject/actor
schema affects this task's exporter-to-Gate-to-Scribe and query assertions;
that seam is audited here. This report does not certify unrelated verifier,
workflow, SQL or platform changes imported from main.

## Producer-to-consumer invariant traces

### Runtime correlation and callback state

Python `otel.py::_enter_run/_exit_run` owns an OTEL context value holding
Card/Run pairs. Nested scopes push/pop only their own matching pair; the
correlation processor copies the current innermost pair at span start. An
already-attributed enclosing span is not overwritten by an inner Card. The
bridge starts a framework trace in current OTEL ancestry, starts each child
under the first still-known framework parent/root, and otherwise preserves
the available enclosing context. It stores actual OTEL spans rather than
manufacturing trace IDs. Completion only adds allowlisted late data and ends
the original span; it never re-reads the completion scope for identity.

The Python open-span map and closed flag share a lock. Duplicate live starts
cannot overwrite an unfinished span; end removes before enrichment; shutdown
marks closed and clears the map before ending the detached unfinished set.
Thus an end racing shutdown can finish its already-claimed span, while the
remaining mapped spans receive the fixed interruption status. Late callbacks
cannot add state. TS follows the same identity/lifetime division using its
single-runtime synchronous map operations. Its callback-facing consumed-field
interfaces avoid optional Agents declaration imports; the installed Agents
processor assignability check remains in the unit test.

Python `_gen_ai` treats safe Responses usage independently of content-bearing
`response`: direct `data.usage` wins, missing individual counters fall back to
the response when available, and zero counts survive. Unknown types receive
basic span identity/timing/status without guessed GenAI data. Python `_nanos`
and TS `agentsTime` fall back on malformed/missing timestamps. Mapping reads
no prompt, completion, tool input/output or raw error payload. Callback and
scope tests cover completion under a different Run, fallback ancestry,
unfinished cleanup, optional imports, Python threads and explicit propagated
context. Run exit itself creates, ends and flushes no span.

### One provider owner and honest lifecycle settlement

Python `PyWyrdState::start_telemetry` owns a OnceLock and the installed PyO3
GIL-aware mutex capability. It rechecks the cell after waiting; only that
state's successful setup publishes a retained provider handle. Failed import
or conflict remains retryable. `_start_telemetry` refuses an existing provider
and releases the newly built losing provider if another installer wins.
Traversal/clear and lifecycle readers use the cell without taking the setup
gate, avoiding a second lifetime owner or hidden token registry.

TS `WyrdState.startTelemetry` rechecks its retained provider after the dynamic
import, immediately before synchronous install/publication. Same-state
overlap therefore converges; distinct state/application owners remain refused.
Stock global registration retains already-registered context management and
propagation. Application-owned providers use the existing stock exporter and
the construction-time processor factory (TS) or retryable installation
(Python); their sampler, resource and exporter list remain application-owned.

Python `_ManagedTelemetry` is a bounded wait owner, not an exporter queue.
The stock provider's synchronous flush runs in at most one retained daemon
operation. Entry snapshots the started generation: a prior operation may be
waited out, but cannot alone certify spans ended by a later caller. Waiting
for that operation and the caller-covering operation shares one monotonic
deadline. The stock-held-export/stale-settlement regression tests this exact
producer-to-completion distinction for both state lifecycle methods. Native
`BifrostLifecycle` cannot flush a Python provider and is correctly retained
as the separate native drain owner.

Python state `flush/shutdown` first records provider outcome, then invokes
native drain and uses shared `TelemetryFlushFailure::settle`. TS follows the
same order through `#flushTelemetry/#settle` and the native catalog projection.
Timeout/other-provider failures remain primary; a native error retains its
stable secondary code, while native-only failure passes through. Neither
path shuts down the global provider or ends active spans. Application-owned
providers are never flushed by the state. Python OTEL's successful flush
after an exporter HTTP refusal is a stock-library behavior disclosed by the
docs; the denied-export journey proves no persistence, rather than inventing
a stronger exporter acknowledgement contract.

### Authentication, publisher identity, replay and siblings

Stock Python `_session` and TS `config` resolve the selected shared client's
access token on each export. They retain no setup bearer or second renewal
cache. State client selection remains shared native behavior. Framework model
clients using the Wyrd gateway use existing `http_auth/httpAuth`; direct
external model providers receive their independent key. The Python subprocess
negative now emits the actual public WyrdError code and re-raises; the parent
requires exact `WYRD_CLIENT_401_NO_CREDENTIALS`, preserving nonzero status and
positive upstream-key/export/readback assertions.

`AuthContext::attribution` uses shared `delegated_attribution`: the verified
principal is subject and the last verified chain step is current actor, or
subject equals actor for direct work. Gate trace/log/metric projection checks
write permission and asserted Cards before shared `dispatch_canonical`.
`otlp_batch_id` now hashes exactly that trusted subject/actor pair, tenant,
destination and accepted logical/correlation content. `dispatch_canonical`
passes the same attribution to Scribe. No Run/Card attribute selects a token,
tenant or publisher. Request/receipt values and historical actors do not
become replay inputs.

Scribe's existing `ManagedValues/managed_arrays` stamps both publisher
columns from the frame. `LogicalIdentity` and its reuse check continue to
exclude managed columns; correlation content remains separately included in
Gate's implicit OTLP ID. The correction therefore separates publishers
before the unchanged WAL/memory/durable replay fences, where the missing
actor previously caused information loss. It does not add a downstream
guard, client identity assertion, new schema or second authority resolver.

All three OTLP signals share this dispatch for both HTTP and gRPC. Native
explicit-ID ingestion remains a separate path and retains its caller ID and
first-acceptance semantics. Internal Scribe outbox identity already includes
the pair and remains its own owner; it must not be substituted for public
OTLP derived identity. The seven-file Rust OTLP capability has no remaining
obsolete `principal_id` row read in its attribution proof. `query/query_as`
remain schema-agnostic public client forwarders. Logs/metrics and other trace
siblings keep their ordinary payload queries and auth behavior.

The corrected attribution journey uses actual delegated tokens from existing
server exchange with intersected permissions. It exports identical accepted
content as two direct subjects, one correlated Run and two actors for the
same subject, replays direct/delegated work across HTTP/gRPC, then groups the
queried subject/actor/Run triple. Five groups, two direct subjects, two
delegated actors and the exact marker multiset in every group jointly detect
the prior collision and duplicate insertion. This is a real accepted-write
proof, not a wrong-audience/denied-export surrogate.

## Complete obligation matrix

Evidence names below refer to inspected checked-in tests, not new executions
by this reviewer. Historical execution records are distinguished below.

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-010 | Python `otel.py::WyrdAgentsTraceProcessor`, TS `index.ts::WyrdAgentsTraceProcessor`: explicit optional bridge, generated OTEL identity, parent fallback, start scope, late safe mapping, duplicate/closed state and payload-free interruption | Both callback-lifecycle tests; Python thread/optional-import tests; actual OpenAI journeys | PASS |
| REQ-011 | Existing state setup, exporter/correlation APIs and native framework builders; Gate derived IDs bind authenticated publisher pair independently of optional Run/Card | Managed/owned three-Python-framework and TS journeys; uncorrelated edges; corrected real attribution journey | PASS |
| REQ-012 | Python OnceLock/GIL-aware setup gate, losing-provider cleanup and retryable marker; TS post-await recheck and construction-time processor; stock sampling/resources retained | Setup overlap/refusal/retry tests, owned correlation/resource/sampler tests and owned framework journeys | PASS |
| REQ-013 | Python covering bounded operation then native drain; TS stock forceFlush bound then drain; shared catalog settlement; global/active/owned lifetimes preserved | Managed lifecycle injections, stock held/stale deadline test and both live managed journeys | PASS |
| REQ-014 | Existing OTLP projection retained; context-based optional correlation; safe bridge allowlist; upstream native content controls and explicit version qualification | Native signal regressions retained; callback/owned/edge/privacy framework tests | PASS |
| REQ-015 | Observe installation/build/execute/readback procedure, owned-provider recipe, current links/tabs/draft, Connect/Understand references and lasting design lifecycle text | Recipe-to-builder comparison; historical docs generation/check and clean import prerequisite proof | PASS |
| REQ-016 | Actual Pydantic AI/ADK/OpenAI execution with local scripted models, real server/Postgres/query, Python subprocess provider isolation and TS real Agents calls; owner/Bifrost selectors collect these files | Checked-in managed/owned/edges/privacy/gateway matrices; historical 13 Python and 5 TS full-file passes; current root proof tracked separately | PASS |
| REQ-017 | Existing ambient shared client and per-export token hooks; stock gateway auth transports; isolated config and model keys; actual error code survives subprocess | Token freshness tests, shared auth regressions, actual Python/TS gateway journeys with exact credential-negative assertions | PASS |
| INV-001 | Verified tenant/publisher pair owns auth and implicit identity; Card authorization precedes canonical dispatch; correlation changes no credentials | Denied/cross-tenant framework journeys; corrected direct/delegated attribution query | PASS |
| INV-002 | No new framework runtime/registry/adapter/export queue/durable surface; Rust and log/metric owners retained; only existing Gate implicit identity corrected | Cumulative diff/owner comparison; retained seven-file Rust OTLP capability and focused attribution proof | PASS |
| INV-003 | Context enrichment fail-open; start-time correlation; mapping/error metadata exclude payloads; explicit lifecycle errors remain actionable | Scope-failure/user-exception tests, callback secret sentinels and real privacy/model/tool failure paths | PASS |
| INV-004 | Successful state owns only its installed provider handle; existing providers refused; Run is context only; application lifecycle retained | Managed setup/owned-never-flushed tests and live post-shutdown provider export proof | PASS |
| AC-001 | Three real Python builders and actual TS Agents model/tool execution through authenticated exporter/server/client readback | Managed/owned framework journeys assert real model counts/available usage, tool and ancestry | PASS |
| AC-003 | Shared error projections/generated declarations, package optionality proof and owner/lane selection maintained; required Rust settlement Panics contract completed | Historical static/types/codegen/deps/docs lanes; R3 committed focused/static passes; fresh root verification is explicit below | PASS |
| AC-007 | Both bridges preserve known ancestry/timing/late usage/status and start identity; optional generic import surfaces remain usable | Callback lifecycle, actual ResponseSpanData safe zero/fallback case, Python threads and optional imports; TS strict packed consumer | PASS |
| AC-008 | Same-state managed idempotence, explicit conflicts and owned composition remain in existing owners | Python overlap/retry/provider tests, TS overlapping setup and owned units/journeys | PASS |
| AC-009 | State flush is a live barrier before native close; failure still drains; active span/global provider survive | Python app flush/print/wait/read/resume handshake; TS readback between turns; held span and post-shutdown turns; failure/deadline injections | PASS |
| AC-010 | Context isolation, late completion, nested restoration, uncorrelated and error/stream executions retained; publisher-pair identity prevents identical accepted exports collapsing | Framework edges plus correlation regressions; corrected Rust direct/delegated/replay marker sets | PASS |
| AC-011 | Stock sampler/native content controls respected; payload-free bridge; server permission/Card/tenant checks remain authoritative | Always-off, sentinel absence, viewer denial and cross-tenant cases in actual framework journeys | PASS |
| AC-012 | Recipes construct agents/providers/tools/runners, show complete prerequisites, execute/flush/query and state residual content/sampling/failures; draft retained | Docs source review against real builders, historical docs:generate/docs:check and dependency-import closure | PASS |
| AC-013 | Ambient setup and query use existing chain; gateway stock client uses shared auth; per-export freshness; Python negative asserts structured exact code | Both gateway journeys and exporter freshness tests; Python child catches public WyrdError only and re-raises | PASS |

## Constraints, non-goals and existing-owner comparison

| Constraint or non-goal | Source evidence and conclusion | Result |
|---|---|---|
| Preserve client/server and foreign-runtime ownership | Python/Node glue owns interpreter/provider callbacks; native drain and auth remain shared Rust; Gate/Scribe own durable admission/stamps/fences | PASS |
| No new telemetry protocol, route, table, migration, Card kind or payload | Task-specific contrast changes existing exporter/provider/lifecycle and implicit identity inputs; imported-main schema migration is authorized separately | PASS |
| No discovery, monkey-patching, proprietary schema, Pydantic/ADK adapter or Rust framework adapter | Framework builders invoke native upstream instrumentation; explicit OpenAI bridge maps standard GenAI fields; retained Rust generic OTLP | PASS |
| No bespoke exporter/retry queue/token cache/new production dependency/Cargo feature | Uses installed stock exporters and selected shared client. `_ManagedTelemetry` stores one operation and deadline state because stock synchronous flush lacks an effective waiting bound | PASS |
| No global-provider shutdown redesign or lifecycle on Run exit | State flushes installed provider and drains native; callback shutdown closes only callback map; Run only updates context | PASS |
| Preserve owned sampler/resources/context/propagation/exporters and sampling loss | No adopted provider; owned construction unchanged; global register APIs refuse replacement; nonrecording spans are not recreated | PASS |
| No universal redaction or payload authorization bypass | Native content emission remains upstream-owned and disclosed; bridge allowlist adds no raw content; server payload/query authorization retained | PASS |
| Preserve ambient auth and external-provider key isolation | Existing selected client current-token path, stock http_auth/httpAuth; isolated test config homes and separate local upstream key | PASS |
| Keep optional dependency/import and first-class-language boundaries | Python Agents/OTEL extras and TS optional peers remain optional; root TS callback declarations consume local structural fields | PASS |
| Preserve doc publication and source-driven generated artifacts | Observe draft flag retained, existing tabs/taxonomy/links used; checked-in generation proof, no hand-authored wire compatibility alias | PASS |

Existing alternatives were traced before accepting new machinery: native
Bifrost lifecycle cannot perform a foreign-runtime provider flush; stock
Python flush needs a bounded waiter but supplies the export/batch behavior;
installed PyO3 mutex support solves GIL-aware setup; TS needs only a post-await
check; the stock exporter and shared client solve authentication; existing
Gate/Scribe identity and stamp owners solve publisher separation. No new
replacement workflow or independently deletable unearned abstraction was
established.

## Prior findings and verification limits

All stable prior findings are source-closed in the current candidate:

| IDs | Source closure |
|---|---|
| 1 | Root TS callback structures no longer require optional Agents declarations; clean consumer checker compiles public root APIs without peers |
| 2 | Complete framework/provider/readback recipes and TS OTEL direct import dependency set |
| 3 | Live Python parent handshake and TS readback before continuation; native/framework continuation and post-shutdown provider export |
| 4 | Python Responses direct safe usage, individual fallback and zero preservation |
| 5 | Real deadline around retained stock-provider flush operation |
| 6 | TS same-state check after import, before synchronous publication |
| 7 | Python caller only succeeds on a flush started after its entry, sharing one deadline |
| 8 | Python installed GIL-aware setup gate with OnceLock recheck; GC/readers do not acquire it |
| 9 | Gate binds both trusted attribution members before unchanged Scribe replay fences; actor-only divergence and real accepted readback assertions |
| 10 | Shared settlement regression has substantive `# Panics`, with assertions unchanged |
| 11 | Python subprocess carries actual public code as structured output and re-raises; parent requires exact code |
| 12 | Rust attribution query/extraction/grouping uses subject/actor pair plus Run; no restored obsolete column or weakened query helper |

Committed original/R1/R2/R3 records supply historical successful framework,
static, types, package, generation, dependency and docs proof. They are not
new executions by this reviewer. Current `verification.md`, inspected again
after source review, records fresh cumulative diff-check PASS; exact Rust
derived-ID and settlement units each selected 1/passed 1; TS telemetry units
5 passed; Python runtime selectors 8 passed; and clean packed runtime/strict
consumer PASS. The managed Postgres command completed successfully: Rust
direct/delegated attribution selected 1/passed 1, Python managed three-framework
and gateway exact-code cases 4 passed, and TS managed OpenAI selected 1/passed 1.
The other four TS tests were unselected by the explicit selector, not missing
required collection. These fresh results substantiate the current source
closures and the live provider continuation path. Broad static, generation,
docs and full-framework aggregate results remain historical evidence; they
were not freshly rerun by this reviewer or claimed as such.

The implicit OTLP digest changes across this remediation. Historical
implementation evidence discloses that a replay spanning the rollout can use
the old and new derived IDs; this review does not infer a new upgrade-spanning
deduplication/migration guarantee or prescribe a compatibility registry. The
approved correction retains same-candidate same-pair replay and leaves native
explicit IDs unchanged. No process-crash-lossless delivery guarantee is added.

No material finding, required correction or optional redesign is proposed.
No source, test, build, index, dependency or HEAD mutation was performed.

Skipped own execution, broad aggregate reruns and unrelated imported-main certification; root supplied the fresh focused checks. Risk: the disclosed old/new implicit-ID rollout limit remains.
