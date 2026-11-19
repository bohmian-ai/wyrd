# TASK-003 full behavior review

**Result: FAIL.** One material documentation-contract finding is proposed,
BEH-001. The four tutorial behaviors and the approved SDK ergonomics have
source-backed acceptance evidence; the newly strengthened retention guarantees
do not describe the existing outbox.

## Subject and coverage

Read-only candidate: `/tmp/wyrd-task003-review-d7685d129`, HEAD
`d7685d129ffee870ab1c497cdcb1206a89bc80c4`; full requested diff
`634e19cbc517293ed2458f30ce73401714f3447d...d7685d129ffee870ab1c497cdcb1206a89bc80c4`.
The candidate checkout remained clean. No primary-worktree implementation or
test edits were read as candidate evidence. Only this report was written.

Authority: supplied original TASK-003, positioning revision 7, service identity
and telemetry revision 17, root and SDK/docs AGENTS rules, architecture authority
map and the configured-state/telemetry and outbox sections of the owning design
documents, maintainer style, spec-driven workflow and testing map. The original
task's prohibition on covert runtime changes is read under the subsequently
approved REQ-017 additions. Revision 7 expressly supersedes the old demand for
extra Rust configure/flush calls.

The independent navigation map expanded from the common map to the actual
finance API handlers and their reader journeys; refund-agent programs,
investigation programs and native trace consumers; scheduled record/results
programs and real minute-window journeys; shared input-slot lowering;
Python/TypeScript provider installers, configuration wrappers and ASGI startup;
shared first-write state/Observe/RunBifrost owners; and the actual direct-result
staging, Scribe sink, shared outbox dispatcher and server shutdown consumers of
the changed retention prose. The requested full range was inspected; prior
acceptance was used for unchanged server judging/read-authorization behavior,
with its changed producers and callers checked here.

## Acceptance matrix

Verification references below identify committed proof inspected, plus the
supplied results in review-scope.md. No tests or Cargo commands were run by this
reviewer; fresh execution is owned by the orchestrator. PASS here means scoped
source/acceptance evidence, subject to the integrated verification record and
the separately assigned invariant, standards, system and domain reviews.

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Request: verdict precedes release; failed/inconclusive/typed errors hold; unique Run per request | `examples/finance-release/api.{py,rs,ts}` puts the awaited verification before the sole release branch, handles only passed as release, and returns execution/run attribution. Python lifespan loads once and shuts down its state. | Python `test_finance_api_releases_only_passed_reports`; Rust `finance_api_releases_only_passed_reports`; TS finance-release integration's actual API subprocess case assert returned decisions, actual execution verdict/run identity, three unique Runs and refused viewer credential. | PASS |
| Actual complete/skipped agent work with equally plausible replies; insufficient evidence is inconclusive | `examples/refund-agent/refund_agent.{py,rs,ts}` executes lookup only on complete path, records real root/draft/lookup spans, flushes their owning pipeline before checking, and supplies no trace in the third case. No fabricated span collection or replacement evaluator. | Python `test_docsite_agent_workflow_example` checks equal replies and original span names; Rust refund-agent journey and TS refund-agent integration compare the actual program's three verdicts to captured output. | PASS |
| A single typed model supplies ordinary and Trace inputs in all SDKs; canonical slots/wire values unchanged | Rust `authoring/verifier.rs::Trace` transparent serialization and `InputSlot::json` consume marker; Python `verify.Trace` Pydantic hooks/mapping schema; TS `definitions.Trace` metadata is lowered through existing inputSlots/native owner. Actual refund programs use these models. | Rust `trace_field_declares_a_trace_slot`; Python `test_trace_fields_declare_trace_slots`; TS authoring's Trace property/mapping case inspect `{"type":"trace"}`, preserve reply schema and compare bare ID values. | PASS |
| Python/TS default managed configure, opt-out, async TS, frozen repeats and missing/existing-provider refusal | Python state configure fixes native context then invokes internal installer; TS async configure fixes native context then awaits private installer. Opt-out bypasses installation; removed public state starter names have no compatibility alias. Existing provider ownership and ASGI internal startup are preserved. | Python state observation units exercise configured providers, missing imports, existing providers and frozen retries; TS telemetry units exercise defaults/resource, application provider refusal, explicit opt-out and missing optional exporter import followed by frozen retry. Existing ASGI journey coverage remains. | PASS |
| Rust agent setup follows current AC-016 | Refund Rust calls `otel::start_telemetry`, executes real spans and calls returned handle's shutdown before judging. It has no Bifrost row writes requiring configure/flush. | Actual Rust refund-agent journey checks complete/skipped/no-trace and investigation output; revision-7 exception is explicit. | PASS |
| Writerless flush succeeds; closed refuses; started drains still run after provider flush error | Shared lifecycle flush returns success only in not-started/starting phases, uses existing writer flush when started, and returns closed error after shutdown. Python/TS settle telemetry outcomes around native drain. | `flush_drains_only_a_started_writer` covers never-started, starting and closed; Python/TS managed lifecycle units retain drain/error priority/provider survival assertions. | PASS |
| First samples, table writes and table selection need no explicit startup; concurrent first writes survive | `state.rs::bifrost_writer` fast-paths existing writer, serializes starts, rechecks under gate and reuses start_gated/complete_start. Observe stages first window samples, then starts and records them; RunBifrost writes/use_table_by_name use same owner. Scheduled, author-in-code and support-desk programs remove their redundant explicit start. | TS observation integration submits two first-use samples concurrently, writes a table row, shuts down, and reads two distinct records plus correctly attributed row. Actual scheduled and author/support journeys exercise their presented entry points. Cancellation/interleaving coverage is separately assigned. | PASS |
| Server schedule, normal/violating/empty windows, public attributable readback | Scheduled YAML declares Trigger/binding; record programs only submit samples and drain. Results programs read retained window verdicts and exact identities; no client execution/polling substitute. | Python `test_docsite_scheduled_verification_example`, Rust `scheduled_check_judges_each_window`, TS scheduled-check integration execute actual programs and wait for the real scheduler's fast/slow/empty results. | PASS |
| Investigation reads original evidence/exact versions without rejudging; unknown/foreign refusal remains native | `examples/refund-agent/investigate.{py,rs,ts}` bounded-polls stored failure then reads execution details; no call to verification/judge. Actual-value/expected-check/version output comes from retained details. | Python investigation case checks source evidence, unknown and foreign execution refusal; Rust/TS actual program journeys compare captured detail output and unknown execution refusal. Existing shared reader/tenant owner is unchanged. | PASS |
| Reader-complete distinct procedures, setup and canonical handoff | Four guides embed actual programs, prerequisite setup, commands/output, defining negatives and next links. Scheduled guide now has local CLI tabs and direct Zod install; setup supplies Rust serde/schemars/tokio and agent guide supplies tracing. Shared generated text reflects the changed guides. | Inspected guides and source imports; supplied docs command/link/build/a11y/generated and reader checks. Prior route/disposition acceptance carries forward. | PASS |
| Native evidence/durability distinctions and truthful public claims | Ordinary verdict/error distinctions remain intact. Newly changed retention paragraphs assert unconditional loss/drain/timing guarantees that the existing producer/sink/shutdown paths contradict. | Existing outbox retry/shutdown tests and actual source below falsify the prose; rendered test success cannot establish these guarantees. | FAIL: BEH-001 |

## Proposed finding

### BEH-001 — INCORRECT: retained-result prose promises loss and timing guarantees the outbox does not provide

**Obligation.** Positioning INV-003 preserves native durability, distinguishes a
returned verdict from retained publication and forbids changing the runtime to
strengthen it. REQ-012's investigation requires honest later visibility
handling; INV-005 and AC-009 require claims to match current authority.

**Changed locations.**

- `docs/src/content/docs/understand/how-verification-works.md:190` now says
  records batch for at most 50 ms and then write, a clean shutdown writes
  everything queued, and only an abrupt stop loses a record.
- `docs/src/content/docs/understand/technical-questions.md:53` repeats the
  50-ms-before-write and only-abrupt-loss claims.
- Generated consumers: `docs/public/llms-full.txt:6449` and `:6788`.

**Producer-to-consumer proof.** Direct verification reserves two outbox slots
before running (`components/verification/service.rs:604`), then stage_direct
stages its inputs and Result on those reservations (`:921` and `:929`); the
returned verdict waits for neither Scribe acceptance nor durable storage.
The ordinary graceful server shutdown explicitly invokes the Scribe outbox's
bounded shutdown (`app/server.rs:832`). `wyrd-runtime/src/outbox.rs:625-675`
cancels the writer at its deadline, abandons unwritten pending items and counts
them lost. Thus a healthy process that returns a verdict while its Scribe is
temporarily unavailable can then receive a normal termination signal, perform
the documented graceful shutdown, exhaust its drain share and lose the record
without any abrupt server stop. The existing
`shutdown_counts_items_unwritten_at_the_deadline_as_lost` test (`outbox.rs:2251`)
directly asserts this intended behavior; `scribe_outbox.rs:3134-3146` also
demonstrates an ordinary peer route remaining pending while no Scribe is ready.

The 50 ms is the batching linger, not an upper bound to a durable write.
`Outbox` dispatch readiness (`outbox.rs:1135-1138`) requires no same-lane write
in flight and elapsed retry backoff, and the sink retries unavailable Scribe
delivery. The sink can also consume terminally rejected/projected-away writes
as counted loss (`scribe_outbox.rs:1880-1904`); sibling input/result/audit/span
writes share that existing owner. Finally, the shared outbox explicitly states
that abrupt process-stop losses are *not* counted (`outbox.rs:54-58`), so the
paragraph must not imply otherwise when explaining its loss boundary.

**Consequence/materiality.** These pages own the durability explanation used by
the failure-investigation tutorial and adoption FAQ. Operators can rely on a
clean stop or a 50-ms interval as proof of retained evidence, despite successful
verdicts whose records remain unwritten or are abandoned. This violates an
explicit evidence/durability teaching obligation in realistic dependency-outage
and routine-deployment scenarios; it is not a wording preference or a request
for stronger persistence.

**Smallest correction.** Correct the two existing prose owners and regenerate
their agent-readable consumers. Explain that writes happen off the verdict's
durable-write path, 50 ms is batching linger rather than a storage/visibility
deadline, and graceful shutdown only drains within its deadline. Preserve the
returned verdict versus retained record distinction; describe observable
outbox losses as counted without promising counts for abrupt process death.
Do not change outbox, retry, shutdown, request waits, or add a new durability
mechanism. A prose-only correction is sufficient; reverting the newly added
absolute claims plus accurately describing shutdown/loss limits is smaller
than any runtime change.

**Focused closure proof.** Compare corrected prose to the cited production
shutdown/readiness/drop branches and existing tests, then run the existing
docs/generated checks. No new test harness or engine change is needed.

## Prior findings and convergence

- FIND-TASK-003-1: closed; direct scheduled-reader Zod and language CLI setup
  are present in the committed guide and generated text.
- FIND-TASK-003-2: the two last recorded sites are closed: Trace's tuple field
  is documented and the lifecycle flush test has explicit panic conditions.
  Broad new-item rustdoc coverage is assigned to the standards reviewer.
- FIND-TASK-003-3: superseded by approved revision 7, and the actual Rust
  program satisfies the replacement AC-016; no extra calls are prescribed.
- FIND-TASK-003-4: closed; committed TS public configure negative unit proof
  mocks an absent optional exporter, checks failed installation and frozen
  later configuration.

This proposed failure is a newly introduced misleading retention explanation
in the reopened six-commit scope, not an incomplete closure of those fixes or a
new demand for Rust calls. The convergence path is bounded prose correction at
the two current owners and their generated consumers. It does not justify
another tutorial/runtime redesign or re-opening accepted native judging.

Skipped: test execution, Cargo, broad unrelated positioning work and dirty
primary-worktree source. Risk: new published retention guarantees currently
overstate the committed outbox; independent validation must reconcile BEH-001
with the other selected reports and orchestrator verification.
