# TASK-009 R3 structured Ponytail validation

## Immutable subject and independent coverage

- Repository: `/home/thorrester/Documents/GitHub/wyrd-verification-closeout`
- Base: `7d96c30066425e0cde2290842d5801307843283d`
- Candidate: `d01307b8c37b47488115743c94b624a29d66e4be`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 45
- Original task: `changes/active/verified-change-contract/tasks/TASK-009-run-context-and-python-otel-correlation.md`
- Prior remediations: `TASK-009-R1-restore-otel-correlation.md` and
  `TASK-009-R2-close-proof-and-boundary-parity.md`

The candidate matched `HEAD` before and after validation. The reviewed source
was not modified; only round-three review artifacts were untracked. There is no
`.codegraph/` directory, so repository search and direct source inspection were
used.

This validation read the complete cumulative diff, every required round-three
discovery report and the focused follow-up, both prior validated ledgers and
remediation tasks, and the applicable repository, specification, Run API,
telemetry, maintainer, and testing authority. No intended verdict was supplied.

## Producer-to-consumer and sibling trace

Shared Rust remains the identity producer. `WyrdState::run` and
`run_for_card` select the root or a hydrated Card before `Run::new` mints one
invocation ID; `Run::for_card` creates immutable sibling views that preserve
that ID. Python `PyRun` projects the exact native `card_ref` and `run_id` into
`wyrd.otel._enter_run`. Explicit Drift, Eval, and generic observations consume
the native Run directly and do not depend on ambient Python context.

The ambient Python owner is one module-level execution-local stack. `_enter_run`
attaches the pair under one locked, lazily created OTel key and appends its exact
token and prior value to `_scope_tokens`. `_exit_run` pops only the current
execution context's innermost entry, detaches that token, and restores the prior
value after a raising or swallowed reset failure. `_RunCorrelationProcessor`
reads the same key from each span's parent context. Provider registration and
the key are process resources; the Card/Run pair and token stack are
`ContextVar` values, not state on `PyRun`.

The production design therefore satisfies the same-object concurrency rule by
inspection. The proof does not. The only async concurrency test creates
`views["model"]` and `views["backup"]` with two separate `for_card` calls and
enters those distinct Python objects. Nested `with run, run` is one execution
context, the first-use thread test enters distinct sibling objects, and the
persisted journey enters one object once. Repository-wide caller search found
no test where two concurrent asyncio tasks enter the identical `PyRun` object.

That distinction is required, not speculative. REQ-151 prohibits storing one
shared attach token on the immutable Run; AC-032 and TASK-009 Scenario 2
explicitly require concurrent tasks using the same immutable Run. A per-object
token regression passes the current sibling-object test because each object
retains its own slot, but fails when two task contexts share one object and an
interleaved exit tries to detach the other task's token. The resulting optional
telemetry failure can be swallowed while stale correlation remains in the
exited task. The existing authenticated journey cannot substitute for this
explicit focused concurrency proof.

## Proposal decisions

### `TEL-R3-001` — CONFIRMED

- **Reachability and obligation:** identical-object concurrent entry is a
  public, reachable `PyRun` use expressly named by REQ-151, AC-032, the original
  Scenario 2, and `run_api.md`. It is not a dormant or inferred edge case.
- **Source evidence:**
  `sdks/wyrd-sdk-python/tests/unit/state/test_observe_surface.py:313-345`
  enters two distinct objects constructed at lines 319-320;
  `sdks/wyrd-sdk-python/src/observe/mod.rs:184-216` shows `for_card` constructs
  a new `PyRun` and every entry delegates to the common ambient owner;
  `sdks/wyrd-sdk-python/python/wyrd/otel.py:189-195,279-326` contains the
  execution-local implementation that the missing case must pin.
- **Sibling-consumer check:** nested same-object use at
  `test_observe_surface.py:360` is sequential in one context; the raw-thread
  case at lines 504-546 proves one lazy key with distinct views; the real
  journey has one entry. None can detect a token stored on the Python Run
  object and overwritten by another task.
- **Ponytail decision:** retain as new `FIND-TASK-009-7`. No production change,
  new helper abstraction, fixture, dependency, provider wrapper, or integration
  journey is warranted. The existing focused async test home and installed OTel
  test provider are sufficient.
- **Smallest safe correction:** extend the existing focused Python async proof
  with two coordinated tasks that both enter the identical `run` object. Force
  one task to exit while the other remains entered; prove the exited task's
  next span has no Wyrd correlation, the still-entered task's next span retains
  the exact pair, and a span after both exits has no pair. Preserve the current
  sibling-Card isolation and task-copy assertions. This directly distinguishes
  execution-local bookkeeping from a shared per-object token without changing
  runtime behavior.

No other discovery proposal was made. The behavior, invariant, repository,
maintainer, system, and concurrency reports' empty ledgers are supported by
source and authority; their broad PASS conclusions do not override the unique
telemetry proof finding.

## Prior-finding closure

| Prior finding | Independent closure evidence | Result |
|---|---|---|
| `FIND-TASK-009-1` | The focused failure tests now make API-only, actual raising registration, attach, enrichment, and detach failures reach the ordinary explicit Drift boundary; the registration branch records the attempted processor addition. | CLOSED |
| `FIND-TASK-009-2` | `Run::subject` now accurately documents root, initially selected, and sibling-selected views at `crates/shared/wyrd-client/src/observe/mod.rs:52-55`. | CLOSED |
| `FIND-TASK-009-3` | `_enter_run` records exact token/prior pairs and `_exit_run` restores the prior value after raising or swallowed detach failure; the focused test proves nested outer restoration and outermost cleanup. | CLOSED |
| `FIND-TASK-009-4` | `_key` double-checks under the existing lock, and the deterministic two-thread first-entry test proves one key and correct correlation for both directly entered scopes. | CLOSED |
| `FIND-TASK-009-5` | PyO3 parameter names/defaults, owning and generated stubs, and public conventional-keyword/omitted calls agree. | CLOSED |
| `FIND-TASK-009-6` | Both new shared Rust tests include accurate `# Panics` sections without executable changes. | CLOSED |

The new finding is not a reopening or duplicate of findings 3 or 4. Those IDs
cover failed restoration and first-use key publication in production. Finding 7
covers a separately explicit acceptance-evidence class for concurrent entry of
one object; current production behavior appears correct.

## Final validated finding ledger

| ID | Discovery source IDs | Status | Classification | Violated obligation | Exact location | Evidence | Observable consequence | Decision-complete correction | Focused closure proof |
|---|---|---|---|---|---|---|---|---|---|
| `FIND-TASK-009-7` | `TEL-R3-001`; follow-up identical-Run concurrency analysis | CONFIRMED | MISSING | REQ-151, AC-032, TASK-009 Scenario 2, and the approved Run API require proof that concurrent tasks using the same immutable Run do not share one attach token. | `sdks/wyrd-sdk-python/tests/unit/state/test_observe_surface.py:313-345` | The concurrent tasks enter two distinct sibling `PyRun` objects. All other same-object entries are sequential or single-use; no current test exercises identical-object concurrent entry and interleaved exit. | A regression that stores the token on `PyRun` can pass every current test yet let one task detach another's scope or retain stale correlation after exit, producing an uncorrelated or misattributed span and breaking required joins. | In the existing focused Python test home, use the existing provider/exporter and native asyncio coordination to make two tasks enter the identical `run`. Interleave their exits and assert cleanup in the exited task while correlation remains in the active task, followed by no correlation after both exit. Retain the current sibling-view and copied-task coverage. Do not change production code or add a harness, dependency, abstraction, timeout-based retry, or second journey. | Run the exact new/adapted focused pytest case and the complete `test_observe_surface.py`. The case must fail for shared per-object token storage and pass with the current execution-local stack. |

## Verification and correction boundary

Independent execution of the current async test passed (`1 passed`), confirming
the sibling-view case but not the missing identical-object case. Cumulative
`git diff --check` passed. Recorded broader evidence credibly covers the real
authenticated persisted journey, shared and language SDK suites, typing,
generation, boundaries, formatting, and lints; none exercises the missing
proof class.

The retained correction is test-only and uses existing owners and mechanisms.
It requires no new product, public API, architecture, security, compatibility,
cross-service, concurrency-semantics, resource-ownership, or persistent-data
decision. No `SPEC_REVISION_REQUIRED` condition exists.

**Validated ledger result: one retained finding, `FIND-TASK-009-7`.**
