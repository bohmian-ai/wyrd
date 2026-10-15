# Independent R1 findings validation

**FIX_REQUIRED.** Three bounded correction boundaries remain: retained
FIND-TASK-001-2, new FIND-TASK-001-7 and new FIND-TASK-001-8. Five original
diagnoses close. No specification revision is required for the selected
corrections. No source change or verification lane was run by this validator.

## Immutable subject and complete inputs

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/split/agents-trace-processor`.
- Base: `dac4c340f835676adc29f3dcc01f98e1d3d51465`.
- Prior reviewed candidate: `692d390dc40e43c6e1d7e25c7850d5c826d26a9e`.
- Current candidate: `295446005b18604030bf376de6209098a28886f7`.
- Authority: approved `../../spec.md` revision 2; original
  `../../tasks/TASK-001-framework-agnostic-telemetry.md`; repository rules,
  active Wyrd design and doctrine, reference router, spec-driven development,
  maintainer style, applicable runtime/language/error/testing and telemetry
  rules, and docs CONTRIBUTING/PRODUCT. Implementation evidence cannot amend
  the approved contract.
- Historical inputs: prior `verdict.md`, `findings-validation.md`, and
  `TASK-001-R1-telemetry-contract-closure.md`, including its appended evidence.
- Current inputs: `subject.md`, all nine discovery reports
  (`task-review-behavior.md`, `task-review-invariants.md`, `standards-review.md`,
  `maintainer-review.md`, `system-review.md`, `domain-review-security.md`,
  `domain-review-concurrency.md`, `domain-review-otel.md`,
  `domain-review-durability.md`), `claim-comparison.md`, `verification.md`, and
  the completed fresh `followup-review.md`.

All required reports are present and cover their assigned boundary. Security's
independent report was persisted verbatim by the orchestrator because that
reviewer could not edit files; this does not make it an orchestrator-authored
security conclusion. Follow-up was warranted by the contradictory lifecycle
closure claims and unclosed sibling Python startup path. Its probes corroborate
source paths; neither agreement nor probe output substitutes for source
validation here.

CodeGraph was consulted before code navigation. The complete base-to-candidate
changed surface, and the fix diff as navigation, were inspected. Expanded owners
include Python correlation/export/bridge/managed lifecycle, the PyO3 state
field/constructor/start/flush/shutdown/GC methods, TS bridge/type/correlation and
state/provider lifecycle, shared failure settlement/catalog, public projections,
package checker/manifest, real framework builders and readback assertions,
fixtures, docs and lane selection. Installed OTEL Python 1.42.1 and PyO3 0.28.3
mechanisms were read directly. The initial attempted Python dependency path used
3.11; the actual existing environment is 3.12 and its sources were then read.
No dependency behavior is inferred from that unsuccessful path lookup.

## Proposal-by-proposal dispositions

| Proposal or conflicting claim | Disposition | Source-backed resolution |
|---|---|---|
| BEH-R1-001 | CONFIRMED | New FIND-TASK-001-7. The pending Future can represent a provider operation that already finished draining before this call began. Its True is not this call's flush barrier. |
| OTEL-R1-001 | CONFIRMED, duplicate | Same cause and both public consumers as BEH-R1-001; one FIND-7 correction, no callback/exporter-specific additional finding. |
| DUR-R1-01 | REVISED | Its behavioral counterexample is confirmed and deduplicated into FIND-7. Reject reopening FIND-5 solely because the same owner now contains a different failure: original synchronous unbounded waiting is corrected; obsolete successful-result reuse is introduced by R1. |
| FUP-OVERLAP-01 | CONFIRMED, duplicate | Final stock-drain-to-Future-settlement interval establishes FIND-7. The ordinary held-export test instead queues spans before stock EXPORT_ALL's final check and cannot falsify this interval. |
| INV-R1-001 | REVISED | New FIND-TASK-001-8. Python shared-state initialization is reachable and non-atomic. Revise the proposed PyOnceLock substitution to preserve GC traversal, as detailed below. |
| FUP-OVERLAP-02 | REVISED, duplicate | Same startup cause as INV-R1-001. Public-runtime corroboration is credible; installed PyOnceLock's fallible initializer alone is not a complete drop-in correction for this GC-owning state. |
| INV-R1-002 | CONFIRMED | Retain FIND-TASK-001-2. The construction gap is corrected, but the explicit TS install command still omits two mandatory imports of the setup module. |
| Behavior/standards/maintainer/OTEL broad closure of FIND-2 | REJECTED for full closure | Defined execution objects establish partial closure, not executable prerequisites: `src/otel.ts:17-18`, emitted `dist/otel.js`, and optional peer metadata establish the remaining loader failure. |
| System/concurrency/maintainer PASS claims for managed operation coordination | REVISED in scope | Bounded wait, bounded work, correct Future identity, preserved callbacks/native drain and no cancellation promise all hold. They do not establish that an earlier completed drain covers a later call's already-ended spans. No arbitrary-write snapshot or cancellation requirement is added. |
| Durability claim that FIND-5 remains open | REJECTED as stable-ID/closure claim | Same module is not same root cause. The original real-stock timeout defect closes; FIND-7 retains the newly produced false-success state. |
| Broad Python startup/idempotence PASS rows | REJECTED for same-state overlap | A OnceLock final `set` does not serialize the Python initialization side effects between `get` and `set`; the GIL is released within legitimate construction/import work. |
| All relevant closure claims for FIND-1, FIND-3, FIND-4, FIND-6 | CONFIRMED | Independently traced in the six-row closure matrix below, rather than accepted from votes. |
| Security empty material ledger | CONFIRMED | Export retains one selected client and fresh-token owner; the worker does not resolve a new identity. Allowlists and type-only errors are unchanged. No new trust/tenant/durable authorization owner appears. |
| Standards/maintainer nonblocking import placement, interface/type spelling, historical RED and fixture notes | REJECTED as material findings | No additional behavioral/public-contract consequence or unearned replacement machinery is established. No missing Rustdoc is waived. No retroactive RED fabrication, unrelated fixture rewrite or style-only remediation is required. |

Every current material source-local proposal has a disposition. No proposal
remains unresolved. Human-directed ambient-auth, docsite, Python-floor and
ordinary development-dependency changes are authority inputs, not retroactive
review findings, and receive no FIND IDs.

## Six-prior-findings closure matrix

| Prior stable ID and original cause | Current implementation evidence | Proof assessment | Closure |
|---|---|---|---|
| FIND-TASK-001-1: optional Agents declaration edge | `wyrd/src/index.ts:2493-2531` declares consumed structural callback inputs and removes the imported Agents base/type edge. The optional OTEL storage type is private and does not escape the root declaration. `scripts/check_ts_package.mjs:155-178` compiles root APIs/processor with no optional peers and `skipLibCheck false`; actual framework assignability remains in `telemetry.test.ts`. | Fresh package checker and five TS unit tests pass; historical TS typecheck also covers real `setTraceProcessors` call. | CLOSED |
| FIND-TASK-001-2: complete executable procedures/prerequisites | Observe `:182-270` now builds the Python agents, ADK runner/session/message and TS tool/agent; invocation and authenticated readback are supplied at `:281-320`. However `:168` lacks logs/metrics exporters required to load the unchanged OTEL module. | Docs generation/check and fully provisioned development journeys cannot prove this missing clean-install dependency. INV-R1-002's isolated loader probe corroborates the static import graph. | OPEN, reduced to prerequisites |
| FIND-TASK-001-3: no live flush/continuation journey | Python `frameworks/app.py:312-341` flushes before printing and waiting; parent `test_framework_telemetry.py:138-187` queries before resuming. Second native/framework work, held-span completion, state shutdown and later global-provider export are distinct phases. TS journey `:232-259` has the same ordering. | Fresh three Python managed cases and one TS managed case pass. Server publication only publishes accepted data; it cannot drain the separate application OTEL queue/native writer. Original missing proof closes. | CLOSED |
| FIND-TASK-001-4: direct safe Responses usage discarded | Python `otel.py:477-483` reads direct usage with per-counter response fallback, retaining zero; final `_end` invokes that mapper. Actual `ResponseSpanData(response=None)` is populated late in callback test and exact payload-free attributes asserted. | Fresh callback selector passes, with absent-response 7/0 case and existing full-response fallback. No TS direct field exists to add. | CLOSED |
| FIND-TASK-001-5: stock synchronous flush overruns and blocks native drain | `_ManagedTelemetry:216-254` runs stock flush on a daemon; Future wait is bounded. Binding `mod.rs:508-524` converts builtin TimeoutError before either native drain. Correlation and ordinary emission stay synchronous. Only one attempt runs. | Fresh real-stock held-export deadline selector passes: repeated bounded waits, 504/native code, ordinary correlated emission during hold, one worker, release/recovery and global survival. This directly closes the old cause. New obsolete success is FIND-7. | CLOSED, new regression separately retained |
| FIND-TASK-001-6: TS import-await startup race | `index.ts:2931-2943` rechecks the same state immediately after import, then installs/publishes without another await. `otel.ts:96-107` preserves distinct-owner refusal/loser cleanup. | Fresh TS setup case overlaps calls before awaiting and checks one provider plus other-owner refusal. Python has a separate runtime owner (FIND-8), not an uncorrected TS path. | CLOSED |

## Deduplicated retained ledger

### FIND-TASK-001-2 — CONFIRMED — MISSING: TS install procedure cannot load telemetry

**Discovery:** INV-R1-002; residual of the prior recipe finding. **Obligations:**
REQ-015, AC-012 and original task's complete prerequisite/procedure outcome.
**Exact location:** `docs/src/content/docs/build-and-verify/observe/record-a-run-and-observations.md:168`.

The recipe installs the trace exporter and API/context/core/trace SDK, but
`sdks/wyrd-sdk-ts/wyrd/src/otel.ts:17-18` unconditionally imports
`@opentelemetry/exporter-logs-otlp-proto` and
`@opentelemetry/exporter-metrics-otlp-proto`. `index.ts:2930` imports this module
for `startTelemetry`; the emitted `dist/otel.js` retains both imports. Node
resolves the module's full static import graph before calling its trace-only
installer. Package `peerDependenciesMeta` marks these peers optional, so the
documented install need not supply them. The installed Agents package is not a
substitute owner of Wyrd's optional exporter peers. Root clean-package import
and noEmit checks intentionally do not load this optional module.

**Reachable consequence:** a clean user following the TS procedure reaches
`ERR_MODULE_NOT_FOUND` on `await state.startTelemetry()` before any model
execution/export/readback. Development's already-installed peers and green
docs structure checks conceal it. The invariant report's clean temporary-module
probe reproduces the logs-exporter refusal; source independently establishes
the analogous metrics dependency. The remaining gap shares prior FIND-2's
complete-prerequisite cause; no new documentation ID is warranted.

**Selected smallest correction:** add these two existing optional peers to the
existing TS installation command. Preserve module/exporter behavior and
production optionality. Do not split the module, change loading strategy,
redesign logs/metrics, make the peers mandatory for ordinary SDK consumers, or
add another package checker. Deletion of the imports would break the existing
log/metric factories and is outside this documentation correction.

**Focused closure proof:** statically reconcile that command with every direct
external import of the current OTEL entrypoint and perform the same isolated
module import with exactly the corrected dependency set using existing packages
(temporary work cleaned up). The module must load without installing unrelated
development peers. This is a documentation/dependency procedure correction and
needs no new permanent test/harness or manufactured RED. Run
`mise run docs:generate`, then `mise run docs:check`, preserving index/drift
requirements, tabs/frontmatter/current links and `draft: true`. Existing real TS
managed journey remains behavior proof; another server test is unnecessary.

### FIND-TASK-001-7 — REVISED — REGRESSION: an older Python flush can falsely satisfy a later call

**Discovery:** BEH-R1-001, OTEL-R1-001, DUR-R1-01, FUP-OVERLAP-01.
**Obligations:** REQ-013, AC-009, original task managed lifecycle scenario;
REQ-010/014 and INV-002/004 constrain the correction.
**Exact producer:** `sdks/wyrd-sdk-python/python/wyrd/otel.py:236-252`, specifically
pending `_attempt` reuse and returning its result. **Consumers:** public
`PyWyrdState::flush:278-281` and `shutdown:300-303`, through
`flush_telemetry:508-524`; shared `TelemetryFlushFailure::settle` cannot detect
an incorrectly produced True.

Pinned stock `BatchProcessor._export` drains under its export lock and loops
until its queue no longer requires export; `force_flush` returns after that
loop (`sdk/_shared_internal/__init__.py:167-199,240-246`). Wyrd then settles its
Future in a separate Python operation. A legitimate scheduler switch after
the last stock drain/return and before `attempt.set_result` permits:

1. Another thread ends a span, enqueueing it synchronously.
2. A later public flush/shutdown begins after that span ended. `_attempt` is
   still pending, so no new provider operation is performed.
3. The old worker publishes True. The later call accepts it, drains native
   observations and can return success with that completed span still queued.

Ordinary overlaps or a recovery call joining a previously timed-out operation
can select this interval. It does not require an outage or spans ending after
the later invocation. The current held-export test adds spans before the stock
EXPORT_ALL loop's final check, so stock often drains those correctly; that
green test cannot establish the missing barrier. Discovery and fresh follow-up
selected the real return-to-settlement boundary with stock providers/processors
and observed two True results with only the first span exported. Static source
independently explains those results.

**Consequence:** successful state flush may leave this caller's completed
framework work buffered; successful shutdown followed by exit may lose it.
Native drain has a separate queue and cannot repair it. Normal server publication
delay and Python's documented exporter-refusal logging limitation are different
boundaries and do not excuse bypassing this provider operation.

REQ-013 already requires a bounded flush of completed managed spans per state
lifecycle call. The [OTEL Trace SDK ForceFlush contract](https://opentelemetry.io/docs/specs/otel/trace/sdk/#forceflush)
also describes flushing pending spans through registered processors. The R1
implementation's appended limitation that joined calls report an earlier
attempt is lower authority and cannot amend that behavior. This does not add
lossless process-death delivery, cancellation, active-span completion, arbitrary
concurrent-write snapshots or guarantees beyond stock sampling/export outcomes.

**Selected decision-complete correction:** delete obsolete-result coalescing
as a successful barrier. Reuse this managed owner's standard-library
synchronization to serialize *fresh stock provider flushes*: every invocation
that succeeds must have a provider flush begun for that invocation after its
entry. Admission to that single in-flight operation and waiting for its result
share one monotonic absolute deadline using the existing SDK default bound.
If an older operation cannot finish in time, return the existing timeout;
otherwise run the current invocation's fresh stock flush using only the
remaining time. Keep at most one worker, daemon/background completion after
caller timeout, and no pending operation/retry/export backlog. Keep outcome
identity local to the actual attempt. Do not spend a second full timeout after
waiting or wait on an unfinished worker during timeout cleanup.

This selects conventional bounded serialization at the existing owner rather
than new span sequence tracking, a queue, generation registry, retry engine or
public option. Stock batching/export/auth remain unchanged; ordinary callbacks
and caller-local correlation stay synchronous. Both native drains and shared
scrubbed timeout/failure precedence remain unchanged. Application-owned
providers, Run lifecycle and sibling services acquire no new lock or ownership.
Provider flushing must remain outside span callback/map locks. No consumer guard
belongs in Rust settlement: the invalid success is produced in this managed
owner and should be prevented there once for both consumers.

**Exact focused closure:** extend the existing real-stock managed deadline
owner with a deterministic final-drain-to-settlement interleaving. End a second
span before the later public lifecycle invocation; while the prior operation
is pending, start the later flush and shutdown/recovery variants. Release the
prior operation and require fresh export of that span before later success,
or honest bounded 504 with native drain attempted. Assert one in-flight worker,
one total deadline, usable correlation during blocked export, recovery, active
span survival and global-provider continuation. Clean controlled scheduling
work in `finally`. Use the existing runtime-local owner; no new framework or
network harness is necessary because the gap is interpreter-local and REQ-016
permits deterministic lifecycle injection. Run:

```sh
mise exec -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q tests/unit/state/test_observe_surface.py::test_managed_telemetry_flush_deadline tests/unit/state/test_observe_surface.py::test_managed_telemetry_lifecycle tests/unit/state/test_observe_surface.py::test_owned_provider_correlation
```

These are required prospective closure commands, not executions by this
validator. Retain the existing successful real-framework journeys; do not
require broad journey-suite reruns for this remediation.

### FIND-TASK-001-8 — REVISED — INCORRECT: Python same-state setup races its own provider

**Discovery:** INV-R1-001, FUP-OVERLAP-02. **Obligations:** REQ-012, AC-008 and
original task provider idempotence. **Exact producer:**
`sdks/wyrd-sdk-python/src/state/mod.rs:183-201`, retained field at `:94`,
constructor at `:165`; `_start_telemetry` `otel.py:277-289` supplies the global
installation result. **Sibling readers/writers:** `flush_telemetry:508-524`,
`__traverse__:467-488`, `__clear__:492-498`; no other initializer repairs the
publication gap.

Two public `&self` calls may both see an empty OnceLock, then enter Python
construction before either publishes. PyO3 shared borrows permit overlap;
Python construction/import/thread startup can release the GIL. The global OTEL
once permits one provider; the losing attempt correctly shuts down its candidate
and returns None. The binding unconditionally raises 409 for that None,
regardless of the winning same-state publication. OnceLock serializes the
final `set`, not the initialization side effects. A final recheck alone does
not serialize the still-running winning initialization. The independent public
runtime follow-up uses real provider construction and returns one success and
one `WYRD_SDK_409_TELEMETRY_PROVIDER_EXISTS`, consistent with the complete source.

**Consequence/reachability:** ordinary two-thread startup on one public shared
state spuriously fails the promised idempotence. No unrelated application
provider is adopted and no tenant mixup is asserted. This is an original-task
gap in a separate Python owner, not a reopening of the now-correct TS
post-await path (FIND-6); assign the next unused ID 8.

**Selected smallest safe correction:** preserve the existing
`OnceLock<Py<PyAny>>` as the sole retained-provider/GC authority. Serialize its
check, fallible Python construction and successful publication using a
per-state initialization gate acquired through the already-installed
`pyo3::sync::MutexExt::lock_py_attached`. Recheck the cell inside that gate.
The extension detaches a blocked interpreter waiter, avoiding a GIL/ordinary
Rust mutex deadlock; there is no new dependency, global lock/registry or identity
lookup. Failed initialization leaves the provider cell empty and retryable;
successful repeats return without installing another provider. Preserve
ImportError/credential propagation and `_start_telemetry`'s other-owner
refusal/loser cleanup. Do not cache a failed result or introduce an adoption
fallback. Keep lifecycle readers and cyclic traversal/clear on the same cell,
without blocking traversal on the initialization gate. Handle gate acquisition
without a new unchecked unwrap/panic path.

**Why revise the proposed PyOnceLock remedy:** pinned PyO3 0.28.3's
`sync/once_lock.rs:53-55` requires a Python token for `get`, while this state's
`__traverse__(&self, PyVisit)` deliberately has none. PyO3
`impl_/pymethods.rs:295-301` forbids safely attaching within traversal; PyVisit
does not expose a token. Merely substituting PyOnceLock would therefore break
the existing GC reader or tempt an unsafe/new dual reference store. The pinned
`sync.rs:291-303,409-435` mutex extension provides the required interpreter-aware
waiting while retaining the correct existing token-free OnceLock reader.
`OnceLockExt` supplies only infallible initialization, so caching a PyErr there
would also weaken retryability. No GC/API redesign is necessary.

**Exact focused closure:** extend existing public Python setup coverage to
contend two same-state calls while the sole initialization is in progress.
Both must return successfully with one installed provider/pipeline; a separate
state/application provider must still be refused and preserved, and the losing
candidate cleanup case stays covered. Include a failed optional-import attempt
followed by a successful retry to prove the new gate does not memoize errors.
Keep GC traversal/clear source coverage and normal lifecycle readers intact.
The race and retry are interpreter-local with no cross-boundary state; the
existing real managed journeys prove normal setup/export, so unit-only
deterministic contention is appropriate. Run:

```sh
mise exec -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q tests/unit/state/test_observe_surface.py::test_telemetry_setup tests/unit/state/test_observe_surface.py::test_telemetry_setup_names_the_otel_extra_without_the_sdk tests/unit/state/test_observe_surface.py::test_application_provider_is_never_flushed_by_state
```

These are prospective commands; rebuilding the Python boundary through existing
`mise run py:setup` precedes execution after a Rust edit. Do not emulate Python
lifetimes inside Rust tests.

## Reconciled original-task obligation matrix

Every one of the original task's 21 IDs is included. PASS is source-supported
under the verification limits below, not a claim that this validator executed
every historical lane. FAIL names only the retained gap, preserving adjacent
working behavior.

| ID | Implementation evidence and remaining boundary | Verification evidence | Result |
|---|---|---|---|
| REQ-010 | Python/TS explicit bridges retain live-ID maps, parent/root/enclosing fallback, start-time context and late allowlisted metadata. Ordinary OTEL IDs/timestamps/status; duplicate/missing/late callbacks harmless; interrupted shutdown ends only mapped spans. Local TS callback inputs keep framework optional. Direct safe Responses usage now retained. | Callback/thread/import tests; fresh Python callback and TS unit file; actual framework edge assertions and clean package checker. | PASS |
| REQ-011 | Generic exporter/correlation paths; native Pydantic and ADK builders, explicit OpenAI bridges, optional Run and independent provider keys; no manual model-span implementation. | Real framework app/journey source; fresh three Python managed cases and TS managed case, historical owned/uncorrelated cases. | PASS |
| REQ-012 | Application-owned composition, retryable Python correlation install, sampler/resources and unrelated exporters retained; TS setup overlap corrected. Python same-state constructor/publication not atomic (FIND-8). | Setup/owned unit and real journeys support passing paths; public-runtime follow-up and source show Python overlap gap. | FAIL: 8 |
| REQ-013 | Both lifecycle owners attempt managed stage before native drain; catalog precedence, failures, active/global survival and native ownership preserved. Original Python bound corrected; old pending result can falsely satisfy later call (FIND-7). | Fresh real-stock deadline and live journeys pass their selected paths; return-to-settlement source/probes falsify remaining success barrier. | FAIL: 7 |
| REQ-014 | Stock OTLP/exporters/context and native fidelity preserved; only optional Run/Card additions; nested/concurrent scope, sampling and version-qualified content disclosures retained. | Existing fidelity/correlation tests and framework edges/privacy source and recorded results; fresh owned correlation test. | PASS |
| REQ-015 | Four concrete procedures, independent keys, ambient state/readback, versions/content/add-vs-replace/provider/failure guidance; current tabs/links/draft flags and lasting design. Missing TS loader peers (FIND-2). | Historical docs generation/check and source construction proof support partial closure; emitted-module/import graph disproves full prerequisites. | FAIL: 2 |
| REQ-016 | Actual three Python/one TS frameworks, local scripted endpoints, real SDK/server/Postgres/readback; success/failure/stream/scope/owned/sampling/content/authorization/tenant assertions; owner and Bifrost lanes collect dependencies without silent skip. Live flush proof now present. | Fresh Python3/TS1 managed journeys; historical J/K matrix, source assertions/selection and supporting runtime tests. Runtime-local overlap corrections may use the explicitly allowed deterministic supporting proof. | PASS |
| REQ-017 | `_session` and TS config use the selected shared client's fresh access token per export; stock http_auth/httpAuth only for Wyrd gateway, independent external keys; isolated ambient test config. | Fresh-token unit owners, gateway/no-credential/upstream-header J5/K5 assertions and historical transport proof. No new auth implementation. | PASS |
| INV-001 | Selected credential owns tenant/publisher; Run/Card remain optional record attributes authorized by unchanged server owners; existing query auth retained. | Security report and matched negative both-tenant journey assertions; no server/auth/storage implementation change. | PASS |
| INV-002 | No new durable protocol, registry/runtime/discovery/patching, Pydantic/ADK adapter, bespoke export/retry queue or Rust OTEL/log/metric redesign. New wait wrapper owns only lifecycle waiting. | Full cumulative write-set and runtime-owner inspection, historical boundary gate. Adding existing peers to docs preserves log/metric behavior. | PASS |
| INV-003 | Enrichment remains fail-open, real model/tool exceptions remain application-owned; bridge/errors use allowlisted metadata, fixed interruption and type-only diagnostics. | Callback/content/error sentinels and real failure assertions; safe direct usage test. | PASS |
| INV-004 | Only managed installed handle retained by state; other owners refused, owned provider untouched, Run has no provider lifecycle. | Setup/owned lifecycle source and tests. Same-state race is idempotence failure, not implicit provider adoption. | PASS |
| AC-001 | Real three-framework Python and TS model/tool spans queried through authenticated server boundary. | Fresh managed journeys plus historical owned framework matrix and concrete assertions. | PASS |
| AC-003 | Formats/types/errors/projections/package/lane/docs owners supplied; generic TS declaration gap corrected; no reported red verification lane is excused. | Historical original/R1 scoped checks plus fresh package/TS unit/Python focused/diff checks. Whole-change shared-infrastructure aggregate remains change-review-owned. | PASS within task proof scope |
| AC-007 | Bridge fidelity/start attribution/optional imports/types and supplied direct usage are supported without missing upstream metadata invention. | Callback/thread/optional import source, fresh callback/TS unit/package checks and historical real edge journeys. | PASS |
| AC-008 | Owned providers preserve settings and correlation; Python same-state overlap can return 409. | Passing normal setup/owned proof plus public overlap counterexample. | FAIL: 8 |
| AC-009 | Live readback, bounded failure/native drain, active/global survival established for ordinary paths; obsolete joined success remains. | Fresh live Python3/TS1 and real-stock deadline test; deterministic source-local overlap evidence. | FAIL: 7 |
| AC-010 | Uncorrelated/nested/concurrent/enclosing/error/stream identities, terminal data and logical no-duplicate assertions implemented in real framework matrix. | J3/K3 source and historical positive counts plus callback regression proof. No arbitrary detached-thread propagation is promised. | PASS |
| AC-011 | Native upstream controls/sampling, payload allowlist and authenticated denied/cross-tenant persistence assertions retained. | J4/K4 exact test bodies and historical results; callback sentinels. Missing supplied usage is fixed, not a redaction claim. | PASS |
| AC-012 | Construction/readback and durable/machine docs align, but TS installation cannot load setup without omitted peers. | Structural docs checks cannot validate runtime import closure. | FAIL: 2 |
| AC-013 | Ambient setup/gateway/no-credential/fresh-token mechanisms reuse merged shared auth. | J5/K5/U5/T5 and recorded existing transport regressions; selected identity retained across flush worker. | PASS |

Scope/non-goals also pass: no new server route/table/migration, public payload,
Card noun, provider runtime, auth/transport owner, production dependency/Cargo
feature or Rust adapter. Python floor/dev-dependency integration was explicitly
owner-directed. Existing draft publication scope is preserved. These facts do
not waive the three retained behavior/procedure gaps.

## Correction reach, verification and limits

The three corrections reach only the existing Observe install command and
Python managed/startup owners with their supporting runtime tests. Reuse the
current state/native drain/error/auth owners and framework/package fixtures.
No additional durable/public/ownership/security/concurrency contract is needed:
the startup correction realizes existing same-state idempotence; the flush
correction realizes existing per-call bounded ForceFlush. General lifecycle
linearizability, arbitrary shutdown-vs-write semantics and cancellation are
outside the recommendations. If implementation instead requires one of those
new contracts, stop for authority rather than silently expanding this packet.

For remediation, exact proof above plus the changed owners' existing
`mise run py:format:check`, `mise run py:lints`, `mise run py:typecheck`,
`mise run fmt`, `mise run lints`, and docs generation/check are sufficient
scoped static lanes. Keep substantive rustdoc on changed private fields and
methods; preserve authored stubs if any signature changes, though none is
selected. No new catalog/API/generated native surface is required. Do not
require full journey suites or repository aggregate as remediation proof;
final integrated selection remains change-review work.

Current `verification.md` records fresh cumulative diff check, three focused
Python tests, five TS unit tests, clean package checker, three real managed
Python framework journeys and one TS managed journey, with traced isolated
Postgres setup/cleanup. Original and R1 format/lint/type/codegen/docs and broader
J/K results remain historical evidence checked against their assertions and
selectors. Green normal-path tests do not invalidate the uncovered return gap,
same-state initialization interleaving or clean recipe prerequisite failure.
The deterministic discovery/follow-up probes use real installed dependencies,
legal scheduling windows and controlled cleanup. I did not duplicate those
executions or claim them as my own.

I used read-only source/Git/CodeGraph inspection and checked the primary OTEL
ForceFlush standard. No build, install, test lane, server operation, credentials,
source/spec/task/index edit, staging, commit, or additional probe was performed.
HEAD was checked as the pinned candidate and tracked/index diffs were empty
immediately before writing this owned report. Only this validation report is
owned by this agent; other agents' reports are preserved. The required report
set is complete, immutable source was available, and no material disagreement
remains unresolved; BLOCKED or SPEC_REVISION_REQUIRED is not supported.

Skipped broad re-verification and implementation changes. Remaining risks are
the TS procedure's missing imports, Python false flush success and Python
same-state startup refusal identified above.
