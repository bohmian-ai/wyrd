# TASK-001 concurrency, durability, and lifecycle domain review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `a51af030b6039eea4b2914f3ebf2c31925d08721`
- Candidate: `09e4b82c2a4cab3ea27e0889d3acf3ea22c2b596`
- Approved authority: `changes/active/skald-workflow-runtime/spec.md`, Revision 11
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-001-explicit-local-runtime.md`
- Remediation: `changes/active/skald-workflow-runtime/review/TASK-001-r4/TASK-001-R3-close-round-four-review-gaps.md`
- Prior closure hypothesis: `FIND-TASK-001-28`

The candidate remained at the stated commit throughout this review. The repository has no `.codegraph/` directory, so navigation used `rg`, Git, and direct source inspection.

## Reviewed boundary

This review traced the lifecycle that owns database- and storage-using work:

| Path | Owner and transition | Teardown consumer |
|---|---|---|
| Bound server | `WyrdTestServer::settle_lifecycle` cancels and joins or aborts `serve_handle`; `BoundServer::run` joins supervised work, marks Forge supervision drained, and performs Bifrost drain/abort | `WyrdTestServerInner` field drop, then `PgFixture` / `TestDatabase::drop` |
| In-process server | No `BoundServer::run` or serve handle; `settle_lifecycle` calls `Bifrost::shutdown`, then its abort fallback | Dedicated Scribe/Forge runtime owners, then forced fixture database drop |
| Implicit async drop | `Drop` selects a zero budget and drives `settle_lifecycle` on a scoped thread using the shared runtime | Same field-order fixture release |

Authority and source coverage included `AGENTS.md` async/testing/completion rules; `architecture/agent-rules.md`; `architecture/bifrost-design.md` lifecycle and durability sections; `architecture/references/languages/spec-driven-development.md`; the prior verdict, validation ledger, and remediation; `crates/wyrd/wyrd-testing/src/server.rs`; `crates/wyrd/wyrd-server/src/app/server.rs`; `crates/wyrd/wyrd-server/src/state.rs`; Forge scheduler/worker cancellation; Scribe and Oracle abort owners; `vala-bifrost-redux` storage settlement; and `crates/shared/wyrd-dev-fixtures/src/pg.rs` forced database removal.

## Residual-risk assessment

The reported fact is correct: an in-process fixture never calls `BoundServer::run`, so Forge's `supervision_drained` bit remains false (`state.rs:1502-1524,1572-1579`; the only setter is `app/server.rs:780-782`). Its attempted `Bifrost::shutdown` therefore refuses a clean Forge drain at `state.rs:2015-2020` and enters the existing abort path. This is not independently a defect: in-process mode started no `BoundServer` Forge supervisor to join, and the approved remediation explicitly permits completing **or aborting** that work.

The acceptance defect is that the harness does not wait for that abort to finish. `settle_lifecycle` marks `bifrost_settled = true` before attempting settlement, gives graceful shutdown one deadline, then gives `Bifrost::abort` the already-consumed same deadline (`server.rs:831-846`). `Bifrost::abort` is intentionally an awaited, unbounded completion fence: it aborts retained role owners and waits until every storage loader and governed request is idle (`state.rs:2062-2089`; `storage/mod.rs:1041-1058`). If that future is still pending when the shared deadline expires, `timeout_at` drops it, the harness logs and proceeds, and the pre-set boolean prevents `Drop` from retrying. The zero-budget path used by implicit drop on an active Tokio runtime is an even sharper instance: the abort future receives only its initial poll. The fixture can then reach `DROP DATABASE ... WITH (FORCE)` (`wyrd-dev-fixtures/src/pg.rs:426-475`) without the completion fence the remediation requires.

## Material finding

### CONC-R5-001 — `FIND-TASK-001-28` remains open because an unfinished Bifrost abort is recorded as settled

- Classification: **INCORRECT**.
- Violated obligation: the remediation requires bound timeout work to be aborted and joined, and in-process/implicit teardown to complete or abort Bifrost-owned work **before** runtime owners and the fixture are released. Its focused proof specifically requires live role work and an observable ordering barrier.
- Exact location: `crates/wyrd/wyrd-testing/src/server.rs:831-846,3675-3708,5479-5505`; lifecycle contract at `crates/wyrd/wyrd-server/src/state.rs:1990-2001,2015-2020,2062-2089`; storage completion fence at `crates/vala/vala-bifrost-redux/src/storage/mod.rs:1041-1058`; destructive consumer at `crates/shared/wyrd-dev-fixtures/src/pg.rs:426-475`.
- Evidence: in-process Forge can never satisfy `supervision_drained`, so its graceful call necessarily enters abort. The harness wraps that abort in the same deadline and treats timeout as a warning after already setting `bifrost_settled`. `BifrostStorage::abort` documents that return, not invocation, establishes that no loader or governed request remains. Therefore a stalled storage request can outlive `settle_lifecycle` and race fixture destruction.
- Observable consequence: the scheduling-dependent database-after-drop failure or Oracle/role failure that `FIND-TASK-001-28` was meant to close remains reachable during implicit async drop or any in-process shutdown whose abort settlement exceeds the two-second grace.
- Smallest testable correction: keep the correction in `WyrdTestServer` and reuse `Bifrost::abort`. Use the budget only for the graceful attempt; if it does not return a clean report, await the existing abort completion fence before setting `bifrost_settled` or releasing fields. The zero-budget path should skip graceful drain and await abort directly. Do not add a second lifecycle abstraction or weaken production fencing.
- Focused closure proof: replace the idle-storage smoke test with the remediation-required deterministic in-process case that holds a real governed storage/role operation at an existing barrier, starts implicit teardown (including the active-runtime path), and proves fixture release cannot occur until abort has settled the operation. Retain the bound stalled-serve test, which does prove abort-and-join of the serve task.

## Verification assessment and limits

- The recorded focused tests selected two cases and passed, and the recorded `mise run test:wyrd` lane passed.
- `shutdown_aborts_and_joins_a_serve_task_that_outlives_its_drain` credibly proves the bound serve handle is not detached.
- `dropping_an_in_process_server_settles_bifrost_before_fixture_release` runs off a Tokio runtime with idle storage. It asserts only that an immediately settling storage owner is closed; it neither creates live role/storage work nor exercises the zero-budget implicit-drop path. It therefore does not supply the proof required by the remediation.
- No tests were rerun in this discovery pass; the finding follows from the candidate's reachable lifecycle control flow and the recorded proof's missing scenario.

## Overall result

**FAIL** — retain prior stable finding `FIND-TASK-001-28`. The reported always-abort behavior is acceptable for an in-process owner with no Forge supervisor, but the abort must be allowed to complete before fixture release; the candidate can time it out, mark it settled, and proceed.
