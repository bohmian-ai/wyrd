# TASK-009 Structured Ponytail Validation

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-verification-closeout`
- Base: `7d96c30066425e0cde2290842d5801307843283d`
- Candidate: `e4f30547906b8046bdccbfd38dff0eb0ee475d06`
- Original task: `changes/active/verified-change-contract/tasks/TASK-009-run-context-and-python-otel-correlation.md`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 35
- Locked Run contract: `changes/active/verified-change-contract/architecture/logic/run_api.md`

The candidate was `HEAD` before and after source inspection. This validation
read the complete cumulative diff, every required discovery and follow-up
report, the task and specification obligations, the Run API authority, and the
applicable repository, Rust, Python, telemetry, maintainer, and testing rules.
The checkout has no `.codegraph/` directory, so repository-native source search
and direct caller inspection were used. No Cargo command or reviewed-source
edit was performed.

## Producer-to-consumer validation

The shared Run producer is `WyrdState::run` / `run_for_card`; it resolves one
hydrated `CardRef`, and `Run::new` mints the invocation ID consumed by immutable
views and every explicit observation. Python and TypeScript delegate to that
owner. This path supports the requested initial-Card behavior without another
resolver, queue, or abstraction.

The Python ambient producer is `_enter_run`. It obtains the module's private
OpenTelemetry key, attaches `(card_ref, run_id)` to the execution-local OTel
context, and records the returned token in `_scope_tokens`. The registered
processor consumes the same private key for every span start. `_exit_run`
consumes the saved token. Explicit Drift, Eval, and record emissions are sibling
consumers of the native `Run`; they do not read this ambient scope. Eval alone
also performs an independent best-effort active-span ID lookup before using the
ordinary native observation path.

The persisted journey proves the healthy path from this processor through
authenticated OTLP ingest to server-resolved Card identity and Bifrost joins.
It cannot close the two exceptional producer defects below: concurrent first
key publication can make the processor read a different key from the one an
entry attached, and failed detachment can leave that attached value current for
later spans. Generic telemetry accepts omitted correlation, while stale
correlation can be admitted under the wrong Run/Card or rejected by signed
scope enforcement.

## Proposed-finding decisions

### INV-REV-001 — REVISED

- **Reachability and obligation:** AC-032 and TASK-009 Scenario 3 expressly
  require focused proof that the named optional-telemetry failures do not
  escape or block explicit observations. Only the missing-package test reaches
  `run.observe.*`. The API-only/registration test never enters a Run or emits,
  and the processor-enrichment/detach test never emits. REQ-151 also explicitly
  makes attach failure fail-open, but no test injects it.
- **Source evidence:**
  `sdks/wyrd-sdk-python/tests/unit/state/test_observe_surface.py:374-428`;
  `sdks/wyrd-sdk-python/python/wyrd/otel.py:241-307`;
  `sdks/wyrd-sdk-python/src/observe/mod.rs:260-337`.
- **Revision:** the discovery request for an additional direct active-span
  `set_attribute` injection is unnecessary. The existing processor lookup
  injection is a valid span-enrichment failure; it only needs to be paired
  with an explicit observation. AC-032 does not require every internal
  enrichment call site to receive a separate test.
- **Decision:** retain as `FIND-TASK-009-1`.

### MR-001 — CONFIRMED

- **Reachability and obligation:** `WyrdState::run_for_card` now constructs a
  Run whose initial subject can be any hydrated Card. The materially changed
  `Run` invariant is therefore read by every Rust maintainer and projected SDK,
  not a dormant or test-only path. `AGENTS.md` section 16 and the Rust/maintainer
  references make accurate invariant-bearing rustdoc a hard acceptance rule.
- **Source evidence:** `crates/shared/wyrd-client/src/observe/mod.rs:42-67`
  states that `subject` is the root Service until `for_card`, while
  `crates/shared/wyrd-client/src/state.rs:498-510` passes an initially selected
  non-root subject directly to `Run::new`.
- **Decision:** retain as `FIND-TASK-009-2`.

### SYS-001 — CONFIRMED

- **Reachability and obligation:** the candidate deliberately exercises a
  raising `context.detach` and acknowledges that the Run scope remains attached
  by moving the failure into a copied context. In a normal caller context,
  later spans therefore read stale correlation after the lexical scope has
  exited. This violates REQ-151's restoration and detach-failure no-enrichment
  guarantees; swallowing the exception alone does not fail open.
- **Source evidence:** `sdks/wyrd-sdk-python/python/wyrd/otel.py:274-307` pops
  its bookkeeping before detachment and performs no restoration after failure;
  `sdks/wyrd-sdk-python/tests/unit/state/test_observe_surface.py:401-424`
  explicitly quarantines the resulting leak. The pinned OTel implementation
  also catches failures in its public `detach` wrapper, so safe correction
  cannot depend only on receiving an exception from that wrapper.
- **Sibling-consumer check:** explicit observations remain independent and user
  exceptions still propagate, but every later span in the same execution
  context is an affected sibling consumer of the leaked OTel value. Nested
  scopes additionally need the outer pair restored rather than merely clearing
  the key.
- **Decision:** retain as `FIND-TASK-009-3`.

### D-TEL-001 — CONFIRMED

- **Reachability and obligation:** `_scope_key` uses an unlocked lazy
  check-then-create, while pinned `opentelemetry.context.create_key` returns a
  new UUID-backed key on every call. Two threads independently entering their
  own Run scopes on first use can attach under different keys; the last global
  assignment determines what the processor reads. Raw-thread context
  propagation is outside the contract, but direct Run entry in each thread is
  not propagation and remains a required in-scope path.
- **Source evidence:** `sdks/wyrd-sdk-python/python/wyrd/otel.py:195-225,
  274-291`. The asyncio test at
  `sdks/wyrd-sdk-python/tests/unit/state/test_observe_surface.py:311-343`
  initializes the key sequentially before task concurrency and cannot exercise
  first-use publication.
- **Sibling-consumer check:** `_enter_run` and every registered provider's
  `_RunCorrelationProcessor` share this producer. A losing thread's span omits
  both attributes, so persisted trace correlation and the required custom/Eval
  joins are absent even though entry succeeds.
- **Decision:** retain as `FIND-TASK-009-4`.

The two telemetry implementation findings share a module but not a correction:
one protects publication of the single process key, while the other restores
execution-local state after an already-attached scope. Combining them would
hide two independently reachable failure conditions. The detach closure test
may also supply the detach row of `FIND-TASK-009-1`; no duplicate test harness
is warranted.

## Final validated finding ledger

| ID | Discovery source IDs | Status | Classification | Violated obligation | Exact location | Observable consequence | Decision-complete correction | Focused closure proof |
|---|---|---|---|---|---|---|---|---|
| `FIND-TASK-009-1` | `INV-REV-001` | REVISED | MISSING | AC-032 and Scenario 3 require focused failure-injection proof that optional registration, processor enrichment, and detach failures do not block explicit observations; REQ-151 also makes attach failure fail-open. | `sdks/wyrd-sdk-python/tests/unit/state/test_observe_surface.py:374-428` | The required regression evidence is absent even though today's explicit observation source is independent; a future coupling can violate the contract while all current focused tests stay green. | Extend the existing server-free Scenario 3 tests, without a new fixture or harness, so API-only/registration failure, attach failure, the existing processor-enrichment failure, and detach failure each reach one representative explicit observation's normal `WYRD_SDK_400_BIFROST_NOT_STARTED` boundary rather than a telemetry exception. Preserve the existing missing-package, user-exception, and unknown-alias proof. Do not add per-observation-method or per-enrichment-call-site cases. | Run the focused `test_observe_surface.py`; each named injected failure must reach the same ordinary Drift boundary error, and the existing user exception must propagate unchanged. |
| `FIND-TASK-009-2` | `MR-001` | CONFIRMED | VIOLATION | Materially changed Rust fields must document their actual workflow invariant. | `crates/shared/wyrd-client/src/observe/mod.rs:52` | The core owner falsely tells maintainers that a Run subject begins at the root, obscuring the new initial-Card constructor path. | Change only the `subject` field rustdoc to describe the exact Card observed by this immutable view: root for `run`, initially selected Card for `run_for_card`, or selected sibling for `for_card`. Reuse the surrounding Run terminology; add no test or abstraction. | Inspect the corrected invariant and run the repository Rust format/lint lane required for the one-line documentation change. |
| `FIND-TASK-009-3` | `SYS-001` | CONFIRMED | INCORRECT | REQ-151 requires scope exit to restore prior correlation, including nesting, and detach failure to degrade to no enrichment without affecting application execution. | `sdks/wyrd-sdk-python/python/wyrd/otel.py:274-307`; current leak acknowledged at `sdks/wyrd-sdk-python/tests/unit/state/test_observe_surface.py:419-421` | After a detach failure, later same-context spans can be durably attributed to a stale Run/Card or rejected by a narrower signed Card scope. | Keep the correction in the existing `_enter_run`/`_exit_run` owner. Record the prior Wyrd correlation with each successful attach, attempt the exact-token detach, then restore or neutralize the Wyrd key from that recorded value even when OTel's public detach raises or internally swallows a reset failure. Restore the outer pair for nesting and `None` outside the outermost scope. Swallow fallback failures and preserve user exceptions. Do not add retries, warnings, a second context system, or lifecycle work. | Inject detach failure, exit, restore the test double, and create a span in the same execution context; it must have no Wyrd correlation. Repeat with an inner Card scope and prove the next span has the outer Card/run pair. The test should also perform the representative explicit observation required by `FIND-TASK-009-1`. |
| `FIND-TASK-009-4` | `D-TEL-001` | CONFIRMED | INCORRECT | REQ-151 requires every span started inside a directly entered Run scope to receive that scope's exact pair, including concurrent uses of the same immutable Run. | `sdks/wyrd-sdk-python/python/wyrd/otel.py:195-225,274-291` | Concurrent first entry can silently export one scope's spans without `card_ref` or `run_id`, preventing the required persisted joins. | Serialize the optional one-time `_scope_key` creation with the module's existing lock, including a second `None` check inside the lock, so every caller receives one key. Keep creation lazy so OpenTelemetry remains optional; add no dependency or new lock abstraction. | Deterministically overlap two first entries in separate threads and prove spans from both independently entered scopes receive their own exact CardRef/run pair. The existing asyncio isolation test remains the non-threaded regression proof. |

## Ponytail correction assessment

None of the retained corrections needs a new product, public API, architecture,
security, compatibility, concurrency-semantic, resource-ownership, or
persistent-data decision. Existing owners and native mechanisms suffice:
accurate rustdoc, the current focused Python test home, the existing OTel
context key/token stack, and the module's existing lock. No proposed finding is
rejected, and there are no prior `FIND-*` IDs to preserve.

**Validated ledger result: four retained findings.**
