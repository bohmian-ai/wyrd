---
id: TASK-001-R2
kind: remediation
status: proposed
spec: SPEC-agents-trace-processor
spec_revision: 2
parent_task: TASK-001
remediates:
  - FIND-TASK-001-2
  - FIND-TASK-001-7
  - FIND-TASK-001-8
depends_on: []
---

# Close Python telemetry overlap gaps and TS installation prerequisites

Implement this single bounded remediation directly with `$wyrd-implement` under
approved revision 2. No new specification or planning gate is required. This
packet diagnoses and selects the correction boundaries; reversible local code
and fixture choices remain implementation-owned. Do not implement unrelated
review suggestions or treat implementation as review approval.

## Authority and immutable review inputs

Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/split/agents-trace-processor`.

- Approved [specification r2](../../spec.md).
- [Original TASK-001](../../tasks/TASK-001-framework-agnostic-telemetry.md).
- Original base: `dac4c340f835676adc29f3dcc01f98e1d3d51465`.
- Prior reviewed candidate: `692d390dc40e43c6e1d7e25c7850d5c826d26a9e`.
- Current reviewed candidate: `295446005b18604030bf376de6209098a28886f7`.
- [Prior verdict](../TASK-001-r2-692d390dc-20261015/verdict.md) and
  [R1 task/evidence](../TASK-001-r2-692d390dc-20261015/TASK-001-R1-telemetry-contract-closure.md).
- Current [verdict](verdict.md), [independently validated ledger](findings-validation.md),
  [focused follow-up](followup-review.md), and [verification](verification.md).

Follow repository AGENTS.md, agent rules, active design/doctrine, applicable
language/runtime/testing references and docsite CONTRIBUTING/PRODUCT. The
reviewed source is immutable during review; implementation starts afterward.
The cumulative original task remains the acceptance scope after remediation.

## FIND-TASK-001-2: complete the TS recipe's actual prerequisites

**Obligation:** REQ-015 / AC-012 require executable framework procedures with
complete installation prerequisites.

The Observe recipe at
`docs/src/content/docs/build-and-verify/observe/record-a-run-and-observations.md:168`
installs the trace exporter but omits
`@opentelemetry/exporter-logs-otlp-proto` and
`@opentelemetry/exporter-metrics-otlp-proto`. Both are unconditional imports of
`sdks/wyrd-sdk-ts/wyrd/src/otel.ts:17-18`, retained in its emitted module.
`startTelemetry()` imports that module, so a clean documented installation
fails module resolution before traces can export. Optional peer metadata does
not install these packages. Defined example objects and green structural docs
checks fix the earlier construction problem but cannot prove dependency closure;
the isolated module probe and static graph show the remaining failure.

**Selected correction:** add those two existing optional exporter peers to the
existing TS installation command. Retain the current exporter implementation,
optional production peer policy, framework construction/readback, independent
model-provider credentials and ambient Wyrd credentials. Removing existing
imports would break log/metric factories; changing module loading or making all
peers mandatory expands scope. No new checker or permanent test harness is
needed for this documentation correction.

**Closure:** reconcile the corrected command with all direct external imports
of the OTEL entrypoint, then reproduce isolated module loading with exactly the
documented dependency set using existing packages and clean temporary work.
The module must load without unrelated development peers. Generate/check docs
under current conventions, preserving tabs, links, frontmatter and draft flags.
Record the actual probe command/result rather than claiming docs checks prove
runtime installation. Preserve generated index/drift requirements; staging and
commits remain caller-owned.

## FIND-TASK-001-7: make each successful Python flush cover its own call

**Obligation:** REQ-013 / AC-009 require bounded flushing of completed managed
spans before native drain. Stock OTEL ForceFlush is the underlying operation.

The producer of false success is `_ManagedTelemetry.force_flush` in
`sdks/wyrd-sdk-python/python/wyrd/otel.py:236-252`. It joins any pending `_attempt`
and returns that attempt's result. Stock provider flushing can finish its final
queue drain, then yield before Wyrd settles its Future. Another thread can end
a span and invoke public `state.flush()` or `state.shutdown()` during that
interval. That later invocation receives the old True without a new provider
operation, leaving a span completed before its own entry queued. Native drain
cannot drain the OTEL queue; successful shutdown followed by exit can lose the
evidence. Both consumers share `flush_telemetry` in native `state/mod.rs:508-524`.

The real held-export deadline test correctly proves R1's bounded wait, but
queues its extra work before stock batching's final drain, missing this later
return-to-settlement interval. Discovery and fresh follow-up reproduce the gap
with the real stock provider, batch processor and exporter. This is new
FIND-7; original FIND-5's unbounded synchronous wait is closed. The reported
implementation limitation about returning an older result is not an approved
contract amendment.

**Selected correction:** remove obsolete-result coalescing as a successful
barrier. Reuse this managed owner's standard-library synchronization to
serialize fresh stock provider flush operations. Every successful invocation
must obtain a provider flush begun after that invocation's entry. Waiting for
an earlier operation and obtaining this call's result share one monotonic
absolute deadline with the existing SDK bound. If the earlier operation uses
the available budget, return the existing 504; otherwise use only the remaining
budget for a fresh covering operation. Keep at most one in-flight worker,
background completion after timeout and outcome identity tied to its actual
attempt. Do not enqueue pending operations, restart a full deadline, add span
sequence tracking, cancel stock exports or wait on an unfinished worker during
timeout cleanup.

Fix this at the existing managed owner once for flush and shutdown. Keep stock
batching, export/token refresh and callbacks, synchronous Run correlation,
application-owned provider isolation, active-span survival, native drain on
failure and existing error precedence. Keep provider flush outside callback/map
locks. No new public option, error, export queue or lifecycle semantics are
selected. Spans ending after this call and abrupt process death gain no new
delivery promise.

**Runtime TDD:** extend the existing real-stock deadline test owner. First
reproduce the final stock-drain-to-Future-settlement scheduling interval, end a
second span before the later public invocation, then invoke later flush and
shutdown/recovery variants while the old operation remains pending. RED is
old success with that span still buffered. GREEN requires a fresh operation
exporting it before success, or the existing bounded 504 with native drain
still attempted. Assert one in-flight operation and one total caller deadline;
preserve callback usability during blocked export, recovery and active/global
provider survival. Clean controlled scheduling in `finally`. Refactor only
within that owner after focused proof passes. No new framework/network harness
is needed: this interpreter-local gap is covered by deterministic runtime proof
allowed by REQ-016, with existing real managed journeys retained.

## FIND-TASK-001-8: serialize Python initialization on the same state

**Obligation:** REQ-012 / AC-008 require idempotent telemetry setup on one state
while preserving refusal to replace an unrelated provider.

`sdks/wyrd-sdk-python/src/state/mod.rs:183-201` checks the retained OnceLock,
constructs/installs the provider through Python, then publishes the handle.
The cell at `:94` serializes publication only. Two public shared-state calls
can both see it empty; legitimate Python construction/import/thread startup can
release the GIL. `_start_telemetry` in `python/wyrd/otel.py:277-289` installs one
global provider and correctly rejects/cleans the other candidate. The native
loser raises 409 even though the winner belongs to the same state. Public
runtime follow-up reproduces one success and one 409. Existing sequential
Python setup and corrected TS await-overlap tests do not exercise this native
initialization interval. TS FIND-6 remains closed; this separate owner is new
FIND-8.

**Selected correction:** keep `OnceLock<Py<PyAny>>` as the single retained-provider
and GC authority. Serialize its check, fallible construction and successful
publication with a per-state initialization gate acquired through the installed
PyO3 `MutexExt::lock_py_attached` mechanism. Recheck the retained cell under that
gate. Interpreter-aware waiting lets the initializer regain the GIL and avoids
an ordinary blocking-mutex/GIL deadlock. Handle acquisition failures without
unchecked panic/unwrap. Failed setup leaves the cell empty and retryable;
successful repeats return without another installation.

Do not substitute PyOnceLock: pinned PyO3 requires a Python token for its get,
whereas this state's `__traverse__` has no token and must not attach. Preserve
token-free traversal/clear and lifecycle readers on the existing cell, without
blocking GC on the new gate. A final recheck outside serialized initialization
cannot cover an unfinished winning initializer. Do not introduce a global
registry/lock, provider adoption, cached errors or duplicate retained Py objects.
Preserve optional-import diagnostics, credentials, other-state/application
provider refusal and candidate cleanup. Existing installed PyO3 is sufficient;
no new feature or dependency is necessary.

**Runtime TDD:** extend existing public Python setup coverage to contend two
calls on one state while initialization is in progress. RED is a spurious 409;
GREEN is both calls succeeding with exactly one provider/pipeline construction.
Keep distinct-state/application-provider refusal and loser cleanup covered.
Add a failed optional-import attempt followed by successful retry to prove the
gate does not memoize failure. Use deterministic interpreter-local scheduling
with cleanup and inspect GC traversal/clear preservation. Rebuild the native
boundary before execution; never initialize Python inside Rust tests. Existing
real framework journeys retain normal setup/export proof.

## Acceptance and preserved boundaries

| Finding | Acceptance |
|---|---|
| FIND-TASK-001-2 | Corrected documented dependency set loads the existing OTEL entrypoint; docs generation/check pass under current docsite conventions. |
| FIND-TASK-001-7 | No later call accepts an obsolete provider operation as its success barrier; fresh covering flush or bounded existing timeout, one total deadline, one in-flight worker, and native drain/error behavior preserved. |
| FIND-TASK-001-8 | Concurrent same-state Python setup succeeds with one installation, failure remains retryable, other owners remain refused, and GC/lifecycle readers retain their existing authority. |

Prior FIND-1/3/4/5/6 stay closed: generic TS consumers need no optional Agents
package, real live-flush/readback/continuation journeys remain, payload-free
Responses usage retains zero counts/fallback, Python waits remain bounded, and
TS overlapping setup remains idempotent. Preserve framework-agnostic OTEL
composition, first-class framework coverage, standard attributes/content controls,
ambient credentials with fresh export tokens and tenant boundaries. Do not add
server routes/tables, SDK API/catalog entries, schema changes, runtime adapters,
auth mechanisms, transport/retry owners or production dependencies. Maintain
substantive rustdoc on materially changed Rust fields/methods. No style-only
cleanup or unrelated fixture rewrite belongs here.

## Focused verification and handoff evidence

The selectors below already exist. Extend their cases rather than assuming a
new test target. Record RED/GREEN evidence for the two runtime corrections,
actual selected test counts, changed owners, scoped checks and remaining limits.
If a necessary new test is named instead, update and run its exact focused
selector in this task's implementation evidence. These commands are prospective
requirements, not executions claimed by this review.

After Rust boundary edits:

```sh
mise run py:setup
```

Managed lifecycle/deadline closure:

```sh
mise exec -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q tests/unit/state/test_observe_surface.py::test_managed_telemetry_flush_deadline tests/unit/state/test_observe_surface.py::test_managed_telemetry_lifecycle tests/unit/state/test_observe_surface.py::test_owned_provider_correlation
```

Public startup/refusal/retry closure:

```sh
mise exec -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q tests/unit/state/test_observe_surface.py::test_telemetry_setup tests/unit/state/test_observe_surface.py::test_telemetry_setup_names_the_otel_extra_without_the_sdk tests/unit/state/test_observe_surface.py::test_application_provider_is_never_flushed_by_state
```

Changed owner static and documentation lanes:

```sh
mise run fmt
mise run lints
mise run py:format:check
mise run py:lints
mise run py:typecheck
mise run docs:generate
mise run docs:check
```

Also run and record the transient documented-dependency module-load proof above
and `git diff --check`. These runtime-local fixes and documentation prerequisite
correction need no full user-journey suite or broad repository aggregate in this
remediation. Change review owns the final integrated verification selection.
No API/stub/catalog change is selected; preserve existing exports and generated
surface parity. Do not weaken checks or increase deadlines to clear failures.
If a lane fails, diagnose with the repository's required tracing and fix its
cause or report the actual blocker.

Return the implemented candidate and appended evidence for another independent
cumulative `$wyrd-task-review` of original TASK-001 plus both remediation packets.
