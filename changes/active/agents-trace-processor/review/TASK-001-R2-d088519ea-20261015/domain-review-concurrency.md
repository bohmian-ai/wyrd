# Concurrency domain review — TASK-001 R2

Result: **PASS**. Proposed finding ledger: **empty**.

Subject: original base `dac4c340f835676adc29f3dcc01f98e1d3d51465`, candidate
`d088519ea846aba8b2a77c24b2af2491ad2b3936`. The full cumulative diff was
the subject; `1b769759d..d088519ea` was navigation for the latest correction,
not a replacement base. Historical findings 5–8 were hypotheses, not closure
authority. No current peer discovery report was read.

## Boundary and authority

Reviewed Python managed setup/flush and PyO3 lifetime ownership; TypeScript
same-state startup, lifecycle sequencing and bridge callback ownership; Run
correlation under threads/async execution; native writer lifecycle; and imported
main's telemetry routing/identity seams. Applied approved spec r2 REQ-010,
REQ-012–014, REQ-016, INV-002–004, AC-007–010 and original TASK-001. Applied
AGENTS.md, agent rules, reference router, spec-driven development, maintainer
style, PyO3 boundaries, TypeScript guide, testing workflows and telemetry
observations references; active design's managed telemetry and observation
identity contracts and doctrine's client/server ownership boundary.

CodeGraph was consulted before code search. Full bodies of the relevant
owners, callers and sibling entry points below were inspected. Locked Python
OTEL 1.42.1 and installed PyO3 0.28.3 mutex implementation were inspected to
check the claimed native behavior rather than infer it from the wrapper.

## Coverage and conclusions

| Obligation / seam | Source and caller trace | Assessment |
|---|---|---|
| Effective Python caller deadline; no accumulating export workers | `python/wyrd/otel.py:215–270`, `_ManagedTelemetry.force_flush/_flush`; constructed only by `_start_telemetry`; consumed by `src/state/mod.rs:525` | PASS. One monotonic deadline includes waiting for an obsolete attempt and waiting for a covering attempt. The lock guards attempt selection and launch; actual flush and waiting are outside it. At most one unfinished Future/worker exists. No worker join, executor, extra export queue or retry policy is introduced. |
| Later calls cannot accept obsolete success | `_ManagedTelemetry.force_flush`, `_started`, `entered`, `covers` | PASS. A call records the started-operation count at entry. An already-running operation cannot cover it; it waits for settlement then obtains a newly started operation, or a concurrent caller's operation numbered after its entry. Thus a span ended after an old drain but before this call is not dismissed by that old success. Each worker settles its own explicit Future. A covering false/exception is returned unchanged; an obsolete attempt's failure is not substituted for the new call's operation. |
| Same-state Python setup, retry and GIL | `PyWyrdState` fields/constructor; `start_telemetry:191–219`; PyO3 0.28.3 `sync.rs::MutexExt::lock_py_attached` | PASS. Fast check and check under the per-state setup gate prevent duplicate installation. Contended acquisition detaches from Python, so a waiting caller cannot hold the GIL against the installer. Guard release on ordinary error leaves setup retryable. Existing shared client resolution remains the identity owner. The added mutex stores no Python references. |
| Readers and cyclic GC do not invert setup locking | `flush_telemetry`, `__traverse__:484`, `__clear__:507` | PASS. Published provider remains in `OnceLock<Py<PyAny>>`; flush and traversal only read that cell, without entering the setup mutex. Clear uses exclusive mutable access and takes the cell. There is no GC traversal waiting for a setup lock while the installer requires Python. No borrowed Python object crosses an await. |
| Cross-state/application provider ownership and loser release | Python `_start_telemetry:273–313`; TS `src/otel.ts:96–109`; native state setup callers | PASS. Global installation uses OTEL's once-only ownership. Python checks the actual installed provider after installation and releases a loser. TS checks `setGlobalTracerProvider`'s result and shuts down only its refused provider. Neither path silently adopts another identity. |
| TS overlapping setup | `src/index.ts:2924–2940` → synchronous `src/otel.ts::startTelemetry` | PASS. The second field check follows the only await. Check, installation and publication contain no suspension, so another same-state continuation observes publication and returns. No Promise registry or additional lifecycle state is needed. Other states still reach global refusal. |
| Managed flush before native drain, including failure | Python `flush/shutdown:295–320`, `flush_telemetry`; TS `#flushTelemetry/#settle:2727–2744`, `flush/shutdown:2971–2995`; native `cards.rs:548–586`; shared `state.rs::TelemetryFlushFailure` | PASS. Both public lifecycle methods capture provider outcome first and then attempt the existing native drain. Timeout/false and other exceptions use the shared settlement owner; native-only errors retain their codes, combined errors retain native code secondarily. Application-owned providers have no retained managed handle. State shutdown does not shut down the global provider. |
| Native writer lifecycle remains native | shared `state.rs:689–705` → `observe/lifecycle.rs::started/shutdown` → existing `Bifrost.flush/shutdown` | PASS. No runtime glue replaces writer startup/admission/drain/closed-state ownership. A failed native shutdown keeps the existing writer for retry; successful shutdown remains terminal. The telemetry deadline is not falsely presented as a total native-drain deadline. |
| Callback map ownership and start-time correlation | Python `WyrdAgentsTraceProcessor:377–450`; TS `WyrdAgentsTraceProcessor:2552–2630`; Python Run context helpers and `_RunCorrelationProcessor`; TS `Run.scope` and `runCorrelation` | PASS. Python admission/parent selection/start happen under one map lock; end removes exactly one retained span and performs terminal mutations outside the lock. Shutdown closes admission and removes remaining spans atomically. TS transitions have no await despite async callback signatures. Correlation is captured through the original start context, with no completion-time Run lookup. Duplicate ends/starts and late callbacks cannot replace live ownership. |
| Scope isolation and ordinary callbacks during blocked export | Python `_scope_stack/_enter_run/_exit_run`, `install_run_correlation`; TS `Run.scope` and AsyncLocalStorage; installed OTEL batch processor | PASS. Scope is context-local, with immutable Python stack values and TS scoped context. Only explicit provider flush runs on the daemon worker. Stock batch `emit` queues spans without acquiring its export lock, so the held export does not become a shared callback scheduler or stop start/end correlation. Registration's existing lock preserves one processor and retry after failed add. |
| Imported main telemetry attribution does not mutate sibling routes | `wyrd-telemetry/src/export.rs::TenantRoutes::{start,end,bind,rebind,slot}`, `bind_operation_route`; `wyrd-runtime/src/request_context.rs` | PASS at the merge seam. Rebind replaces only the current live span's route slot under the registry lock, with the same sink and immutable new identity. Later children inherit that slot; request/sibling slots retain their original shared route. End removes its own entry. The imported subject/actor pair is not a process-global current identity. |
| Imported ingest/query changes preserve concurrency boundary | cumulative changes to Scribe `DecodeContext/ManagedValues/managed_arrays`, shared client Table description and observation/workflow correlation, server/test fixture callers | PASS at the seam. Attribution values are carried per request/batch and stamped into managed columns, while optional Card/Run remains per record. They introduce no new client telemetry lifecycle, mutable global credential/correlation state or extra setup path. This is not a whole-main audit of unrelated verification/storage work. |

## Prior findings and proof assessment

| Prior ID | Independent source assessment |
|---|---|
| FIND-TASK-001-5 | Closed: actual stock batch `force_flush` at installed `_shared_internal/__init__.py:240–246` still ignores its argument and exports synchronously. The daemon/Future boundary now bounds the waiting caller without canceling or multiplying that operation. The native drain follows timeout. |
| FIND-TASK-001-6 | Closed: TS post-import recheck prevents the formerly reachable same-state loser, while preserving other-state collision refusal and loser cleanup. |
| FIND-TASK-001-7 | Closed: operation-start counting prevents accepting an obsolete in-flight flush as proof for a later call's already-completed spans. A single deadline prevents the old-wait plus new-wait sequence doubling the wait allowance. |
| FIND-TASK-001-8 | Closed: PyO3-aware same-state setup serialization covers check/install/publication and leaves the independent published cell available to GC/readers. It uses the installed standard PyO3 mutex mechanism rather than ad hoc GIL handling. |

Inspected supporting proof: Python `test_telemetry_setup` overlap/repeat/other
state/application/lost-race cases; failed-optional-import and correlation retry
cases; managed lifecycle outcome cases; `test_managed_telemetry_flush_deadline`
with real stock batch processor, held exporter, two bounded waits and one worker;
`_stale_flush` for later flush and shutdown with a newly ended span after the
earlier drain; callback thread-safety, explicit context propagation, nested and
concurrent async scope tests. TS setup uses overlapping Promise calls; lifecycle
tests preserve active spans and global provider; owned-provider tests exercise
parallel/nested scopes. Real framework journeys cover concurrent/nested activity
and live flush/provider continuity. Historical execution records are claims,
not fresh runs by this reviewer; root verification is reported separately.

The standard/native alternatives were checked: the installed Python batch
processor cannot itself enforce the required effective wait bound; a shared
Future and one daemon are a narrow boundary around that stock owner, not an
exporter replacement. The setup gate uses PyO3's native attachment-aware lock.
TS's synchronous post-await sequence already supplies sufficient serialization.
No additional queue, state machine, cancellation API or shutdown protocol is
required by the approved contract. No correction is proposed.

## Limits and final guard

No source changes, installs, builds, tests or isolated runtime probes were
performed. This review does not require arbitrary-thread context propagation,
shutdown joining a callback already removed for completion, cancellation of an
overrunning stock export, or linearizability of setup racing shutdown: those
would add concurrency semantics beyond the approved task. Abrupt process exit
can still lose background export, as with the existing OTEL pipeline.

HEAD was checked as the pinned candidate before review and again before report
completion. Only this assigned report was written by this reviewer.

**PASS** within the reviewed concurrency boundary.

Skipped fresh execution; root owns verification. Risk: a timed-out stock flush
continues best effort in the background and is not canceled by state shutdown.
