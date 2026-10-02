# TASK-009 round-six findings validation

## Immutable subject and validation coverage

- Repository: `/home/thorrester/Documents/GitHub/wyrd-verification-closeout`
- Base: `7d96c30066425e0cde2290842d5801307843283d`
- Candidate: `1a4bbff5a26a1462d0f509c4595d52d08fbd25ae`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 46
- Original task: `changes/active/verified-change-contract/tasks/TASK-009-run-context-and-python-otel-correlation.md`
- Remediations: TASK-009-R1, TASK-009-R2, TASK-009-R3, TASK-009-R4E
  (superseding TASK-009-R4), and TASK-009-R5

The candidate matched `HEAD` before validation. The repository has no
`.codegraph/` directory, so direct source and caller inspection was used. All
required round-six discovery reports and the focused follow-up were present and
read. This validation inspected the complete cumulative diff, approved Run and
telemetry authority, the original and remediation tasks, the current provider
registration owner, its public and automatic callers, sibling active-span and
processor paths, focused tests, and prior findings 1 through 13.

## Producer-to-consumer trace

Shared Rust remains the sole Run identity producer. Python `PyRun.__enter__`
passes the immutable view's exact CardRef and Run ID to `_enter_run`, which
best-effort performs three independent operations: install the global
provider's processor, attach the execution-local tuple stack, and stamp an
eligible already-active span. The explicit private-provider hook calls the same
`install_run_correlation` owner directly.

Provider registration weakly publishes a provisional `False` outcome under
`_outcomes_lock`, invokes the duck-typed provider's `add_span_processor`, then
activates that attempt's processor and changes the outcome to `True` only after
normal return. This correctly prevents duplicate handoff and keeps a processor
retained by a failed attempt inert. The remaining defect is that the foreign
callback runs while a non-reentrant `threading.Lock` is held. A same-thread
callback to the supported public installation hook blocks acquiring that same
lock before it can read the already-published provisional result. The outer
callback waits for the nested call, so direct installation and automatic Run
entry never return.

The active-span path is a separate consumer of the scope pair. It does not use
the registered processor and OpenTelemetry's public active-span surface does
not identify the owning provider. The approved Run authority defines active
span stamping and provider installation as separate best-effort entry
operations, while its failed-registration explanation specifically keeps the
processor retained by that failed attempt inert. Gating active-span stamping on
the global provider's Boolean would break the required private-provider case.

## Proposed-finding decisions

### `CONC-R6-001` — CONFIRMED as `FIND-TASK-009-14`

- **Reachability:** `install_run_correlation(provider)` is the approved public
  escape hatch for a framework-owned duck-typed provider, and `_enter_run`
  invokes the same function for the global provider. A provider callback that
  re-enters installation for itself reaches the lock recursively on the same
  thread.
- **Source proof:** `sdks/wyrd-sdk-python/python/wyrd/otel.py:205,275-286`
  holds a non-reentrant `Lock` across `add(processor)`. A bounded fresh-process
  reproduction printed callback entry and timed out with exit 124 before the
  nested installation returned.
- **Sibling impact:** the provisional `False` entry is already sufficient for
  nested installation to return without another provider call. The deadlock is
  caused only by the lock preventing that lookup. Context attach, active-span
  stamping, explicit observations, and failed-processor activation do not own
  this state and need no downstream guard.
- **Ponytail decision:** retain. In the existing registration owner, use the
  standard-library reentrant lock for `_outcomes_lock` while retaining the
  ordinary `Lock` used by `OtelObserver`. Re-entry can then observe and return
  the provisional `False`; the outer call completes, activates its processor,
  and records `True`. This preserves one foreign call, cross-thread
  serialization, weak identity bookkeeping, terminal failure, and healthy
  enrichment. Moving the foreign call outside the critical section would need
  a second in-progress coordination mechanism and is not the smaller safe
  correction.

### `INV-R6-001` — REJECTED

The reported runtime observation is real: `_enter_run` stamps an eligible
already-active span even when installation on the current global provider
returns `False`. It is not a violation of the approved contract.

Revision-46 REQ-151 independently requires entry to stamp an already-active
recording span when it lacks `wyrd.card_ref`. The changed Run authority at
`changes/active/verified-change-contract/architecture/logic/run_api.md:140-164`
separates that operation from provider installation and explains the failed
provider's no-enrichment result through the registration-owned processor it may
have retained staying inert. The public active-span API supplies no provider
identity, and the active span may belong to a successfully installed private
provider while global installation fails. The proposed global-result guard
would therefore remove explicitly required private-provider correlation.

No correction belongs in this review. Making a failed global provider's own
active span distinguishable would require new provider-identification behavior
and a specification decision; current approved behavior does not require it.

## Prior-finding closure

| Prior finding | Independent closure evidence | Result |
|---|---|---|
| `FIND-TASK-009-1` | Optional package, provider, context, processor, and span failures remain contained while explicit observation behavior stays strict. | CLOSED |
| `FIND-TASK-009-2` | `Run::subject` documents root, initially selected, and sibling-selected immutable views. | CLOSED |
| `FIND-TASK-009-3` | Revision 46 supersedes token/detach recovery with the approved attach-only tuple stack. | SUPERSEDED AND CLOSED |
| `FIND-TASK-009-4` | The private OpenTelemetry context key is created once at module import. | CLOSED |
| `FIND-TASK-009-5` | PyO3 exit names/defaults and owning/generated declarations agree. | CLOSED |
| `FIND-TASK-009-6` | Both shared Rust tests contain the required panic documentation. | CLOSED |
| `FIND-TASK-009-7` | Deterministic coverage proves two tasks may enter the identical immutable Run and exit independently. | CLOSED |
| `FIND-TASK-009-8` | A provider outcome is published before its one foreign call, so an accept-then-raise provider is not retried. | CLOSED |
| `FIND-TASK-009-9` | Outcomes use weak referent identity, so distinct equal providers register independently. | CLOSED |
| `FIND-TASK-009-10` | Each attempt owns an inactive processor that becomes active only after normal provider return; a failed retained processor cannot enrich. `INV-R6-001` does not reopen this processor-specific finding. | CLOSED |
| `FIND-TASK-009-11` | The changed Run authority identifies approved specification revision 46. | CLOSED |
| `FIND-TASK-009-12` | Replaced TASK-009-R4 is machine-readably `superseded` by R4E. | CLOSED |
| `FIND-TASK-009-13` | The original task records all three exact repository-native Rust commands and zero-exit evidence. | CLOSED |

## Final deduplicated finding ledger

| ID | Discovery source IDs | Status | Classification | Violated obligation | Exact location | Evidence and observable consequence | Decision-complete correction | Focused closure proof |
|---|---|---|---|---|---|---|---|---|
| `FIND-TASK-009-14` | `CONC-R6-001`; focused follow-up resolution 1 | CONFIRMED | INCORRECT | Revision-46 REQ-151 and the Run authority require optional provider integration to be fail-open, thread-safe, idempotent, and attempted at most once per provider without blocking Run entry or application execution. | `sdks/wyrd-sdk-python/python/wyrd/otel.py:13,205,275-286`; callers at `sdks/wyrd-sdk-python/python/wyrd/otel.py:293-310` and `sdks/wyrd-sdk-python/src/observe/mod.rs:195-217` | `_outcomes_lock` is a non-reentrant `Lock` held across the foreign `add_span_processor` call. Same-thread callback re-entry blocks before reading the provisional outcome; a bounded reproduction timed out with exit 124. Direct private installation hangs, and the same provider returned globally hangs `Run.__enter__`, so optional telemetry can stop application execution. | Keep the existing critical section and outcome owner; change only `_outcomes_lock` to a standard-library `RLock` (importing it alongside the still-needed ordinary `Lock`). Preserve provisional `False` publication, one provider call, per-attempt activation only after normal return, final `True`, weak identity/lifetime, cross-thread serialization, and terminal failed outcomes. Add no callback wrapper, retry, timeout, second registry, condition state, dependency, or consumer guard. | Add one focused bounded-subprocess case with a weak-referenceable provider whose `add_span_processor` re-enters `install_run_correlation(self)`. Prove the nested call returns the provisional `False`, the provider receives exactly one processor, the outer and later calls return `True`, and the activated processor applies the exact in-scope pair. The subprocess timeout must turn a lock regression into a test failure rather than hanging the suite. Run that exact case and the complete focused Python surface file. |

## Validation result

One bounded implementation defect remains. `FIND-TASK-009-14` is local to the
existing Python registration owner and its minimum correction is fully
determined by approved revision 46; it requires no new public API, dependency,
provider lifecycle, concurrency semantics, persistent state, or specification
revision.

`INV-R6-001` is omitted from the ledger because current authority requires the
provider-agnostic active-span operation independently of global registration.

**Validated ledger result: one retained finding, `FIND-TASK-009-14`.**
