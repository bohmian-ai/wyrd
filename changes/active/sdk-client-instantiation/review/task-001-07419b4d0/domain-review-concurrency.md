# Concurrency and foreign-runtime domain re-review

**Result: FAIL — the runtime boundary passes source review; one previously validated native API documentation obligation remains open.**

Subject: cumulative `2f1db2194f5630df850b10987fe0f360946a525c..07419b4d060303959384b682ca88e8673c722e63`, with `fde7d396a..07419b4d0` used for closure navigation. HEAD remains the candidate; tracked source and index remain unchanged. This is domain discovery, not a task completion verdict.

## Boundary, authority and source coverage

Reviewed spec revision 11, the original base TASK-001, the shared immutable subject, prior concurrency discovery and independent findings validation. Applied REQ-003/005–009/014–015, INV-001–005, AC-002/004–005/007, AGENTS §§5–8/11 and mandatory changed-item rustdoc, agent rules, spec-driven-development and maintainer guidance, Wyrd Client model/doctrine public surfaces, Bifrost public composition/admission, and PyO3 boundary rules. CodeGraph preceded source lookup; focused source/diff inspection expanded its results. No other current-round discovery report was read.

| Reviewed boundary | Complete affected owner/caller coverage and assessment |
| --- | --- |
| Python initialization and publication | `src/client.rs:75–145`, including constructor, noninitializing accessor, double-checked first-use gate, shared runtime and failed-attempt return; all lazy consumers in Bifrost, Cards, Gateway, Principals and OperatorConnections. `WyrdClient::connect_ambient/connected`, shared handle factories and Bifrost connect/assemble retain the existing client/cache/transport owners. |
| Bifrost local-to-native binding | `src/bifrost/mod.rs:529–604,609–750,927–947`; local `Binding`, native connection publication, handoff, local table mutation/read and counters, eager factory and all server-operation access through `handle`. Shared `facade.rs:147–154,220–245,270–332,553–609` owns final binding and writer operations. |
| Cards derived lifetime and identity | `src/state/mod.rs:1651–1794,2047–2076,2390–3025`; parent and Data/Model/Prompt/Workflow views retain `SharedCards(Arc<LazyConnection<Cards>>)`. Every view server operation uses that shared initializer; no connected clone is created during view selection. Existing registration/download/list/resolve/delete/workflow helpers still own their original work. |
| Python GIL, async and native lifetime | `src/trace.rs:86–104`, `LazyConnection::get`, and facade `_client.py:638–750,760–794,1040–1203` and remaining async methods. Constructor/factory parameters become Rust values; asynchronous Python calls use the existing `asyncio.to_thread` bridge. PyO3 0.28 and shared Tokio 1.49 runtime boundaries remain unchanged. |
| N-API factory and operation lifetime | Cumulative changed factories in native `cards.rs`, `gateway.rs`, `principals.rs`, `operators.rs`, `client.rs`, new listing in `lib.rs`, TypeScript callers, and `trace.rs::spawn`. Futures own cloned clients/Bifrost Arc and converted arguments; `spawn` requires `Send + 'static`. No borrowed Env, JavaScript value or runtime object enters those futures. |
| Runtime proof inspected | Python instantiation unit/integration modules, public Bifrost wrappers, and task evidence claims. Unit local access runs on the async event loop without credentials; integration retains a preselected binding/view across first connection and subsequent ambient mutation; eager AsyncBifrost now makes a real no-client list call in both ambient scenarios. |

## Prior finding closure and interleaving analysis

**FIND-TASK-001-1 / local portion of CONCURRENCY-001: CLOSED in source.** `use_table` and `table` inspect `Binding`, and counters call only `LazyConnection::connected`; none initializes. Cards getters clone only the Arc-backed owner. The source-selected unit test now uses actual listing as first use, and explicitly verifies local binding/counter/view access without credentials.

The new Bifrost handoff does not lose concurrent local mutations. Connection runs without the binding lock. A simultaneous `use_table` either replaces the local value before handoff, which the first successful caller moves, or observes `Native` afterward and mutates the native owner. Handoff takes the same binding mutex and sets `Native` only after `get` has returned a published native handle (`mod.rs:580–587`). Every server operation enters through this handoff before using the handle. A second first-use caller may win the handoff after connection publication; that still moves the one latest local binding before either caller proceeds. Local readers see the old local or final native value under that same mutex; no partially moved state escapes. Native active-table locking does not span network awaits, and none of the inspected paths takes the Python binding mutex while holding the native active mutex in reverse order.

**Once-only success, failure and retry: PASS in source.** The connection gate is acquired inside `py.detach`, rechecks OnceLock, publishes only success and is dropped before interpreter reattachment (`client.rs:132–144`). Failed initialization leaves both OnceLock and local binding unchanged; later calls retry with the retained explicit client or the same ambient path. Local mutation can proceed during that failed attempt. Successful operations never clear or replace the retained native handle; shared auth retains renewal through its existing Arc owner.

**Cards view identity/lifetime: PASS in source.** Views survive parent collection through the Arc; parent and every pre/postconnection view share one gate and one OnceLock. Simultaneous first use cannot choose separate ambient clients. The existing shared Cards engine remains the connected capability; this small runtime wrapper does not duplicate registry or auth behavior.

**Rejected initial-insert redesign remains rejected.** Prior validation explicitly permits first synchronous insert to perform shared connection setup. It does not authorize accepting rows before connection, a second buffer, an awaitable insert, a new queue/lifecycle owner or stronger cancellation semantics. After setup, native insert still delegates to the same bounded WriterPool. Public facade prose now correctly qualifies first-use blocking and errors. No event-loop-heartbeat test or queue redesign is requested.

**Async and resources: PASS in source.** `AsyncBifrost()` performs no connection work; eager connect and awaited IO methods run native work in a worker thread. GIL detachment is used for blocking initialization, not confused with event-loop scheduling. Cancelling a `to_thread` await still cannot stop its already-running worker, an existing bridge property; bound methods retain their native object while running. Initialized Bifrost retains one writer/transport owner and existing flush/shutdown behavior. No new accepted-write loss or Node/Python lifetime defect was established.

## Material proposed finding

### CONCURRENCY-R2-001 — prior FIND-TASK-001-4 remains partially open

- **Classification:** VIOLATION, required changed-item operation/failure documentation; no new runtime defect claimed.
- **Exact location:** `sdks/wyrd-sdk-python/src/bifrost/mod.rs:729–737` (`Bifrost::insert` rustdoc), with executing initialization at `:747`.
- **Violated obligation:** The prior validated FIND-4 explicitly requires native `insert` to qualify its bounded-send description with initial lazy setup and connection-error timing. AGENTS/agent-rules require accurate operation/side-effect and error rustdoc for materially changed Rust items; REQ-007/008 permit this initial connection behavior.
- **Evidence and consequence:** Native prose still says unconditionally “Non-blocking” and lists only validation/no-table/queue errors. The body calls `self.handle(py)?`, which synchronously drives shared credential exchange and persistent transport establishment on first use. Native API maintainers are still given the wrong scheduling/failure contract; the corrected public Python prose does not fix this explicitly identified native item.
- **Minimum correction:** Qualify only this native rustdoc: bounded/nonblocking after connection, first lazy insert may block for shared connection and return that path's existing errors. Preserve executable code, queue ownership, backpressure and flush/shutdown semantics. This is continuation of FIND-4, not a new finding ID or reason to reopen rejected buffering proposals.
- **Focused closure:** Source-compare the final doc with `insert -> handle -> LazyConnection::get -> shared connect`. No runtime test or new checker is needed solely for this prose correction.

## Verification limits

Static review only: no tests, builds, Cargo commands, formatters, generators or runtime test subprocesses executed. Appendix PASS labels were treated as claims; raw transcripts were not supplied. Inspected tests cover local access and sequential handoff/retention but do not deterministically exercise simultaneous first-use, failure/retry or cancellation. Their absence alone creates no additional finding or obligation to add concurrency machinery.

Skipped unrelated server/storage behavior and stress execution. Residual risk is unexecuted interleaving/runtime verification; source review established no additional reachable concurrency or foreign-runtime regression.
