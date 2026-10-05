# TASK-001 round-five findings validation

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `a51af030b6039eea4b2914f3ebf2c31925d08721`
- Candidate: `09e4b82c2a4cab3ea27e0889d3acf3ea22c2b596`
- Candidate tree: `b5dd2f061959724877c385cd759ee13924da2213`
- Approved specification:
  `changes/active/skald-workflow-runtime/spec.md`, Revision 11, including
  `REQ-053`
- Original task:
  `changes/active/skald-workflow-runtime/tasks/TASK-001-explicit-local-runtime.md`
- Prior verdict and validated ledger:
  `changes/active/skald-workflow-runtime/review/TASK-001-r4/{verdict.md,findings-validation.md}`
- Reviewed remediation:
  `changes/active/skald-workflow-runtime/review/TASK-001-r4/TASK-001-R3-close-round-four-review-gaps.md`

The candidate remained at the stated commit throughout validation. No
`.codegraph/` directory exists, so caller and consumer tracing used Git, `rg`,
and direct source inspection. This pass read every required round-five
discovery report and independently checked the shared lifecycle claim against
the cumulative candidate and applicable authority. No follow-up report was
required because discovery materially agreed and exposed no conflicting or
unreviewed path.

## Validation result

One prior finding remains:

| Stable finding | Discovery sources | Validation | Classification |
|---|---|---|---|
| `FIND-TASK-001-28` | `BEH-R5-001`, `INV-R5-001`, `STD-R5-001`, `MNT-R5-001`, `SYS-R5-001`, `CONC-R5-001` | **REVISED, DEDUPLICATED, PRIOR ID PRESERVED** | `INCORRECT` |

`FIND-TASK-001-27` and `FIND-TASK-001-29` are closed. The remaining
correction stays inside the existing test-harness and Bifrost lifecycle owners
and requires no new public API, architecture, security, compatibility,
concurrency semantics, resource-ownership decision, or persistent-data
decision. This is bounded remediation, not `SPEC_REVISION_REQUIRED`.

## Proposal-by-proposal disposition

### Shared lifecycle proposals

- `BEH-R5-001`: **CONFIRMED and retained as `FIND-TASK-001-28`.** Active-runtime
  implicit drop is common and reachable. It passes a zero budget to the shared
  lifecycle owner, whose expired timeout polls `Bifrost::abort` once and then
  cancels it if storage settlement is pending.
- `INV-R5-001`: **CONFIRMED and deduplicated.** `bifrost_settled` is written
  before shutdown or abort completes. That producer feeds every later teardown
  consumer, so a cancelled abort suppresses the only retry before fixture
  destruction.
- `STD-R5-001`: **CONFIRMED and deduplicated.** The implementation and its
  proof do not meet the remediation's explicit completion-before-fixture-drop
  rule. The warning after timeout cannot substitute for quiescence.
- `MNT-R5-001`: **CONFIRMED and deduplicated.** The field name, rustdoc, and
  idle-storage test claim a stronger lifecycle state than the owner has
  established. The smallest correction is at the one shared producer, not at
  each teardown caller.
- `SYS-R5-001`: **CONFIRMED and deduplicated.** The production Bifrost abort
  contract is a completion fence. Cancelling it permits harness-owned work to
  outlive runtimes, storage roots, and the fixture while teardown reports
  settlement.
- `CONC-R5-001`: **CONFIRMED and deduplicated.** `BifrostStorage::abort` is
  intentionally unbounded after the graceful deadline because it awaits all
  retained loaders and governed requests. Reapplying the elapsed graceful
  deadline recreates the exact race that abort exists to close.

These are one defect, not six: `WyrdTestServer::settle_lifecycle` produces a
false settled state after cancelling the existing abort completion fence.

### Empty ledgers

- The network-security review's empty ledger is **VALIDATED**. The remediation
  does not alter endpoint screening, request pinning, credential binding,
  containment, retry classification, authorization, or tenant isolation.
- The telemetry/privacy review's empty ledger is **VALIDATED**. The effective
  post-callback request model is now shared by provider dispatch and the
  payload-free `chat` span, while model-less Gemini and Vertex requests retain
  the resolved Prompt fallback.
- No optional cleanup, refactor, timeout tuning, new lifecycle abstraction, or
  production shutdown change is retained.

## Independent source validation

### Producer-to-consumer trace

1. `WyrdTestServer::settle_lifecycle` is the single lifecycle producer used by
   explicit `shutdown`, startup rollback, `shutdown_and_inspect`, abrupt
   termination, terminal-failure cleanup, and implicit `Drop`
   (`crates/wyrd/wyrd-testing/src/server.rs:772-848,937-948,961-992,1014-1059,3543-3552,3675-3708`).
2. A clean bound `BoundServer::run` owns and joins the supervised process tasks,
   then alone records Forge supervision as drained before Bifrost shutdown
   (`crates/wyrd/wyrd-server/src/app/server.rs:744-782,862-884`). The harness
   correctly treats that returned report as settled and does not drain the same
   owner twice.
3. An ordinary in-process server never runs `BoundServer::run`. Its Forge
   `supervision_drained` flag therefore remains false, so
   `Bifrost::drain_selected_owners` refuses a graceful report and
   `Bifrost::shutdown` enters its existing abort path
   (`crates/wyrd/wyrd-server/src/state.rs:1990-2020`). This is correct: no Forge
   supervisor ran, and the remediation explicitly permits abort.
4. `Bifrost::abort` closes admission, signals selected role owners, and awaits
   `BifrostStorage::abort` (`state.rs:2062-2089`). Storage abort cancels its
   owner, aborts retained loaders, and awaits the governed-request count reaching
   zero. Its wait is deliberately unbounded because graceful close already owns
   the deadline (`crates/vala/vala-bifrost-redux/src/storage/mod.rs:1041-1058`).
5. The candidate instead sets `bifrost_settled = true`, computes one graceful
   deadline, and wraps both shutdown and the fallback abort in that same
   deadline (`wyrd-testing/src/server.rs:831-846`). Tokio polls the wrapped
   future before its timer; with a zero or already-expired deadline, a pending
   abort receives its initial cancellation poll and is then dropped. Signalling
   cancellation is not the completion fence promised by `abort`.
6. `Drop` always supplies `Duration::ZERO` when invoked on an active Tokio
   runtime (`server.rs:3685-3703`). Many async journeys create an in-process
   server and rely on implicit scope drop, so the path is reachable rather than
   speculative. Explicit zero-budget sibling callers reach the same source.
7. Field destruction then releases `AppState` and dedicated runtime owners,
   whose drops use `shutdown_background`, before the last-declared `PgFixture`
   synchronously issues `DROP DATABASE ... WITH (FORCE)`
   (`wyrd-testing/src/server.rs:196-252`;
   `wyrd-server/src/state.rs:189-205,252-267`;
   `crates/shared/wyrd-dev-fixtures/src/pg.rs:426-475`). A governed request that
   has observed cancellation but has not settled can therefore overlap fixture
   destruction.

### Sibling consumers and proof

- The bound timeout branch now retains, aborts, and joins the serve handle. The
  focused stalled-serve test credibly closes the detached-handle portion of the
  prior finding.
- `terminate_abruptly_for_test` and `await_terminal_failure_for_test` explicitly
  call the same zero-budget producer; startup rollback and ordinary shutdown can
  reach the same expired fallback after spending the graceful budget. The
  correction therefore belongs in `settle_lifecycle`, not in `Drop` or a list
  of consumer guards.
- The new in-process test runs synchronously off a Tokio runtime, supplies the
  two-second budget, and holds no live role or storage operation
  (`wyrd-testing/src/server.rs:5479-5505`). Idle storage can complete abort in
  the first poll, so the test cannot falsify the defective zero/expired-budget
  path and does not provide the remediation-required ordering barrier.
- The existing `StorageOperationBarrier` and governed-request settlement path
  already provide deterministic production-shaped proof. Owner cancellation
  wins the storage attempt's biased race, drops the barrier/backend future, and
  lowers the same request count that abort awaits
  (`vala-bifrost-redux/src/storage/mod.rs:848-909,940-966,1062-1119,1717-1815`).
  No sleep, new dependency, or test harness is needed.

## Prior-finding closure

| Prior finding | Result | Validation evidence |
|---|---|---|
| `FIND-TASK-001-27` | **CLOSED** | `docs/architecture/skald.md:3-6,43-67` now shows the narrow `skald-workflow -> wyrd-spec` contract edge, names the other live foundation consumers, and retains the prohibition on `wyrd-server`, application-tier, and Vala dependencies. The text and diagram agree with the manifests and doctrine. |
| `FIND-TASK-001-28` | **OPEN / REVISED** | Bound serve-task detachment is fixed and the harness now reaches the existing Bifrost owner. The shared producer nevertheless cancels a pending abort at the zero or elapsed graceful deadline, records settlement first, and permits fixture destruction to proceed. |
| `FIND-TASK-001-29` | **CLOSED** | `chat_span` reuses the existing `request_model` helper on the effective post-callback request and falls back only for model-less wire shapes. The focused test asserts both actual dispatch and span model plus provider, operation, finish-reason omission, and payload exclusion. |

Earlier `FIND-TASK-001-1` through `FIND-TASK-001-26` were rechecked as closure
hypotheses through the cumulative candidate and prior validated evidence. No
changed producer or sibling consumer reopens them.

## Final validated finding ledger

### FIND-TASK-001-28 — Test-server teardown can record settlement before Bifrost abort completes

- Discovery sources: `BEH-R5-001`, `INV-R5-001`, `STD-R5-001`, `MNT-R5-001`,
  `SYS-R5-001`, `CONC-R5-001`.
- Status: **REVISED**.
- Classification: `INCORRECT`.
- Violated obligation: the approved remediation requires every database-using
  role or serve task owned by `WyrdTestServer` to complete or be explicitly
  aborted before runtime owners and `PgFixture` are released, including
  implicit drop. Abort must establish completion, not merely signal it.
- Exact location: `crates/wyrd/wyrd-testing/src/server.rs:831-846,937-948,986-992,3675-3708,5479-5505`;
  completion contracts at
  `crates/wyrd/wyrd-server/src/state.rs:1990-2001,2062-2089` and
  `crates/vala/vala-bifrost-redux/src/storage/mod.rs:1041-1058`; destructive
  consumer at `crates/shared/wyrd-dev-fixtures/src/pg.rs:426-475`.
- Evidence: `settle_lifecycle` writes `bifrost_settled` before completion and
  gives `Bifrost::abort` the same deadline as graceful shutdown. Active-runtime
  implicit drop supplies zero; an exhausted graceful attempt supplies an
  already-expired deadline. If the abort's initial poll cancels owners but its
  required storage wait is pending, the timeout drops the future, logs a
  warning, and returns. The settled flag suppresses any retry before fixture
  drop.
- Reachability: in-process fixtures are routinely dropped from async tests and
  necessarily select abort because only `BoundServer::run` can certify Forge
  supervision drained. Explicit abrupt and terminal-failure seams also select
  the zero-budget path, while ordinary shutdown and rollback can exhaust the
  graceful deadline.
- Observable consequence: retained storage or role work can overlap dedicated
  runtime release and forced database removal, preserving the
  scheduling-dependent after-drop failure or Oracle self-fence/process-abort
  class that the remediation was required to close.
- Decision-complete minimum correction: keep ownership in
  `WyrdTestServer::settle_lifecycle` and reuse the existing `Bifrost::abort`
  completion fence. Keep the budget on the graceful serve/Bifrost attempts.
  When Bifrost does not produce a clean drain within that budget, await the
  existing abort to completion without another timeout; a zero budget skips
  graceful Bifrost drain and awaits abort directly. Record
  `bifrost_settled` only after a clean bound/drain report or completed abort.
  Preserve bound serve-task abort-and-join, production Forge certification,
  self-fencing, field order, and all production lifecycle APIs. Add no sleep,
  retry, larger timeout, second lifecycle abstraction, or new dependency.
- Focused closure proof: retain the bound stalled-serve test. Replace or extend
  the idle in-process smoke test with one deterministic active-runtime implicit
  drop that admits a real governed storage operation at the existing
  `StorageOperationBarrier`, observes the barrier before teardown, and proves
  teardown cannot release the fixture or return while storage remains
  unsettled. After cancellation, assert the operation terminal and storage
  settlement precede fixture-release observation. Use synchronization and the
  existing inspection/owner state, not timing sleeps. Run the exact focused
  selectors and `mise run test:wyrd`.

## Ponytail assessment

The first valid rung is reuse: `Bifrost::abort` already is the required
completion fence, and `StorageOperationBarrier` already is the deterministic
proof seam. The correction is one shared-owner change plus one focused test.
Consumer-specific guards, a second timeout/configuration surface, a new
shutdown type, or production lifecycle changes would add complexity without
closing the source invariant more reliably.

## Verification assessment

- All eight required discovery reports were present and complete; no required
  reviewer report was missing.
- The recorded focused callback-model test and two teardown tests passed, and
  the candidate records green format, lint, Skald, Wyrd, docs, and client-tier
  lanes. `git diff --check base..candidate` is green.
- The bound focused test is credible for serve-task abort-and-join. The
  in-process focused test is not credible closure proof for live-work ordering
  because it exercises only idle storage off-runtime.
- The immutable candidate and complete caller trace were available. Validation
  is therefore **FIX_REQUIRED**, not `BLOCKED`.
