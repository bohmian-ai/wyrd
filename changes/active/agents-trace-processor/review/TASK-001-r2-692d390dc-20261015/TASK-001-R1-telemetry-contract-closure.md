---
id: TASK-001-R1
kind: remediation
status: proposed
spec: SPEC-agents-trace-processor
spec_revision: 2
parent_task: TASK-001
remediates: [FIND-TASK-001-1, FIND-TASK-001-2, FIND-TASK-001-3, FIND-TASK-001-4, FIND-TASK-001-5, FIND-TASK-001-6]
depends_on: []
---

# Close independently validated telemetry contract gaps

## Authority, subject, and outcome

- Approved spec: [spec.md](../../spec.md), revision 2.
- Original task: [TASK-001](../../tasks/TASK-001-framework-agnostic-telemetry.md).
- Review: [verdict.md](verdict.md), FIX_REQUIRED.
- Source-validated diagnoses/recommendations: [findings-validation.md](findings-validation.md).
- Original full base: `dac4c340f835676adc29f3dcc01f98e1d3d51465`.
- Reviewed candidate/start point: `692d390dc40e43c6e1d7e25c7850d5c826d26a9e`.
- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/split/agents-trace-processor`.

Deliver six bounded corrections under the existing approved behavior. Users can
compile the generic TS SDK without Agents, retain safe supplied Responses token
counts, initialize one state concurrently, flush within its promised bound
without blocking ordinary instrumentation, and follow complete framework
recipes. Real journeys must distinguish live flush from shutdown. This packet
routes directly to `$wyrd-implement`; it authorizes no new product/API decision.

## Diagnoses and selected corrections

### FIND-TASK-001-1 — Generic TS declarations require an optional framework

**Violation:** REQ-010 optional framework dependencies; AC-003/007 typed SDK.
**Current source:** `sdks/wyrd-sdk-ts/wyrd/src/index.ts:1,2517-2540` exposes
imported Agents interfaces in the root exported class. Emitted
`dist/index.d.ts:1,1287-1296` requires `@openai/agents-core` even for WyrdClient/
Cards/Bifrost-only consumers; package metadata declares it optional. Existing
`check_ts_package.mjs:143-155` checks runtime JS and misses declaration resolution.
Independent clean consumers reproduced TS2307 with declaration checking enabled.

**Correction:** Remove the required optional-framework type edge from the root
public declaration. Use precise local structural callback input types for only
the fields consumed by this existing bridge/helpers; retain the approved root
export and real framework assignability. Regenerate from source. Extend the
existing clean-package checker to compile ordinary root APIs and the processor
without optional peers, with `skipLibCheck: false`. Do not make Agents mandatory,
move the public import, weaken declaration checking or add a package harness.

### FIND-TASK-001-2 — Framework recipes cannot be executed from prerequisites

**Violation:** REQ-015/AC-012 explicitly require actual construction or a complete
prerequisite, following current docsite conventions.
**Current source:** Observe page `record-a-run-and-observations.md:159-216` omits
framework package prerequisites and uses unconstructed agent/model/runner/session/
message/input objects. Earlier Wyrd state setup and stock-client HTTP construction
links do not define them. Tests construct them separately, so green journeys and
docs checks cannot establish that the documented procedure works.

**Correction:** Complete these existing sections using the minimal real framework
construction demonstrated in Python `tests/integration/otel/frameworks/app.py:150-251`
and TS integration `agents-trace-processor.test.ts:93-111`: installations, independent
model/provider configuration, agent/tool, input, and ADK runner/session/message.
Keep one-time OTEL setup before framework construction. Use the existing ambient
Wyrd auth and stock-client guide when calls target Wyrd, without forwarding Wyrd
credentials to external model providers. Preserve content controls, versions,
readback, language tabs/frontmatter/internal links and draft flags. No new docs
subsystem or runtime integration is needed.

### FIND-TASK-001-3 — Shutdown-only journeys do not prove live flush

**Violation:** REQ-013/016 and AC-009 require successful state flush/readback while
alive, active spans unaffected, and global export usable after state shutdown.
**Current source:** Python app `app.py:313-344` shuts down and exits before parent
readback (`test_framework_telemetry.py:62-82,114-134`); TS managed journey
`:203-222` also shuts down before querying. Unit lifecycle tests replace provider
flush and do not start the native writer. Neighboring observation journeys flush
an owned provider themselves or have no managed provider. These tests would pass
if managed state.flush omitted export or closed the writer/provider.

**Correction:** Extend existing managed journey owners and their existing
subprocess/server/query fixtures. Start Bifrost, execute a real turn, public
state.flush, and read back before shutdown/process completion. Keep an active
span open across flush and verify it can later finish/export. Continue a real
turn and native observation write through that same state before shutdown.
After shutdown, emit further framework work through the same global provider
and explicitly flush the provider; read it back without reopening the native
writer. Keep application-owned journeys separate and retain their lifecycle.
Server publication flush and in-memory export cannot replace this client barrier.

### FIND-TASK-001-4 — Privacy-disabled Responses lose supplied safe usage

**Violation:** REQ-010/AC-007 maps available final input/output counts.
**Current source:** Python `otel.py:430-435`, called by bridge `_end:382`, reads
only `data.response.usage`. Pinned Agents 0.23.1 `ResponseSpanData` has independent
`usage`; its ordinary and streaming producer populate it even when content
capture withholds the full response. A real `ResponseSpanData(response=None,
usage={input_tokens:7,output_tokens:2})` probe emitted neither counter. The current
callback test supplies a synthetic full response, masking this path.

**Correction:** Extend the existing response mapping to consume supplied direct
usage, preserving the full-response fallback when direct counts are absent.
Preserve zero, absent model/provider omission, generation/unknown behavior, late
completion reads and content exclusion. Test the real pinned ResponseSpanData
through callback completion with no response. No sensitive capture, invented
provider, new mapper, TS field or new Responses HTTP fixture is needed: this
pure runtime-local mapping does not change cross-boundary state, and existing
real OpenAI journeys already exercise the shared export path.

### FIND-TASK-001-5 — Actual Python stock flushing exceeds its deadline

**Violation:** REQ-013/AC-009 requires a bounded managed-provider wait followed by
native drain even on timeout; ordinary correlation and application emission must
remain usable under REQ-010/014 and INV-002/004.
**Current source:** `_start_telemetry` (`otel.py:232-234`) constructs a synchronous
stock provider; binding `flush_telemetry` (`src/state/mod.rs:506-523`) calls its
force_flush directly before either native drain (`:277-280,299-302`). Pinned
OTEL 1.42.1's BatchProcessor ignores its timeout and synchronously drains under
an export lock; a real dependency probe requested 1 ms and returned True after
150 ms. Multiple slow batches or an ongoing export can delay the native drain
past the promised deadline. Immediate injected bool/exception results do not
exercise the real stock behavior.

**Selected correction:** Preserve the existing provider, stock batch/export
ownership, and synchronous ordinary callbacks/correlation. Bound only lifecycle
waiting around managed provider flushing using Python standard threading/result
coordination at that existing owner. Retain at most one in-flight attempt per
managed pipeline; overlapping/repeated calls must not spawn unbounded workers
or queue retry attempts. Use the force-flush API's existing timeout_millis bound/
default; no new Wyrd public timeout option. Deadline expiration yields the existing
timeout outcome so shared error settlement/native drain proceeds while the
attempt completes in the background. Do not re-block on worker cleanup, executor
context-manager exit or newly introduced non-daemon interpreter joins. Preserve
scrubbed type-only errors and later provider usability.

This is the minimal justified additional coordination: the actual stock
processor does not satisfy the approved wait bound, and deleting the bound
would weaken the spec. Whole-provider ConcurrentMultiSpanProcessor substitution
is rejected: it moves caller-context-sensitive correlation to workers, and its
shared executor can block ordinary synchronous callbacks behind a timed-out
export. More threads or merely wrapping its batch child does not remove that
saturation path. No new queue, retry engine, exporter, persistent delivery,
cancellation guarantee, or general runtime is authorized. Private layout and
standard-library mechanics remain implementation-owned.

### FIND-TASK-001-6 — Concurrent TS setup rejects its own state's provider

**Violation:** REQ-012/AC-008 same-state idempotence.
**Current source:** `wyrd/src/index.ts:2880-2891` checks the provider field before
awaiting dynamic import. Two same-state calls can both pass; the first installs,
then the second loses global registration and raises 409. Even cached imports
resume asynchronously. Existing setup proof repeats sequentially.

**Correction:** Recheck same-state installation after the existing import await
and before the synchronous installer. With no intervening await through provider
assignment, that closes the local race. Preserve other-owner conflict and losing
provider cleanup. Do not introduce a registry, mutex, cached startup promise or
public state machine.

## Constraints and preserved behavior

Preserve approved r2 interfaces, errors and lifecycle precedence; managed state
shutdown keeps global OTEL usable. Preserve ambient client resolution and fresh
per-export shared tokens; http_auth/httpAuth remains the Wyrd stock-client path.
Keep native Bifrost drain/retry/auth/tenant authority and application-owned
provider lifecycle intact. No server/protocol/storage/schema changes, production
dependency/Cargo feature, framework detection/patching, Pydantic/ADK adapter,
extra sampling/redaction system or full-suite verification requirement.

The documented Python stock exporter refusal limitation, missing upstream TS
Chat Completions usage, and already-active TS span behavior are not findings to
redesign. Do not fabricate missing historical RED evidence. Do not weaken tests,
add ignore/allow/skip escapes, or silently replace missing real IO with mocks.

## Ordered corrective scenarios

### 1 — Optional typed consumer and concurrent startup

**Behavior.** Generic TS declarations compile without optional peers; overlapping
same-state setup succeeds once and other states still fail. Findings 1, 6.

**RED.** Extend the existing package checker with the isolated noEmit consumer;
confirm current TS2307. Extend existing `telemetry setup` to start both calls
before awaiting and confirm current second-call 409. Run C1/C2 below.

**GREEN.** Apply the local declaration correction and post-import recheck; run
C1–C3 for clean-package typing, same-state/other-owner behavior, callback
compatibility and real installed-framework assignability.

**REFACTOR.** Keep only consumed structural input fields and the existing startup
owner. No extra API/package/initialization owner.

### 2 — Safe direct Responses usage

**Behavior.** Real payload-free ResponseSpanData emits exact supplied counters,
including zero, without a full response or invented model/content. Finding 4.

**RED.** Extend existing callback lifecycle proof with the actual dependency
value and direct usage, confirm missing counts using C4.

**GREEN.** Correct the existing mapper with response-object fallback; C4 passes
new and existing generation/fallback/privacy/callback cases.

**REFACTOR.** Reuse existing mapping and callback paths. No new response transport
or generalized framework mapper.

### 3 — True stock flush deadline without blocking instrumentation

**Behavior.** Public Python flush/shutdown times out while actual export remains
held, attempts native drain and retains ordinary callback/correlation usability;
in-flight lifecycle work remains bounded. Finding 5.

**RED.** Extend existing managed lifecycle proof using a real stock managed batch
processor/exporter held by a controlled Event past a short test-only SDK bound,
without substituting immediate force_flush results. C5 must expose the current
overrun/missing 504 rather than rely on host load or live network.

**GREEN.** Implement the selected bounded lifecycle wait. Assert 504 and native
outcome before releasing export; create/end a new correlated span while held;
release work and prove real subsequent flush, active/global provider survival,
and repeated/overlapping attempt bounds. Clean up controlled work deterministically.
C4/C5 and the normal C6 managed journeys stay green.

**REFACTOR.** Keep retained wait state with the existing managed owner and leave
ordinary synchronous processor callbacks/export batching unchanged. No per-call
unbounded threads, retry backlog or executor join on the timeout path.

### 4 — Read back before shutdown, then continue

**Behavior.** Three Python frameworks and TS OpenAI prove live state.flush,
active spans/native writer continuation, and global export after state shutdown.
Finding 3; also normal-path regression for 5.

**RED.** Extend C6/C7 at existing journey owners so the first query occurs after
public state.flush and before shutdown/process completion. Record that the prior
shutdown-only proof did not exercise this boundary; do not damage production to
manufacture a failure if the normal path already works.

**GREEN.** Implement the smallest existing-fixture coordination/assertion closure,
including same-state second work/write and post-shutdown provider export. Keep
all three Python parameter cases and actual stock framework instrumentation.

**REFACTOR.** Reuse current app/launch/server/query owners; no parallel harness or
manual model-span substitute. Preserve owned-provider journeys unchanged.

## Acceptance and focused verification

| Finding | Acceptance |
|---|---|
| 1 | Generic root APIs/processor compile with no optional peers and declaration checking enabled; actual Agents assignment still compiles |
| 2 | Four recipes define/install all execution objects from clear prerequisites and lead to authenticated readback under docsite conventions |
| 3 | Real readback precedes shutdown; native writer and active spans survive flush; global provider exports after state shutdown |
| 4 | Actual payload-free Responses counts and zero/fallback cases map without content or guessed metadata |
| 5 | Actual blocked stock flush returns 504 within the existing bound, native drain is attempted, callbacks stay usable, and attempts remain bounded |
| 6 | Overlapping same-state setup succeeds with one provider; other-owner refusal/cleanup remains correct |

Run from repository root. Use existing setup (`mise run py:setup`, `mise run
ts:build`) as needed; serialize Cargo-backed work. New proof extends existing
named tests/checker rather than inventing a separate suite. Positive collection
counts are required. Exact focused commands:

```sh
# C1 — existing checker extended with clean consumer noEmit proof
mise exec -- node scripts/check_ts_package.mjs
# C2 — add concurrent calls to this existing setup case
mise exec -- pnpm --dir sdks/wyrd-sdk-ts/wyrd exec vitest run tests/unit/telemetry.test.ts -t '^telemetry setup$'
# C3 — existing TS callback contract plus real installed-framework typing
mise exec -- pnpm --dir sdks/wyrd-sdk-ts/wyrd exec vitest run tests/unit/telemetry.test.ts -t '^agents callback lifecycle$'
mise run ts:typecheck
# C4 — extend existing Python callback lifecycle with actual safe Responses data
mise exec -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q tests/unit/state/test_observe_surface.py::test_agents_callback_lifecycle
# C5 — extend real processor deadline proof and preserve owned correlation
mise exec -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q tests/unit/state/test_observe_surface.py::test_managed_telemetry_lifecycle tests/unit/state/test_observe_surface.py::test_owned_provider_correlation
# C6 — all three existing Python managed framework parameter cases
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q -m integration tests/integration/otel/frameworks/test_framework_telemetry.py::test_managed_framework_telemetry'
# C7 — existing TS OpenAI managed journey
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- pnpm --dir sdks/wyrd-sdk-ts/wyrd exec vitest run tests/integration/agents-trace-processor.test.ts -t "^managed framework telemetry$"'
```

When only supporting runtime-local cases change, these exact selectors provide
closure; do not require new Responses endpoints or unrelated full framework,
SDK/Bifrost, OIDC or repository suites. Keep required native/setup prerequisites
from the current owning tasks. New test names/private paths may be adjusted
with recorded equivalently focused commands, without weakening these assertions.

### Documentation and narrow static proof

Finding 2 is a documentation correction: no manufactured runtime RED is needed.
Walk each installation/construction/execution/readback procedure against the
existing real framework builders and C6/C7. Follow `docs/CONTRIBUTING.md` and
`docs/PRODUCT.md`, existing tabs/frontmatter/navigation/links and unchanged draft
flags. Regenerate with `mise run docs:generate`, then run `mise run docs:check`;
record its index/drift prerequisite honestly without staging absent caller
authorization or weakening the check.

For changed Python source/tests run `mise run py:format:check`, `py:lints`,
`py:typecheck`; for TS/checker changes run `mise run ts:format:check`, `ts:lints`
and C3. Regenerate declarations through normal build, never edit generated output.
If Rust bindings change for lifecycle coordination, run `mise run fmt` and
`mise run lints`; keep native Python typing in sync. If dependency/native/catalog
surfaces are actually changed, use only their applicable existing boundary or
generated checks; no new public error/catalog entry is approved here. End with
`git diff --check`, actual diff audit and finding-to-evidence closure.

Full aggregates and final integrated regression selection belong to change
review, not this remediation task. Preserve all original review reports; append
execution evidence rather than rewriting the independent verdict or approved
spec. Report exact RED/GREEN commands/counts, static results and any blocked
material discovery. Re-review the complete cumulative candidate against TASK-001
and all six stable FIND IDs; implementation alone is not task-review PASS.
