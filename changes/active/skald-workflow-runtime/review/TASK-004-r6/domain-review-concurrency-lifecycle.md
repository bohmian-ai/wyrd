# Concurrency and Lifecycle Domain Review — TASK-004 R6

## Review Findings

### Critical

None.

### Important

None.

### Suggestions

None.

## Reviewed Boundary

Immutable subject: base `b90335e0991438e8c28b65ed9c0c4cd80f9b0d56`,
candidate `5f3b521b5005c26277d53e7dfd2458c4f740e8be`, approved
`SPEC-skald-workflow-runtime` Revision 14, original TASK-004, and remediation
tasks R1 through R5. `.codegraph/` is absent, so review used immutable Git
objects, repository search, and direct source reading.

| Boundary | Authority and source traced | Result |
|---|---|---|
| Preparation deduplication and waiter ownership | Revision 14 normative create flow, INV-018/019, AC-021; `components/workflow/host.rs:75-123,230-287`; `components/workflow/runs.rs:258-367,530-636`; `tracked_preparation_replay_and_disconnect` | **PASS.** The one locked admission decision installs a tracker token before exposing the reservation. Matching requests share one preparation result, handler/waiter loss cannot cancel it, and failure/drop removes the key and releases capacity once. |
| Atomic acceptance, active capacity, and snapshot ownership | REQ-034A/034C/050; `runs.rs:563-688`; `host.rs:276-287`; `WorkflowExecutor::drive`; `AcceptedRun::{observe,finish}` | **PASS.** Acceptance replaces the reservation and transfers its active slot under the run-table lock. Whole nonterminal snapshots come from the scheduling owner; only `finish` publishes a terminal after query owners drain and then releases capacity. |
| Step attempt, cancellation, and deadline semantics | REQ-016/019-023/045/048 and the fixed attempt-one decision; `skald-workflow/src/workflow.rs:226-356`; `skald-workflow/src/run.rs:109-215`; executor cancellation/deadline tests | **PASS.** Scheduling reserves attempt one before publishing `Running`. Every joined interruption settles that published step as `Cancelled`; only pending steps become `Unstarted`. Biased cancellation and total-deadline branches abort and join the `JoinSet` before terminal construction, and no later transition can mutate a committed terminal. |
| Prepared-run deadline ownership | R4/R5, REQ-016/050; `WorkflowExecutor::new`; `PreparedWorkflowRun::{deadline,execute}`; `host.rs:374-400`; `tools.rs:43-114,188-233` | **PASS.** `WorkflowExecutor::new` samples the sole absolute run deadline. The exact instant is bound once through the shared `OnceLock` before acceptance; each query keeps an explicitly shorter positive deadline and clips an omitted or longer one to the prepared run's remaining duration. Revision 14 does not change this path. |
| Query owner, cancellation, and settlement ordering | Query ownership contract, INV-021; `tools.rs:188-233`; `query/collect.rs:259-399`; `query/scheduled.rs`; `oracle/lifecycle_controls.rs:34-162,301-334`; forwarded Oracle journey | **PASS.** Each query runs on the run's tool-owner tracker. Dropping an Agent waiter synchronously cancels its child token while the tracked owner retains the open/response and follows the canonical Oracle settlement path. `RunTools::drain` closes and joins all owners before Workflow terminal publication and active-slot release. |
| Shutdown and blocking preparation | REQ-034A/050, INV-018/019; `runs.rs:346-367,417-433`; `host.rs:255-287`; `app/server.rs:746-821`; run-owner shutdown unit proofs and server lifecycle journey | **PASS.** Reservation ownership covers the pre-spawn interval; blocking preparation remains tracker-owned after its awaiting task is cancelled. Workflow admission closes and its children are signalled/drained first under the one process deadline while gateway, query, and Bifrost settlement owners remain available. Deadline exhaustion is reported as an unclean drain. |
| Retention and process-local loss | REQ-034A/B/C; `runs.rs:117-210,369-415,668-688`; lifecycle journeys | **PASS.** Only terminal entries are expired or evicted; eviction removes their scoped idempotency keys, and active/not-yet-settled runs remain ineligible. A restart or non-owning replica has no run owner and yields the common not-found behavior. |
| Revision 14 dispatch changes | Revision 14 fixed provider decisions; `skald-workflow/src/workflow.rs:534-568`; `skald-workflow/src/route.rs`; R5 compatible-route journey | **PASS.** Attempts now derive dispatch identity from `Prompt::provider()` while retaining the already-fixed absolute deadline, cancellation token, correlation, and attempt registry lifetime. One request variant per wire schema and the Vertex-as-Gemini representation add no second executor, retry owner, clock, cancellation tree, or settlement path. |
| Oracle graph release and fixed human decisions | `architecture/bifrost-design.md` distributed execution; cumulative Oracle diff and lifecycle evidence | **PASS.** Supervisor drain polling and idle refusal remain deleted. Participant grant-stream close remains the follower release, with no leader release acknowledgement. No replacement polling loop, acknowledgement protocol, lifecycle option, setting, or bespoke check was introduced. |

## Authority and Source Coverage

- Read and applied `AGENTS.md`, `architecture/agent-rules.md`, the
  spec-driven-development and maintainer-style references,
  `architecture/wyrd-design.md`, the applicable Workflow and Oracle portions
  of `architecture/bifrost-design.md`,
  `architecture/wyrd-security-posture.md`, Revision 14, TASK-004, and R1-R5.
- Reviewed the complete cumulative base-to-candidate file inventory and the
  current owners/callers around server run reservation, preparation, promotion,
  snapshots, cancellation, capacity, retention, and shutdown; Skald execution,
  attempts, deadlines, and drain; built-in query ownership; and Oracle
  cancellation/settlement and graph release.
- Rechecked the source closure of the concurrency/lifecycle findings from R1
  through R5. The reservation tracker token and tracked blocking work, published
  attempt-one rule, one prepared-run deadline, per-run query-owner join, and
  omitted/longer/shorter real Oracle proof remain present.
- Applied the standing DRIFT direction. The implementation uses established
  Rust/Tokio mechanisms already used in the repository: one bounded state lock,
  `TaskTracker` and its token, `CancellationToken`, `JoinSet`, watch snapshots,
  and one standard-library one-time binding. No mechanism lacking repository or
  comparable-project precedent is required or recommended.

## Open Questions

None.

## Verification Notes

- Strict read-only review: no build, test, Cargo, mise, formatter, linter,
  code-generation, or package-manager command was run.
- Recorded final-tree evidence reports `fmt`, `lints`, `git diff --check`,
  `test:skald`, `WYRD_TEST_PACKAGES=wyrd-server mise run test:wyrd`, and
  `test:bifrost:journey:oracle` passing, along with the focused Workflow server,
  prepared-deadline, and forwarded-Oracle journey commands.
- The R5 forwarded-Oracle journey now drives omitted, explicitly longer, and
  explicitly shorter positive `deadline_ms` inputs through the real Workflow
  tool path. It distinguishes run-deadline timeout from a shorter query timeout,
  checks that no rows escape an unsuccessful query, observes Oracle settlement,
  and verifies later sibling serviceability.
- Source reading establishes the exact ownership and ordering claims above;
  recorded successful tests are supporting evidence, not a substitute for that
  trace. The instructed prohibition on executing verification is the only
  verification limit and does not reveal a source-backed defect.
- Candidate identity was rechecked before writing and remained
  `5f3b521b5005c26277d53e7dfd2458c4f740e8be`.

## Overall Result

**PASS**

No material concurrency or lifecycle finding remains in the cumulative
candidate.
