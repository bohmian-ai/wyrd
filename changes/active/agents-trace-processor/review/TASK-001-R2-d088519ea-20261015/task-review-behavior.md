# TASK-001 independent behavior acceptance review

**Result: PASS.** No material behavior finding proposed. This is the independent
behavior review, not the orchestrator's final verdict or a claim that integrated
change-review verification has run.

## Immutable subject and scope

- Original base: `dac4c340f835676adc29f3dcc01f98e1d3d51465`.
- Candidate: `d088519ea846aba8b2a77c24b2af2491ad2b3936`.
- Approved authority: `changes/active/agents-trace-processor/spec.md`, revision 2,
  and the complete original `tasks/TASK-001-framework-agnostic-telemetry.md`.
- Historical inputs: both previous verdicts and the R1/R2 remediation packets.
  Their closure statements were hypotheses, independently checked in source.
- Full range: original base to candidate. The supplemental contrast against
  `cb29781ab6f633700f9e47befddb1f325fd8b732` separates task changes for navigation;
  it does not replace the original base. The authorized main merge is included.

HEAD was read as the exact candidate before source investigation and again at
completion. The exact guard is:

```sh
test "$(git rev-parse HEAD)" = d088519ea846aba8b2a77c24b2af2491ad2b3936
```

Only this report is owned/written by this reviewer. No source, specification,
task, index, HEAD, installation or build was changed. No current peer discovery
report or implementation chat summary was read. Historical implementation
evidence is treated as reported execution, not a substitute for source proof.

## Authority and navigation

Applied AGENTS.md and agent rules; spec-driven development and maintainer style;
the reference router's telemetry, testing, error, PyO3, Python API/stub and
TypeScript guidance; applicable provider/auth/observation sections of active
Wyrd design, doctrine and Bifrost design; and docs CONTRIBUTING/PRODUCT.
CodeGraph was reached before code search. Its broad queries returned partial or
unrelated symbol matches, so source search and direct source/diff reads expanded
the map rather than treating its result as complete coverage.

Caller-to-result paths inspected:

1. Python `PyWyrdState.start_telemetry` -> state-local GIL-aware initialization
   gate -> `otel._start_telemetry` -> stock provider/exporter -> retained
   `OnceLock`; same-state repeats, other-owner refusal, failed setup and GC.
2. Python public flush/shutdown -> `flush_telemetry` ->
   `_ManagedTelemetry.force_flush/_flush` -> native state drain -> shared
   `TelemetryFlushFailure::settle`. TS follows the same ordering through
   `#flushTelemetry/#settle` and the native error factory. Both sibling public
   lifecycle consumers and the unchanged native drain owner were traced.
3. Python and TS Run scope -> existing OTEL context -> shared managed/owned
   correlation processor -> native framework spans or explicit Agents bridge
   -> stock OTLP exporter -> authenticated server Gate/Scribe -> SDK readback.
4. Both bridge callback owners, timestamp and final attribute mappers,
   duplicate/missing callback and unfinished-span paths; their actual framework
   producers and corresponding unit/journey consumers.
5. Packaging, optional import/declaration closure, authored state stub,
   generated native/error projections, clean-package consumer, framework dev
   dependencies and owner/Bifrost lane selectors; executable documentation.

## Complete acceptance matrix

Evidence paths below are repository-relative. `Py unit` means
`sdks/wyrd-sdk-python/tests/unit/state/test_observe_surface.py`; `Py journey`
means `tests/integration/otel/frameworks/{app.py,test_framework_telemetry.py}`
within that SDK. `TS unit` and `TS journey` mean the telemetry unit file and
`agents-trace-processor.test.ts` integration file within the TS SDK. Test names
identify inspected proof, not tests executed by this reviewer. Committed
original/R1/R2 packets report their focused successful runs and relevant checks;
fresh execution belongs to the orchestrator.

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-010 — faithful optional explicit Agents bridge | Python `otel.py::WyrdAgentsTraceProcessor`, `_nanos`, `_gen_ai`; TS `src/index.ts::WyrdAgentsTraceProcessor`, `agentsTime`, `agentsGenAi`: valid OTEL-generated identities, known-parent/root/current fallback, final data, start-time scope, safe status, idempotent live callbacks and interruption cleanup; no automatic registration | Py/TS `agents_callback_lifecycle`; Python threaded callbacks and no-Agents import; actual OpenAI model/tool journeys; clean-package strict consumer | PASS |
| REQ-011 — generic native instrumentation and ergonomic setup | Existing Python/TS managed exporters and context processors serve native Pydantic/ADK spans and explicit OpenAI bridge; correlation optional; direct local providers independent of gateway | Three Python managed and owned journeys, TS managed/owned journeys; uncorrelated execution in both edge tests | PASS |
| REQ-012 — ownership, repeat setup and sampling | Python `state/mod.rs:198-235` gate covers check/install/publication while preserving token-free OnceLock reads; `_start_telemetry` cleans losing provider. TS `startTelemetry` rechecks after import. Owned processors reuse standard SDK extension points and no provider lifecycle | Py `test_telemetry_setup` includes overlap, other state/application, lost race; retry after missing SDK; owned registration retry and retained resource/sampler; TS `telemetry setup` and owned correlation | PASS |
| REQ-013 — bounded managed flush, then native drain | Python `_ManagedTelemetry` has one deadline, one active stock flush, fresh covering operation after obsolete attempt; binding always drains then settles. TS configured provider bound and equivalent settle. Native-only error unchanged; global provider remains installed | Py lifecycle injected false/timeout/exception/native outcomes, real-stock deadline and stale-flush/stale-shutdown cases; TS lifecycle injection; both live managed journeys | PASS |
| REQ-014 — native fidelity, scope, privacy and sampling | No task-specific native-span normalization; existing OTLP retains canonical fields. Correlation uses existing context; native instrumentation controls content; narrow bridge reads metadata only. Docs qualify versions and residual fields | Native signal/observation regressions retained; Py/TS nested/concurrent/enclosing/stream/uncorrelated journeys and sampling/privacy tests; owned correlation resource/sampler checks | PASS |
| REQ-015 — usable docsite and durable authority | Observe page supplies installs, tool/agent construction, ADK runner/session/message, managed and owned recipes, Run scope/readback, content/sampling/version notes and both lifecycle codes; TS install includes all direct OTEL entrypoint dependencies. Connect/Understand links and design provider contract agree; draft preserved | Recipe-to-journey comparison; committed R2 isolated exact-dependency load probe; docs generation/check execution reported in original and R2 evidence | PASS |
| REQ-016 — actual framework evidence and lane closure | Python app uses supported real framework/provider instrumentation against scripted local model, subprocess per global provider; TS stock model/runner/tools; real Wyrd server/Postgres and SDK readback; framework dependencies now normal dev collection and both Bifrost leaves name journeys | Py all-framework managed/owned/edges/privacy/auth cases; TS equivalents and gateway case; positive historical focused counts and lane collection; runtime-only failure injection stays unit-level as permitted | PASS |
| REQ-017 — existing ambient auth and fresh shared tokens | Python `_session` obtains `client.access_token()` per request; TS exporter headers callback obtains `accessToken()` per export. Lazy state client retained; stock gateway model uses existing `http_auth/httpAuth`, no new auth owner | Py/TS gateway journeys inspect local upstream credential and absence of Wyrd headers; no-credential refusal; exporter-token-freshness and existing transport regressions | PASS |
| INV-001 — authenticated tenant/publisher; authorized optional Card | Clients supply only optional Run/Card; verified server authority selects tenant and subject/actor attribution. Imported-main Gate/Scribe pair propagation preserves this separation; no SDK token selector or tenant switch | Both languages' viewer and foreign-tenant Card refusals/readback; existing signal/auth tests; inspected managed-column producer/stamper | PASS |
| INV-002 — no replacement protocol/runtime/export pipeline | Task changes reuse stock provider, batch processor/exporter, OTEL context and existing auth/ingest/query; no Pydantic/ADK adapter, discovery, monkey-patch, queue or retry owner. Python worker bounds only lifecycle waiting | Cumulative task-source diff; standard-library Future/thread coordination inspected; existing Rust/log/metric surfaces remain; dependency checks reported | PASS |
| INV-003 — enrichment preserves application outcome and content boundary | Scope hooks fail open; explicit setup/lifecycle may fail; callbacks map only metadata with fixed interruption/error text; native content controls/disclosures preserve ownership | Scope failure/user-exception regressions; callback sensitive sentinels; model/tool failures remain observable; full SpanRecord privacy checks, not metadata-only SQL | PASS |
| INV-004 — explicit provider lifecycle, no Run lifecycle | Managed state retains only its installed provider and never closes it. Owned provider never enters state telemetry cell. Run entry/exit creates no span or flush. Adapter shutdown releases its mappings only | Owned-provider-is-never-flushed test; unit active-span checks; live journey active/global provider continuation; bridge shutdown/late callbacks | PASS |
| AC-001 — actual Python3/TS framework export/readback | Native Pydantic AI/ADK and explicit OpenAI bridges; stock model endpoints and real tools | Py managed/owned across all three, TS managed/owned; assert two model calls/one tool, exact model and available usage, ancestry, Run/Card and no duplicate IDs/logical calls | PASS |
| AC-003 — scoped checks, generated/boundary and lane proof | Catalog/native sources produce error/native projections; strict generic TS package consumer protects optional declaration contract; owner/capability selection inspected | Original/R1/R2 committed format/lint/type/codegen/dependency/docs results; current verification.md records 16 Python/5 TS unit cases, 13 Python/5 TS real journeys, clean-package strict consumer and documented-import closure all passing | PASS |
| AC-007 — fidelity, start-time identity, optional imports | Same bridge owner/mappers; local TS structural inputs avoid optional Agents declaration dependency; Python real ResponseSpanData direct usage preserves zero/fallback | Callback tests assert times, ancestry, late data, missing parent, safe errors and retained original Run; Python callbacks across threads; clean generic consumer | PASS |
| AC-008 — idempotent managed and composable owned providers | Python serialized fallible initialization; TS post-await check; Python owned registration marks successful install and can retry failure; TS construction-time factory | Setup repeat/overlap/refusal/cleanup/retry; real owned export/readback plus resource/sampler preservation | PASS |
| AC-009 — live flush and failure/drain/provider survival | Managed lifecycle sequence and per-call covering barrier; writer and global provider owners remain separate | Py parent reads first turn/observation while child waits after state.flush; TS reads before shutdown; second work/native writes, held span, post-shutdown global export; bounded/obsolete-result unit injections | PASS |
| AC-010 — scope, error, stream and ancestry edges | Existing context authority used by bridge and native producers; completion does not rewrite Run/Card; known fallback parent only | Py3/TS real edge journeys plus explicit nested Card restoration/callback late completion tests; missing upstream TS Chat Completions usage remains omitted rather than invented | PASS |
| AC-011 — sampling, content controls, denial and isolation | Stock sampler/content controls, narrow bridge mapping and unchanged server authorization; full canonical record readback exposes attributes/events/links/resource | Always-off yields no model trace; viewer persists none; other-tenant asserted Card denied and cross-tenant query empty; prompt/result sentinels absent from complete returned records | PASS |
| AC-012 — documented execution and confirmation | Complete framework recipes and exact TS module import prerequisite closure; readback and lifecycle troubleshooting; current language tabs, headings and draft metadata | Source recipe walkthrough against actual builders/journeys; isolated dependency probe reported; docs checks reported | PASS |
| AC-013 — ambient stock gateway and fresh exporter tokens | Same shared client/auth transport as existing stock clients; no telemetry-specific credential resolver/cache; external model credentials independent | Gateway-to-telemetry, no-credential and token-freshness proof in both runtimes; isolated config homes and upstream header assertions | PASS |

## Constraints, non-goals and source-level simplification audit

| Constraint or non-goal | Source evidence and boundary | Result |
|---|---|---|
| Native runtime glue only; durable Rust owner preserved | Python/TS bridge/provider glue remains in SDK; shared settlement is pure catalog projection; no Python/Node runtime added to Rust tests | PASS |
| No new telemetry server route/table/migration/durable payload | Task-specific contrast adds none. The explicitly authorized main merge changes subject/actor storage/contracts under its own authority and is not classified as telemetry scope drift | PASS |
| No new auth transport/cache/retry, framework registry/patching or Pydantic/ADK adapter | Existing stock OTLP, shared current-token path, context and native framework APIs cover the outcome; new code does not duplicate them | PASS |
| Preserve native Bifrost lifecycle and sibling log/metric/Rust telemetry | Native state drain remains its existing owner; both language lifecycle consumers use it after telemetry attempt. Signal exporter siblings retained. TS docs install closure adds existing peers rather than deleting their factories | PASS |
| Optional framework production dependencies; authorized Python floor/dev changes | Python `agents` remains extra, native frameworks in dev; TS Agents peers optional and root declaration independent. Python 3.11/dev-lane change is explicitly owner-directed in task evidence | PASS |
| No global-provider shutdown redesign, arbitrary detached-thread propagation, universal redaction or new delivery guarantee | Managed provider stays installed, owned provider remains application-owned, standard context propagation only. Background timeout worker does not add persistence/retry/cancellation promises | PASS |
| No silent skip, synthetic compatibility substitution, unpublished-page promotion or new permanent checker | Actual model/tool producers replace sole synthetic compatibility proof; owner/Bifrost lanes collect them; `draft: true` retained. Existing package checker is extended, not replaced | PASS |

The new Python managed holder earns retained state: provider, active operation,
lock and operation counter enforce the approved bounded/fresh-call lifecycle.
Deleting it restores the pinned SDK's ineffective timeout or the prior stale
result bug. It does not own export batching. The Python setup mutex gates one
existing retained cell; the installed PyO3 GIL-aware primitive is reused rather
than inventing another provider registry or GC owner. TS's single post-await
check is sufficient because installation/publication is then synchronous.
Structural TS callback inputs remove the unneeded optional framework type edge;
the processor factory reuses the same stateless correlation processor. No
material unearned abstraction/dependency/configuration requiring deletion found.

## Prior finding reassessment

| Stable ID | Current source and reachable closure | Assessment |
|---|---|---|
| FIND-TASK-001-1 | Root Agents callback declarations are local structural inputs; optional framework is not a root declaration import. Existing clean-package checker now strict-compiles root use with no optional peers; real SDK assignability remains tested | CLOSED |
| FIND-TASK-001-2 | Framework constructors/prerequisites/readback completed; Observe TS install now includes log and metric exporter modules unconditionally imported by `src/otel.ts` | CLOSED |
| FIND-TASK-001-3 | Real managed journeys read work after live state.flush, continue native/framework work, hold a span, then export through the same global provider after state shutdown | CLOSED |
| FIND-TASK-001-4 | Python Responses mapper reads final direct `data.usage` first, zero retained, full-response fallback for absent counts; real payload-free ResponseSpanData asserted | CLOSED |
| FIND-TASK-001-5 | Caller waits on Future with one absolute timeout; held real stock export does not delay native drain until export completion and no per-call worker backlog is created | CLOSED |
| FIND-TASK-001-6 | TS rechecks retained provider after its sole asynchronous import boundary; no await before installation/publication | CLOSED |
| FIND-TASK-001-7 | Later call snapshots started count, cannot accept operation begun before entry, waits for old completion then obtains a newer operation within the same deadline. Both public flush/shutdown share this owner; deterministic stock final-drain/settlement tests cover both | CLOSED |
| FIND-TASK-001-8 | Python state acquires `MutexExt::lock_py_attached`, rechecks retained provider, and serializes fallible construction/publication; failures remain retryable. GC traversal/clear retain token-free OnceLock access without taking this gate | CLOSED |

## Authorized main-merge seams

The full 288-file range contains imported main changes. This review inspected
their interactions with this task rather than claiming a review of all unrelated
main behavior:

- Verified HTTP/OTLP Caller and Gate AuthContext remain the authority producer;
  `AuthContext::attribution` derives subject/current actor from the verified
  principal/delegation chain. Gate span projection keeps optional Card/Run
  authorization, then dispatches trusted attribution to Scribe.
- Scribe admitted context and managed column stamping carry that pair; user
  columns cannot replace server publisher columns. Optional native correlation
  survives separately. SDK exporter code neither creates nor selects that pair.
- Signal tables, query/trace API and fixture consumers retain canonical span
  fields and tenant-bound readback; docs/examples reflect publisher renaming.
  The task journeys do not hard-code removed publisher columns or old verifier
  request field names.
- Shared WyrdState's imported bound-verifier field rename is separate from its
  unchanged Bifrost flush/shutdown owner. Imported local Workflow Card identity
  travels through its existing gateway dependencies; no telemetry-specific
  identity resolver or lifecycle replacement is introduced.
- The current test server/observed-service fixtures and native drain source were
  checked with the live journey consumer, including server publication flush
  after the application's own barrier. Server publication cannot manufacture an
  OTEL export omitted by the client, which the live assertion proves separately.

No source-supported task regression was found at these seams. This is not an
audit of unrelated imported verification scheduling, all authentication flows,
every SQL migration or distributed recovery path.

## Proposed findings and verification limits

**Proposed finding ledger: empty.** No MISSING, INCORRECT, DRIFT, VIOLATION or
REGRESSION proposal remains with a reachable task consequence. No style-only
or optional improvement is promoted to a blocker.

This reviewer ran read-only navigation/source/diff commands and the exact HEAD
guard, not tests/builds/installations or live probes. Historical checks in both
remediation packets establish available reported proof. Before finalization I
read root's current verification.md: 16 focused Python and 5 TS unit cases,
13 Python and 5 TS real framework journeys, clean-package strict declaration
consumer, direct documented-import closure and cumulative whitespace check all
passed. These were root executions using current installed development
artifacts, not independent reviewer runs or a fresh registry install. Full shared
infrastructure/change-review gates remain required at their owning final gate;
this report does not claim those were rerun or substitute focused checks for
them. No current peer report was used as evidence.

Skipped independent test execution and unrelated-main audit. Broad static gates
remain historical evidence; stock Python export-refusal logging, upstream
missing TS Chat Completions usage
and abrupt-exit telemetry loss retain their documented limits.
