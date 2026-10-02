# Concurrency, Cancellation, and Runtime-Lifecycle Review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `a51af030b6039eea4b2914f3ebf2c31925d08721`
- Candidate: `afdd8cd716c4529bd8cbb7fbe175bef55ef6ee1f`
- Approved authority: `changes/active/skald-workflow-runtime/spec.md`, Revision 11 (`9a621a28a`)
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-001-explicit-local-runtime.md`
- Remediation authority: `changes/active/skald-workflow-runtime/review/TASK-001-r2/TASK-001-R1-close-validated-runtime-gaps.md` plus `TASK-001-R1-addendum-revision-11.md`

The candidate was at the stated hash before and after source inspection and
focused verification.

## Reviewed boundary

This pass traced the cumulative local execution path from
`Workflow::run_with_options` through `ExecutionPlan`, `WorkflowExecutor`,
`StepTask`, `RunLedger`, route adapters, and the Agent loop. It covered bounded
`JoinSet` scheduling, cancellation/total-deadline priority, ordinary peer drain,
pre-poll abort settlement, attempt accounting, retry eligibility and backoff,
checked deadline construction, parent-drop cleanup, tracing span guards, Agent
timeout ownership, and the synchronous Python runtime bridge. It also compared
the deleted Observer paths and prior round-two findings with the Revision 11
replacement and inspected the focused Rust and Python-facing proof.

## Prior-finding closure

| Prior finding | Current evidence | Result |
|---|---|---|
| `FIND-TASK-001-6` / `CONC-001` — Observer callbacks governed Workflow liveness | The Observer crate and Workflow callback path are deleted. `workflow.rs:197-221,583-633` emits payload-free `tracing` spans/events without awaiting a callback. | **CLOSED** |
| `FIND-TASK-001-14` / `CONC-002` — pre-poll abort produced cancelled/zero-attempt state | `workflow.rs:319-345` calls `step_cancelled` only after the atomic attempt counter is nonzero; `run.rs:217-226` converts the untouched running entry to `unstarted` and clears timestamps/attempts. `bounded_attempt_lifecycle` covers both pre-poll and begun abort settlement. | **CLOSED** |
| `FIND-TASK-001-15` / `CONC-003` — unchecked deadline construction | `plan.rs:112-120,308-321` rejects unrepresentable authored step and Agent timeouts; `workflow.rs:147-165,551-560` checks local run and per-attempt construction. `resolved_bindings_reject_before_dispatch` covers `u64::MAX`/`Duration::MAX`. | **CLOSED** |
| `FIND-TASK-001-16` / `CONC-004` — retry counter overflow | `wyrd-spec/src/card/workflow.rs:224-258` rejects only `u32::MAX`; execution can therefore represent `max_retries + 1`, and the focused validation test retains `u32::MAX - 1`. | **CLOSED** |

REQ-053 also closes the old callback containment and observer-documentation
work by deletion. The replacement `workflow.run`, `workflow.step`, backoff,
`invoke_agent`, `chat`, and `execute_tool` instrumentation is synchronous,
payload-free in source, and exercised through the existing
`wyrd_telemetry` capture helper rather than a new runtime or callback system.

## Review findings

### Important

- **CONC-R3-001 — The Agent timeout expires before terminal journal settlement, so a terminal journal append can keep a Workflow attempt alive indefinitely.**
  - **Classification:** `INCORRECT`.
  - **Violated obligation:** Revision 11's Workflow retry/deadline contract and `REQ-016` require the Agent timeout to be part of the attempt's effective wall-clock deadline, all Agent-loop work to remain within that deadline, and an Agent timeout to return as a retryable terminal attempt outcome. `AC-020` requires real Agent-timeout and precedence proof.
  - **Exact location:** `crates/skald/skald-agent/src/loop_runtime.rs:120-135` and `187-202`; `crates/skald/skald-workflow/src/workflow.rs:434-462,497-540`.
  - **Evidence:** Both Agent entry points wrap only `body`/`result` in `tokio::time::timeout`. After that timer has returned `AgentError::Timeout`, each path unconditionally awaits `append_terminal_journal_event` outside the timeout before recording the span outcome and returning. `Journal` is an arbitrary async `Send + Sync` backend (`skald-agent/src/journal.rs:84-89`) and has no liveness bound. The Workflow outer select races cancellation, total deadline, and the optional step-attempt deadline, but has no Agent-deadline arm; `agent_deadline` only enters the attempt/route context. Consequently, when a Workflow supplies only the Agent Card timeout, a journal that accepts earlier events and remains pending on `AgentError` prevents `agent.run_prompt` and the Workflow attempt from ever returning. The existing timeout test uses an immediately completing `RecordingJournal`, while `bounded_attempt_lifecycle` only classifies a constructed `AgentError::Timeout`; neither exercises this reachable tail.
  - **Observable consequence:** a graph with a finite Agent timeout can remain running forever, produce no retry or terminal `WorkflowRun`, and keep its `workflow.step`/`invoke_agent` spans open after the effective deadline. This breaks bounded execution even though provider/tool work was cancelled on time.
  - **Testable correction:** make the existing Agent timeout owner bound every awaited operation that can delay the Agent result, including best-effort terminal journal settlement; do not detach journal work or add another runtime. Once the deadline wins, return the typed Agent timeout promptly so the existing Workflow classifier can retry or exhaust it normally. Add a deterministic Journal fixture that completes pre-terminal appends and remains pending on the terminal append, then prove the Agent call returns its timeout and a Workflow using that Agent records/retries the timeout without surviving work. Retain the current ordinary journal, timeout-precedence, and tracing assertions.

## Authority and source coverage

| Boundary | Authority and source coverage | Result |
|---|---|---|
| Bounded ready scheduling, ordinary failure drain, cancellation/deadline abort-and-drain, parent drop | Revision 11 `REQ-017`–`REQ-020`, `REQ-048`, `INV-016`; `workflow.rs:228-359`; `run.rs:114-179,210-280`; `bounded_attempt_lifecycle` | **PASS** |
| Attempt accounting, retry eligibility, saturating/capped backoff, retry bound | Retry/deadline contract, `REQ-016`, snapshot invariants; `attempt.rs:44-193`; `workflow.rs:411-490,544-560`; `wyrd-spec/src/card/workflow.rs:224-258` | **PASS** |
| Deadline construction and priority | Retry/deadline contract, `REQ-016`, `AC-020`; `plan.rs:53-145,308-321`; `workflow.rs:147-165,284-297,421-490`; Agent `loop_runtime.rs:120-135,187-202` | **FAIL** — `CONC-R3-001` |
| Pre-poll abort and terminal snapshot invariants | Snapshot invariants, `REQ-020`/`REQ-021`; `workflow.rs:319-359`; `run.rs:120-179,217-245`; focused pre-poll settlement proof | **PASS** |
| Route/provider/tool future ownership | `REQ-043`, `REQ-048`, `INV-020`/`INV-021`; `workflow.rs:497-540`; `route.rs:367-548`; Agent tool futures in `loop_runtime.rs:379-511` | **PASS** within Task 001's Skald-owned boundary |
| REQ-053 Observer deletion, span hierarchy, payload exclusion, cancellation guard | `REQ-053`; `workflow.rs:197-221,411-490,583-633,1413-1577`; `loop_runtime.rs:298-317,379-511,590-637`; `agent_timeout.rs:50-246` | **PASS** |
| Python synchronous boundary | `REQ-047`, `INV-016`; `sdks/wyrd-sdk-python/src/workflow.rs:508-533`; shared `wyrd_runtime::runtime()` with `py.detach` | **PASS** — one native engine and no ad hoc runtime |

## Verification notes and limits

Focused commands run against the immutable candidate, all passing:

- `mise exec -- cargo nextest run --locked -p skald-workflow --lib -E 'test(=workflow::tests::bounded_attempt_lifecycle)'` — 1 passed.
- `mise exec -- cargo nextest run --locked -p skald-workflow --lib -E 'test(=workflow::tests::run_tracing_spans)'` — 1 passed.
- `mise exec -- cargo nextest run --locked -p skald-agent --test agent_timeout -E 'test(=agent_run_timeout_terminates_cleanly)'` — 1 passed.
- `mise exec -- cargo nextest run --locked -p skald-agent --test agent_timeout -E 'test(=agent_run_no_timeout_runs_to_completion)'` — 1 passed.
- `mise exec -- cargo nextest run --locked -p skald-agent --test agent_timeout -E 'test(=agent_run_emits_genai_spans_without_payloads)'` — 1 passed.

The remediation record reports the broader format, lint, Skald, Python,
typecheck, codegen, boundary, docs, and example lanes green. This reviewer did
not rerun those aggregates. No current test supplies a pending terminal Journal
or executes a real Agent timeout through Workflow retry/precedence, so those
green lanes do not close `CONC-R3-001`.

## Overall result

**FAIL** — `CONC-R3-001` is a bounded lifecycle defect within the approved
deadline semantics. It requires no new product, architecture, concurrency, or
persistent-data decision.
