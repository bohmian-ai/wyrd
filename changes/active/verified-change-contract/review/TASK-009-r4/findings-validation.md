# TASK-009 round 4 findings validation

## Immutable subject and coverage

- Repository: `/home/thorrester/Documents/GitHub/wyrd-verification-closeout`
- Base: `7d96c30066425e0cde2290842d5801307843283d`
- Candidate: `017a54d5390eb488e820f890bbb953a5f1ca3a53`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 45
- Original task: `changes/active/verified-change-contract/tasks/TASK-009-run-context-and-python-otel-correlation.md`
- Prior remediations: TASK-009-R1, TASK-009-R2, and TASK-009-R3 in their respective review directories

The candidate matched `HEAD` before and after source inspection, and the
reviewed `otel.py` blob remained
`da16b02bcb014e9654ba3d7a325f9a3b967a563a`. There is no `.codegraph/`
directory, so repository search and direct source inspection were used. All
required round-four discovery reports and the focused follow-up were present
and read. The complete cumulative diff, applicable repository and Run API
authority, prior validated ledgers, and remediation tasks were inspected. No
intended verdict was supplied.

## Producer-to-consumer trace

Shared Rust remains the sole Run identity producer. `WyrdState::run` and
`run_for_card` select the root or hydrated alias before `Run::new` mints one
UUIDv7 invocation ID; immutable `Run::for_card` views preserve that ID and
replace only the exact subject. Rust, Python, and TypeScript project those
owners without adding another Run, writer, or lookup implementation.

For Python telemetry, `PyRun.__enter__` calls `wyrd.otel._enter_run`, which
calls `install_run_correlation()` for the current global provider and then
attaches `(card_ref, run_id)` to execution-local OTel context. The public
`install_run_correlation(provider)` escape hatch reaches the same registration
owner for an explicit private provider. `_RunCorrelationProcessor.on_start`
is the only downstream span-enrichment consumer; the authenticated OTLP
journey then exercises the existing Gate/Scribe/Bifrost sinks. Explicit
Drift, Eval, and dataset observations remain sibling consumers of the shared
Run and state-held Bifrost writer, independent of processor registration.

Provider registration is serialized at
`sdks/wyrd-sdk-python/python/wyrd/otel.py:246-276`. The owner accepts any
weak-referenceable object with a callable `add_span_processor`, matching the
approved duck-typed private-provider boundary. It records the provider before
calling that foreign method but removes the mark when the method raises.
Because a foreign call may perform its side effect before raising, the removal
turns an ambiguous first attempt into another processor handoff on the next
explicit call or Run entry. No consumer can determine whether the first
processor was retained, and the span processor itself cannot deduplicate its
registration after insertion.

The sibling paths do not close this state-transition gap. A stock OTel 1.42.1
provider normally appends without a later exception, so the healthy provider
tests and authenticated journey correctly pass. The current raising test at
`test_observe_surface.py:417-427` raises before retaining anything, while the
Run-entry failure test at `:478-493` counts attempts but obtains a new provider
instance on each global lookup. Neither exercises the same supported provider
retaining a processor and then raising. A direct, source-free reproduction
with one such duck-typed provider returned `False` twice while its retained
Wyrd processor count grew from one to two.

## Proposed-finding decisions

### `INV-REV-R4-001` — REVISED

- **Reachability and obligation:** REQ-151 requires one idempotently registered
  processor and an idempotent private-provider hook. AC-032 requires provider
  registration to be idempotent. TASK-009 Scenario 3 makes the observable
  bound explicit: each global or explicitly supplied private provider receives
  at most one Wyrd processor. `run_api.md` expressly says the optional helper
  duck-types private-provider registration, so a wrapper that delegates to a
  stock provider, retains the processor, and then fails a post-registration
  hook is within the implemented and approved boundary rather than a
  speculative stub.
- **Source evidence:** `install_run_correlation` at
  `sdks/wyrd-sdk-python/python/wyrd/otel.py:246-276` discards the only
  per-provider mark on an exception. `_enter_run` at `:279-297` makes a later
  attempt reachable from every entry. The helper's docstring promises
  thread-safe, idempotent per-provider behavior and a truthful Boolean result.
- **Failure trace:** the foreign provider retains the new
  `_RunCorrelationProcessor`, then raises; Wyrd discards the mark and returns
  `False`; a later call with that same provider constructs and hands over a
  second processor. Both processors receive every later span-start callback.
- **Conflict resolution:** the behavior, system, telemetry, and concurrency
  reports established only stock-success or raises-before-retention behavior.
  Their broad PASS conclusions do not disprove this distinct reachable
  transition. The focused follow-up correctly resolves the conflict in favor
  of the invariant review. Its suggested correction is narrowed below so the
  public Boolean remains accurate after both successful and failed attempts.
- **Ponytail decision:** retain as new `FIND-TASK-009-8`. Retrying an ambiguous
  foreign side effect is unnecessary and conflicts with the explicit
  at-most-one bound; TASK-009 already rejects a telemetry retry mechanism.
  Existing weak per-provider bookkeeping and the existing lock are sufficient.
  No provider wrapper, new dependency, warning, lifecycle ownership, or second
  registration mechanism is justified.

## Prior-finding closure

| Prior finding | Independent closure evidence | Result |
|---|---|---|
| `FIND-TASK-009-1` | Missing/API-only, actual raising registration, attach, enrichment, and detach failures reach the ordinary explicit Drift boundary; user exceptions remain unchanged. | CLOSED |
| `FIND-TASK-009-2` | `Run::subject` now documents root, initially selected, and sibling-selected views consistently with both constructors. | CLOSED |
| `FIND-TASK-009-3` | Entry records exact token/prior pairs and exit restores the prior value after raising or swallowed detach failure; the focused test proves nested restoration and final clearing. | CLOSED |
| `FIND-TASK-009-4` | `_key` double-checks under the existing lock, and deterministic concurrent first entry proves one published key and exact per-scope correlation. | CLOSED |
| `FIND-TASK-009-5` | PyO3 names/defaults, owning and generated stubs, and public conventional-keyword and omitted-argument calls agree. | CLOSED |
| `FIND-TASK-009-6` | Both new shared Rust tests contain accurate `# Panics` sections without executable changes. | CLOSED |
| `FIND-TASK-009-7` | Two coordinated asyncio tasks enter the identical `Run`, interleave exits, preserve the active task's exact pair, and leave exited contexts uncorrelated. | CLOSED |

The retained finding is not a reopening of the prior failure-proof finding.
`FIND-TASK-009-1` proved that registration exceptions remain fail-open for
application and explicit-observation behavior. `FIND-TASK-009-8` concerns the
separate producer invariant after that foreign call has an ambiguous side
effect: the same provider must not receive another processor.

## Final deduplicated finding ledger

| ID | Discovery source IDs | Status | Classification | Violated obligation | Exact location | Evidence | Observable consequence | Decision-complete correction | Focused closure proof |
|---|---|---|---|---|---|---|---|---|---|
| `FIND-TASK-009-8` | `INV-REV-R4-001`; round-four focused follow-up | REVISED | INCORRECT | REQ-151, AC-032, TASK-009 Scenario 3, and the approved Run API require thread-safe, idempotent registration and at most one Wyrd processor per global or explicit private provider. | `sdks/wyrd-sdk-python/python/wyrd/otel.py:246-276`, reached from `_enter_run` at `:279-297`; incomplete proof at `sdks/wyrd-sdk-python/tests/unit/state/test_observe_surface.py:392-427,469-493` | The owner removes the provider's mark whenever `add_span_processor` raises, although the foreign method may already have retained the processor. Repeating installation on the same provider reproduces two retained `_RunCorrelationProcessor` instances. Current tests cover success, unsupported providers, and raises-before-retention or distinct provider objects, not this ambiguous same-provider transition. | Keep one weak, lock-protected per-provider registration outcome in the existing owner, distinguishing successful installation from a terminal failed/ambiguous attempt. Mark the attempt before invoking the foreign method; change it to success only when the call returns. Later calls for a successful provider return `True`; later calls for a failed/ambiguous provider return `False` without handing over another processor. Preserve fail-open Run entry, explicit observations, weak provider lifetime, optional OTel imports, and caller-owned provider lifecycle. Add no retry, warning, wrapper, dependency, or second registry. | In the existing provider-registration test home, use one weak-referenceable provider object whose method retains the supplied processor and then raises. Call installation once directly and again through a Run entry (with global lookup returning that exact object), assert application/user-exception and representative explicit-observation behavior remain unchanged, and assert the provider received exactly one Wyrd processor. Also prove repeated direct calls return `False` for a raises-before-retention provider without another call, while existing healthy global/private providers still return `True`, receive one processor, and enrich spans. Run the exact focused cases and the complete `test_observe_surface.py`. |

## Validation result

The reproduction above directly exercised the retained defect. The round-four
focused Python suite and shared Rust tests reported by discovery remain useful
evidence for every other obligation, and prior remediation records provide the
broader authenticated journey, SDK parity, generation, boundary, format, and
lint results. Those green paths cannot establish the missing ambiguous
registration transition.

The correction stays within the existing private Python OTel owner and approved
behavior. It requires no new product, public API, architecture, security,
compatibility, cross-service, concurrency-semantics, resource-ownership, or
persistent-data decision.

**Validated ledger result: one retained finding, `FIND-TASK-009-8`.**
