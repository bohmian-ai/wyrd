# Independent structured Ponytail validation — R3

**PASS. The independently validated material finding ledger is empty.**
All twelve prior stable findings are closed by current source and credible
focused proof. No new FIND ID, correction, or specification revision is needed.
This is a task acceptance verdict; it does not certify final integrated gates,
authorize deployment, or claim upgrade-spanning OTLP deduplication.

## Subject, authority, and independence

Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/split/agents-trace-processor`.
Original base: `dac4c340f835676adc29f3dcc01f98e1d3d51465`.
Candidate: `1cf675c58d01cba0e714102c75cb7d89e7a678b5`.
Previous candidate: `d088519ea846aba8b2a77c24b2af2491ad2b3936`.
The cumulative original task remains the subject. The R3 delta locates changes;
the contrast with imported main `cb29781ab6f633700f9e47befddb1f325fd8b732`
distinguishes the authorized merge's task-facing seams without replacing the
original base. The merge is not unsolicited drift. Unrelated imported-main
capabilities are not independently certified here.

Governing authority is current user authorization, repository AGENTS.md and
agent rules, active Wyrd protocol/doctrine and applicable Bifrost/security
identity and lifecycle contracts, then approved `../../spec.md` r2 and original
`../../tasks/TASK-001-framework-agnostic-telemetry.md`. Applied the reference
router, spec-driven development, maintainer style, Rust, PyO3, Python typing,
TypeScript, error, testing, telemetry/analytical reliability guidance and docs
CONTRIBUTING/PRODUCT. Private remediation mechanics and implementation evidence
cannot override that authority. Owner-directed Python floor/dev dependencies
and the main merge remain authorization inputs, not retroactive review findings.

Inputs include this round's subject and complete nine-report discovery set:
behavior, invariants, standards, maintainer, system, security, concurrency,
OTEL and durability; `claim-comparison.md`; completed `verification.md`; all
three historical verdict/validation ledgers; R1/R2/R3 remediation packets and
their evidence; and the historical replay/overlap follow-up. This validator is
fresh relative to implementation/discovery. There was no intended verdict.
Agreement among the nine reports was not used as source proof.

CodeGraph preceded source navigation. Its incomplete broad results were
expanded through actual owner bodies, callers, sibling writers and consumers.
Reviewed callback and correlation producers, both runtimes' setup/lifecycle,
native settlement/drain, public package/declaration consumers, real framework
builders and assertions, docs/lanes, and the full trusted publisher-to-replay
seam. Full correction-owner bodies were inspected. No source/spec/task/index or
HEAD mutation, build, test, installation, new probe, staging or commit was
performed. Only this report is owned. HEAD remained pinned and tracked/index
diffs were empty at the source guard.

## Discovery and closure claim dispositions

There are no proposed material source-local findings in any current discovery
report. An empty union still requires validation of implementation and proof.

| Claim | Disposition | Independent source resolution |
|---|---|---|
| Behavior/invariant complete acceptance and prior 1–8 closure | CONFIRMED | Current bridge, provider, package, docs and live readback owners satisfy the matrix below; historical verdicts alone did not establish closure. |
| All domain claims that FIND-9 closes at Gate | CONFIRMED | Verified `Attribution` is an explicit input to the sole implicit ID producer and the same pair is sent for Scribe stamping. All signals/transports converge there. Native explicit IDs and outbox identity remain separate owners. |
| Standards/maintainer FIND-10 closure | CONFIRMED | `state.rs:3953–3995` now documents Panics matching its expect/assert/panic body, with assertions preserved. |
| Behavior/standards/security FIND-11 closure | CONFIRMED | The child emits the actual public `WyrdError.code` and reraises; the parent requires nonzero exit and exact JSON equality to `WYRD_CLIENT_401_NO_CREDENTIALS`. Human text cannot substitute. |
| FIND-12 current-schema readback and sibling closure | CONFIRMED | Existing Rust journey selects/extracts both stamps and Run, groups all three and checks exact marker sets. The seven-file OTLP capability has no remaining obsolete `principal_id` row read. |
| Cross-version ID change is a bounded rollout limit | CONFIRMED with explicit scope | The limit is real and is not approved merely because implementation disclosed it. The contract assessment below distinguishes retained native/WAL identity from derived OTLP identity; it establishes no unfulfilled required migration or stable cross-version derivation contract. |
| Function-scoped import observation | REJECTED as a material proposal | Standards records a placement deviation at the settlement test. It has no behavioral/security/durability/public-contract consequence, creates no replacement mechanism and is not missing rustdoc. It does not justify remediation under the review threshold. |

No present claims materially conflict or reveal an untraced required reachable
path. The earlier replay uncertainty has current producer/consumer and dynamic
proof, and the known old/new derivation limit is adjudicated below. The no-new-
follow-up decision is supported; another discovery pass solely to repeat these
traces would not supply missing authority. No rejected proposal is carried as
optional remediation.

## Source traces and existing-owner comparison

### Framework and correlation producers

Python `python/wyrd/otel.py::WyrdAgentsTraceProcessor` retains the actual OTEL
spans keyed by live framework IDs. `_start` holds the map lock through known
parent/root selection and publication; `_end` removes once under the same lock,
then maps final data and ends that original span. Shutdown atomically closes
and takes the unfinished map, then marks those spans ERROR with a fixed
payload-free description. Claimed end callbacks may finish concurrently;
shutdown need not join arbitrary application callback threads. Duplicate live
starts cannot overwrite the retained span; unknown/duplicate ends and late
starts are harmless. TypeScript's synchronous map operations implement the
same lifetime division without a suspension point. Both use the active OTEL
provider to generate IDs and capture correlation at start, rather than inventing
an identity system or adopting a later completion scope.

Python `_nanos` and TS `agentsTime` preserve supplied parseable timestamps and
fall back for missing/malformed input. `_gen_ai`/`agentsGenAi` use known model,
operation/tool and supplied counters, plus safe status/type. Unknown data keeps
basic span properties without guessed GenAI fields. Python Responses counters
come from direct safe usage with per-counter full-response fallback, retaining
zero. Neither mapper reads prompt, completion, instructions, tool input/output
or raw error payload into new telemetry metadata. Original framework IDs are
not required export attributes. The actual SDK compatibility boundary is
qualified by the documented pinned versions, not future releases.

Python Run context uses immutable context-local scope tuples and a retryable
idempotent correlation processor; TS uses the standard context key and existing
Run scope. Nesting restores the outer scope and explicit propagated contexts
remain supported. Context is captured before completion, not inferred from
the current callback at the end. Run exit performs no span/provider lifecycle.
Stock sampling remains effective; non-recording spans are not recreated.
The narrow OpenAI callback bridge is explicitly required. Native Pydantic/ADK
instrumentation needs no Wyrd adapter or common proprietary event schema.

### Provider setup, managed waiting and native settlement

Python `src/state/mod.rs::start_telemetry` is the only provider-cell initializer.
It rechecks the existing `OnceLock<Py<PyAny>>` under the per-state GIL-aware
`MutexExt::lock_py_attached` gate. `_start_telemetry` refuses an existing global
provider; a newly built candidate that loses installation shuts down only
itself. A failed import/installation publishes no result and permits retry.
GC traversal/clear and lifecycle readers use the same token-free cell without
taking the setup gate. This preserves the existing retained-object/GC owner;
neither a second registry nor a PyOnceLock/GC redesign is needed.

TS `WyrdState.startTelemetry` checks its field again after dynamic import and
then installs/publishes synchronously. Other-owner refusal releases only the
loser. A shared startup Promise or mutex would duplicate this ordinary runtime
boundary. Application-owned providers compose stock exporters and the public
construction-time `runCorrelationProcessor`; Python installation rolls back
its marker on failure. Resources, sampler, existing exporters, propagation,
context and lifecycle stay with the application.

Python `_ManagedTelemetry` owns one provider, lock, Future and started-operation
count. A call snapshots that count at entry and waits within one monotonic
deadline. An older pending operation may be awaited, but cannot itself satisfy
that call's completed-span barrier; a covering operation starts after entry.
At most one stock flush is in flight and no pending work/retry/export backlog
is built. Timeout does not join the worker or shut down the provider. This
runtime wait owner is earned: the pinned synchronous stock processor ignores
its nominal wait bound. Replacing ordinary callbacks with a shared concurrent
processor would change caller-local correlation and may block callbacks behind
outstanding exports. The existing native Bifrost drain cannot flush a foreign
provider. Standard threading/Future coordination around only lifecycle waiting
is the smallest sufficient extension.

Both Python lifecycle consumers call `flush_telemetry`, then always attempt the
existing shared native drain, then use `TelemetryFlushFailure::settle`. TS
captures its stock provider's rejection, calls native flush/shutdown and projects
the same shared catalog settlement. The native owner retains startup/closed
refusals, producer ordering, every-producer drain and ambiguous shutdown retry.
OTEL is primary when both fail; the native stable code remains secondary detail;
native-only failure retains its own code. New provider diagnostics contain
scrubbed stage/type, not raw payload/messages. State shutdown never closes the
installed global provider; owned providers and Run acquire no state lifecycle.

### Credentials, authenticated identity and replay consumers

Python `_session` and all three exporter factories use the selected client's
`access_token()` per request; TS's common exporter config supplies current
async headers per export. Both reuse stock OTLP transports. Framework gateway
clients use existing `http_auth`/`httpAuth`; external model clients receive
independent provider keys. Neither Run/Card attributes nor lifecycle workers
resolve another credential/tenant. The child/parent exact-code change is test
communication, not a new ambient resolver or production error mapping.

Shared `delegated_attribution` maps the verified principal to subject and the
last verified initiator-first chain member to current actor, or subject to
itself for direct work. HTTP OTLP copies that verified chain into AuthContext;
gRPC uses `AuthContext::from_verified`. Gate's trace/log/metric methods first
authorize record writes and asserted Cards, then all call
`dispatch_canonical`. That owner computes a `LogicalIdentity`, calls
`otlp_batch_id(auth.tenant, auth.attribution(), ...)`, and sends that same pair
in `ScribeIngressFrame`.

The hash now binds tenant, subject, current actor, table, logical digest and
accepted correlation digest. It excludes refreshed bearer bytes, request ID,
time and prior actor history, preserving legitimate same-pair convergence.
No caller attribute can assert publisher identity. `LogicalIdentity` retains
the existing logical-data calculation/reuse check; `correlation_data_identity`
separately covers accepted Card/Run. Scribe ingress forwards the pair;
`stamp_correlation_columns`/`managed_arrays` stamp both members from trusted
context and preserve table-owned physical schema. Managed-column exclusion
from logical data remains unchanged.

The existing active/immutable memtable lookup matches batch ID and logical
slice facts. Shard control-fence construction carries the received batch ID;
tenant-scoped SQL `resolve` recognizes equal digest/count as AlreadyCommitted
and `resolve_replay` suppresses later matching WAL attempts. WAL reconstruction
uses the recorded identity, never Gate's current derivation. These consumers
need no actor guard or replacement fence. Native dispatch still validates and
forwards the caller-supplied UUIDv7, retaining its first-acceptance semantics.
Internal `ScribeSink::identify` already hashes both pair members under its own
tenant/table/request/IPC domain. Moving that workflow into Gate would change
the wrong owner; extending the existing implicit producer once is sufficient.

Actual delegation is reachable, not merely a constructed AuthContext:
`OtlpJourney::token_with_permissions` bootstraps Card-bound services and public
token exchange; `WyrdTestServer::delegate` posts subject and actor tokens to
the real route. Exchange verifies both, refuses invalid/unbound/self actors,
and intersects authority while retaining subject scope. The Wyrd audience is
accepted on both tested ingest transports. The corrected journey uses those
existing helpers, exports identical direct/delegated content over HTTP/gRPC,
publishes and queries the current pair plus Run. Five groups, two direct
subjects, two delegated actors for one subject, the separate Run group and
the exact per-group marker multiset expose both collapse and replay duplication.

### Packages, procedure and proof ownership

TS root callback input structures contain only consumed fields and no imported
optional Agents declaration. Its actual-framework `satisfies TracingProcessor`
proof remains, while the existing packed checker now strict-compiles a clean
ordinary root consumer with `skipLibCheck: false` and no optional peers. This
protects a reachable published-package edge the dependency-rich monorepo build
cannot establish. It extends the existing package checker rather than adding
a second harness. Native error declarations mirror the Rust source; Python
native lifecycle stubs match runtime and pure Python definitions remain inline.

Observe defines four complete framework constructions, dependencies, provider
keys, setup, owned composition, invocation, flush and ambient readback. The TS
install row covers every direct OTEL entrypoint dependency, including retained
log/metric exporters. ADK includes runner/session/message; OpenAI explains
add-versus-replace; sampling/content/version/provider/failure limits are stated.
Tabs, current links, heading/frontmatter taxonomy and `draft: true` survive.
Lasting architecture agrees with shared current-token exporter and lifecycle
behavior. Existing generation remains the machine-doc owner.

Python actual framework builders and TS actual Agents execution drive local
scripted providers and real server/SDK readback, rather than hand-authoring
model spans. Python process isolation earns its place because providers are
global and ADK binds instrumentation at import. `live` prints only after state
flush, waits while the parent queries, then continues native/framework work,
ends the held span and exports again after state shutdown. TS reads before its
second work and likewise verifies post-shutdown export. Server publication
cannot substitute for draining the separate application queue. Existing owned,
edge/error/stream, privacy/sampling/denied/cross-tenant matrices remain present.
The normal Python integration and Bifrost leaves collect them with dev framework
dependencies; TS leaves name its actual framework file. No silent integration
skip, deleted assertion or broadened escape hides a failure.

## Prior stable finding closure

| Stable ID / source cause | Current source evidence | Focused proof and status |
|---|---|---|
| FIND-TASK-001-1 optional TS declaration edge | `index.ts:2493–2531` self-contained callback inputs; existing package checker strict clean consumer; real SDK assignment in telemetry test | Fresh packed consumer PASS and TS file 5 PASS; historical typecheck. CLOSED |
| FIND-TASK-001-2 incomplete recipes/prerequisites | Observe concrete four-framework construction/readback; install row matches all seven direct OTEL imports including logs/metrics | Historical exact clean dependency import and docs generation/check; current recipe-to-builder comparison; fresh managed execution. CLOSED |
| FIND-TASK-001-3 missing live flush/continuation proof | Python `app.py::live`, parent managed handshake; TS managed readback precedes continuation; native observations, held span and postshutdown turn | Fresh managed Python3/TS1 PASS; explicit assertions separate native continuation from global-provider continuation. CLOSED |
| FIND-TASK-001-4 safe supplied Responses counts discarded | Python `_gen_ai` direct usage and per-counter fallback, None filtering keeps zero; actual ResponseSpanData callback case | Fresh callback selector included in eight passing Python cases; absent response, late 7/0 and fallback assertions retained. CLOSED |
| FIND-TASK-001-5 stock synchronous wait overrun | `_ManagedTelemetry` one daemon/Future and bounded caller wait; both native lifecycle consumers settle only after attempting drain | Fresh real-stock deadline test holds export past bound, checks 504/native code, one worker and usable spans/recovery. CLOSED |
| FIND-TASK-001-6 TS setup suspension race | `startTelemetry:2924–2940` post-import field recheck before synchronous installation/publication | Fresh TS overlapping setup and other-owner/cleanup assertions. CLOSED |
| FIND-TASK-001-7 obsolete Python attempt accepted as later success | `force_flush` entry count and covering-operation test share one deadline; old Future alone cannot certify a later call | Fresh deadline regression's `_stale_flush` exercises flush and shutdown at real stock drain-to-settlement boundary and requires late-span export. CLOSED |
| FIND-TASK-001-8 Python same-state setup race | `start_telemetry:191–219` GIL-aware gate plus under-gate recheck; unchanged sole OnceLock and token-free GC readers | Fresh overlapping two-thread setup and missing-SDK retry cases; separate-owner refusal and losing-provider release retained. CLOSED |
| FIND-TASK-001-9 implicit OTLP actor collision | `gate/mod.rs:133–155,978–999` binds verified pair before unchanged stamping/fences; all signal callers share it | Fresh exact derived-ID unit 1 PASS, direct/delegated real cross-transport attribution journey 1 PASS. Actor-only historical mutation RED remains separate evidence. CLOSED |
| FIND-TASK-001-10 required panic documentation | `state.rs:3953–3995` substantive Panics contract names failed must-error/variant/code assertions; body unchanged | Source/body comparison; historical fmt/lints and fresh settlement selector 1 PASS. CLOSED |
| FIND-TASK-001-11 credential proof reduced to human text | `app.py:383–389` catches public WyrdError, emits actual code and reraises; parent `:280–299` exact JSON equality | Fresh real ambient gateway/credential-negative selector PASS; successful key-isolation/export/readback retained. CLOSED |
| FIND-TASK-001-12 obsolete Rust publisher readback | `negative.rs:386–554` current subject/actor/Run selection/extraction/grouping/rustdoc; seven-file sibling sweep clean | Fresh exact previously-red journey 1 PASS with five complete row sets and no per-group duplicate marker; no alias/caught schema failure. CLOSED |

The same module is not the same root cause: original wait overrun (-5), stale
success (-7), installation race (-8), actor convergence (-9), schema readback
(-12), exact-code proof (-11) and required test docs (-10) have separate closure.
No stable ID is repurposed or assigned to human-directed additions.

## Reconciled complete obligation matrix

Verification is inspected test assertions plus committed historical execution
and the fresh root-owned runs listed below. This validator executed no tests.
PASS does not relabel historical broad lanes as fresh executions.

| Requirement, acceptance criterion, constraint or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-010 | Explicit optional bridges, global-provider ordinary spans, hierarchy/fallback/timestamps, late safe metadata, start identity, duplicate/closed map and interruption handling | Python/TS callback lifecycle, thread/import cases, real OpenAI journeys | PASS |
| REQ-011 | Stock exporters/setup/correlation; native Pydantic/ADK builders; explicit OpenAI; optional Run and verified publisher-pair retention | Managed/owned real three-Python/TS framework matrix; uncorrelated cases; fresh publisher journey | PASS |
| REQ-012 | Python gate/OnceLock and losing-provider cleanup; TS post-await recheck; owned factory/installer and marker rollback preserve application settings | Setup overlap/retry/conflict and owned resource/sampler tests; managed/owned journeys | PASS |
| REQ-013 | Bounded covering Python operation and stock TS bound; OTEL then attempted native drain; shared catalog precedence; active/global/owned lifetimes | Deadline/stale/lifecycle units, shared settlement, live managed readback/continuation | PASS |
| REQ-014 | Existing standard OTLP fidelity retained; context-only optional correlation; metadata allowlist and qualified upstream content/sampling policy | Existing maximal signal fidelity tests, context/nesting/concurrency units and actual edges/privacy cases | PASS |
| REQ-015 | Observe complete installs/constructions/readback/owned/failure recipes; Connect/Understand/Quickstart and lasting design; unchanged draft and current components | Current procedures compared to actual builders/import graph; historical dependency-import/docs generation/check | PASS |
| REQ-016 | Actual frameworks/local models, WyrdTestServer/managed Postgres, SDK query; complete source scenario matrix and owner/capability selection | Fresh managed Python3/TS1; historical full Python13/TS5 files; supporting runtime-local injections | PASS |
| REQ-017 | Selected ambient shared client/current token; existing stock gateway auth; external key isolation; public structured no-credential code | Fresh gateway exact-code case; retained TS gateway and both exporter freshness/transport regression proof | PASS |
| INV-001 | Token-derived tenant/verified pair, Card authorization before dispatch, query binding; no Run/Card credential selection | Viewer/foreign-tenant negatives; fresh direct/delegated retained-pair/replay journey | PASS |
| INV-002 | No new durable protocol/runtime/registry/discovery/patching/export retry queue or Pydantic/ADK/Rust adapter; native/log/metric owners preserved | Cumulative source/owner diff; common signal dispatch and native/outbox sibling traces; Rust current-schema proof | PASS |
| INV-003 | Optional context enrichment fail-open, application exceptions retained; bridge/errors exclude content and raw errors; explicit lifecycle may report failure | Callback/privacy sentinels, real model/tool failure cases and scrubbed lifecycle assertions | PASS |
| INV-004 | No adopted provider; state owns only successful managed handle, application owns its provider, Run owns context only | Setup/owned-never-flushed tests; owned journey; active/global continuation readback | PASS |
| AC-001 | Real Python Pydantic/ADK/OpenAI plus TS OpenAI model/tool execution and SDK readback | Managed/owned framework journeys with exact model/tool counts/ancestry and available usage | PASS |
| AC-003 | Catalog projections/declarations, optional package consumer, lane/dependency closure and required Rust docs present | Historical scoped format/lint/types/codegen/deps/docs; fresh exact tests/package/whitespace. Final integration selection remains change-review-owned | PASS within task scope |
| AC-007 | Bridge source fidelity/start identity and optional imports/types; safe supplied Responses usage | Callback/thread/import/source assertions; fresh callback/TS/package runs and real frameworks | PASS |
| AC-008 | Managed same-state convergence and owned-provider preservation/composition | Fresh setup/overlap/retry proof; historical owned runtime and real owned journeys | PASS |
| AC-009 | Live flush barrier, honest bounded outcomes/native drain, active span and global provider retained | Fresh live Python3/TS1 and real-stock deadline/stale tests; shared settlement and lifecycle injections | PASS |
| AC-010 | Native contexts and callback maps retain no-Run/nested/concurrent/enclosing/error/stream identity without duplicate logical spans; implicit pair-aware fences | Actual edge matrix and callback tests; fresh five-group publisher-pair/Run replay proof | PASS |
| AC-011 | Stock sampler/native controls and safe bridge; unchanged payload/Card/tenant authorization | Actual sampling/privacy/denied/cross-tenant matrix plus callback sentinels; source and historical full-file proof | PASS |
| AC-012 | Usable complete recipes with readback, durable authority and source-driven machine docs; no publication expansion | Recipe/import comparison, actual managed journey proof, historical docs generation/check | PASS |
| AC-013 | Ambient gateway clients, actual no-credential refusal, exporter current token and upstream key separation | Fresh Python gateway/exact-code case, retained TS gateway and freshness/shared auth regressions | PASS |

Additional constraints and non-goals all pass: no new route/table/migration/Card
kind/durable event payload or new production dependency/Cargo feature; no
framework discovery/patching or universal capture/redaction; no provider
adoption, global shutdown redesign, new token cache/transport/export retry
owner; no manually authored framework-internal model spans as compatibility
proof; no implicit arbitrary thread/detached-task propagation promise; no
developer saved credentials/config in the isolated journeys; no Pydantic/ADK TS
adapter or Rust framework adapter; no draft publication; no hand-edited parallel
catalog/generated contract. The authorized imported-main schema changes are
distinguished from task scope, while their publisher/query seams are included.

## Explicit old/new OTLP derivation assessment

The risk is concrete: R3 appends actor bytes to the hash even for direct
publishers. For identical accepted OTLP content, the prior and current
derivations generally yield different UUIDs. If one export is accepted through
the old version and retried through the new, both identities can commit. The
current unit/journey tests prove current-pair/current-derivation convergence,
not mixed-version rollout deduplication. Implementation's disclosure is evidence
of that behavior, not permission to change an approved requirement.

Independent authority comparison does not establish a material conflict:

1. Wyrd's observation contract defines the authenticated publisher as subject
   and current actor. The prior subject-only derived owner therefore failed
   to preserve a distinct accepted publisher's attribution. Binding the verified
   pair realizes that existing requirement at its existing source.
2. Bifrost's explicit native retry contract preserves caller batch IDs and
   matches their logical facts within retention (`bifrost-design.md:102–123`).
   Neither that wire identity nor its SQL/WAL format changes. WAL recovery
   reuses recorded IDs, and native supplied-ID first-acceptance remains intact.
3. The telemetry reference requires deterministic duplicate convergence, and
   the candidate provides it for its implicit producer across both transports,
   replica requests, refreshed credentials and restarts. No applicable approved
   contract fixes the private OTLP hash byte encoding forever or requires two
   server software derivations to interoperate during upgrade. OTLP itself
   supplies no Wyrd batch-id wire field for the exporter to preserve. Treating
   the existing internal domain separator as a new public compatibility version
   would invent a contract absent from authority.
4. An upgrade migration/dual-fence lookup or preservation of selected old IDs
   is not required to close the actual publisher defect. A downstream fallback
   to an old subject-only fence can itself erase the newly required actor
   separation. Such new persistent/compatibility machinery is not a bounded
   missing implementation demanded by r2. It is not prescribed as remediation.

Accordingly this is a disclosed bounded rollout limitation, not a confirmed
new FIND-13 or a reason for SPEC_REVISION_REQUIRED. This conclusion is narrow:
it does not bless generic changes to retry identity, claim seamless rolling
upgrades, or amend duplicate convergence/native/WAL requirements. A deployment
must not infer mixed-version deduplication from this PASS.

## Final ledger, recommendations and verification limits

**Validated material ledger: empty.** Every current empty proposal set and each
prior closure was checked against source. No MISSING, INCORRECT, DRIFT,
VIOLATION or REGRESSION remains that requires a bounded correction under the
approved task. There is no recommendation to add another adapter, registry,
worker pool, migration, consumer guard, optional proof harness or broad test
suite. Keep the existing owners and their tests. No remediation task is needed.

Fresh root-owned `verification.md` records, on the pinned candidate:

- cumulative diff whitespace PASS;
- exact derived-ID unit selected 1 / passed 1;
- exact shared settlement regression selected 1 / passed 1;
- TS telemetry unit file 5 passed and existing packed clean strict consumer PASS;
- Python setup/deadline/optional-import/Responses selectors 8 passed;
- managed Postgres migration selected 1 / passed 1;
- exact real direct/delegated HTTP/gRPC attribution journey selected 1 / passed 1;
- actual managed three-framework Python cases plus ambient gateway/exact-code
  case 4 passed, no skips;
- actual managed TS OpenAI case selected 1 / passed 1; four other cases were
  explicitly unselected by that focused invocation.

All sessions finished, including normal server/environment cleanup. No pending
result is counted as success. This repairs the preceding review's red Rust
readback with current schema and dynamically exercises the allowed delegated
actor path. Historical actor-only mutation RED is separate from the old removed-
column RED. Committed original/R1/R2/R3 records supply broader framework,
owned/edge/privacy, format/lint/type/generated/dependency/docs results; they
are not fresh whole-suite executions by this validator or this round. Final
integrated/shared-infrastructure verification remains the explicitly assigned
change-review gate, as the original task specifies.

Known retained limits: stock Python export refusal can be logged while provider
flush returns true; TS Chat Completions supplies no usage to map; native
instrumentation may emit residual content it owns; sampling may drop spans;
a timed-out Python stock operation can finish in the background; abrupt exit
can lose unacknowledged in-memory work; query visibility is distinct from
provider/native flushing; and mixed-version implicit-ID replay was neither
guaranteed nor dynamically tested. None is silently converted into a stronger
capture/delivery guarantee. No live cloud/model/OIDC, crash drill, benchmark or
production credential was used.

The mandatory reports and immutable source are available; no unresolved
authority conflict or untraced required finding remains. **PASS** is supported.

Skipped own execution, broad gate reruns and unrelated imported-main certification.
Risk: old/new derived OTLP identities can each accept a retry across rollout;
the unchanged best-effort/exporter and abrupt-exit limits also remain.
