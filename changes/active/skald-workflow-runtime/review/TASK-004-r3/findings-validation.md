# Structured Ponytail Validation — TASK-004 R3

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `b90335e0991438e8c28b65ed9c0c4cd80f9b0d56`
- Candidate: `f17726fb25df1fa513875dca8d92f0073340ee0a`
- Approved specification: `changes/active/skald-workflow-runtime/spec.md`,
  Revision 13
- Original task:
  `changes/active/skald-workflow-runtime/tasks/TASK-004-accepted-server-jobs.md`
- Remediations:
  `changes/active/skald-workflow-runtime/review/TASK-004-r1/TASK-004-R1-close-accepted-job-gaps.md`
  and
  `changes/active/skald-workflow-runtime/review/TASK-004-r2/TASK-004-R2-align-revision-and-source-contracts.md`
- Prior verdicts and validated ledgers:
  `changes/active/skald-workflow-runtime/review/TASK-004-r1/` and
  `changes/active/skald-workflow-runtime/review/TASK-004-r2/`
- Review mode: complete cumulative diff, current source, applicable authority,
  every R3 discovery report, the focused follow-up, and recorded evidence only.
  No build, test, Cargo, or mise command was run.

The candidate identity was checked before validation and resolved to the
requested commit. `.codegraph/` is absent, so immutable Git objects, `rg`, and
direct source inspection were used.

## Validation method and completeness

Every proposal was checked against its full owning item, producer, reachable
consumers, sibling consumers, cumulative diff, approved obligation, and the
recorded proof. The two duplicated follow-up claims were validated once with
their discovery sources. The correction ladder was applied to each retained
finding: use the existing scheduler/ledger/counter and existing module import
blocks; add no handshake, protocol, lifecycle owner, parser, dependency,
scanner, lint, check, setting, option, or test harness.

The fixed human decisions remain binding. Deleted Oracle graph-drain polling
and supervisor idle refusal stay deleted. Closing participant grant streams is
follower release, and the leader does not await an acknowledgement. The
foreign-tenant journeys do not need a model step because the harness seeds
gateway credentials only for the fixture tenant.

## Proposal-by-proposal validation

| Discovery proposal | Source validation | Final disposition |
|---|---|---|
| `DOMAIN-CONCURRENCY-R3-001`, `FOLLOWUP-R3-001` | `WorkflowExecutor::drive` marks a step `Running` and synchronously publishes the snapshot before the `StepTask` is spawned or polled. `RunLedger::step_started` leaves attempts at zero. Cancellation/deadline can abort the task before its first poll; `settle` then leaves it running and `RunLedger::finish` rewrites that externally published active step to `Unstarted`, clearing its start time. Revision 13 expressly requires running and cancelled active steps to have at least one attempt, interrupted active steps to retain timestamps, and observable state not to regress. The server stores every callback snapshot through `AcceptedRun::observe`; direct `PreparedWorkflowRun::execute` callers are a sibling observer. | **CONFIRMED**, deduplicated as `FIND-TASK-004-19`. |
| `DOMAIN-CONCURRENCY-R3-002`, `FOLLOWUP-R3-002` | The sentence at `analytical_supervisor.rs:580-585` is inaccurate: `draining_graphs` sees only a local `settlement_failure`, and no source field, transport, or caller represents a follower acknowledgement. However, the exact sentence and method predate the reviewed base unchanged. The cumulative candidate deletes nearby graph-child polling and materially changes other lifecycle owners, but does not introduce or modify this sentence. R2's approved correction was explicitly scoped to `AnalyticalGraphLifecycle` and its transport field, which now accurately describe stream-close release. The runtime and governing Bifrost authority already implement the binding decision. Requiring another edit here would turn unrelated pre-existing documentation debt into TASK-004 acceptance scope. | **REJECTED**. The wording is not a distinct cleanup mechanism and should not justify any acknowledgement, but it is not a candidate finding. No remediation is retained. |
| `STD-004-R3-001`, `FOLLOWUP-R3-003` | All four cited sites are new or materially changed and violate the explicit module-import/bare-interface rules. `raw_body` changed its return contract and qualifies `Value` despite the existing import; `cancel_while_opening` and test helper `later` are new interfaces qualifying `Instant`; materially changed `fixture_rows_ipc` retains a qualified `Bytes` return; and the new server journey places `PermissionsExt as _` inside a non-generic function outside the rule's narrow exception. These are reachable test/support or production interfaces, not dormant examples. | **CONFIRMED**, deduplicated as `FIND-TASK-004-20`. |

The behavior, invariant, maintainer, system-resilience, security/tenancy,
query-settlement, and provider-contract reports proposed no other findings.
Their PASS conclusions do not displace the two source-supported findings above.

## Required explicit assessments

### Published running state and pre-poll abort

The zero-attempt running snapshot is not an intentional unpublished scheduler
state. `drive` invokes `on_transition` synchronously at
`workflow.rs:275-276`; the server callback immediately replaces the stored GET
snapshot at `workflow/host.rs:285` and `workflow/runs.rs:650-665`. The public
`PreparedWorkflowRun::execute` callback is also documented to receive each
complete step-start snapshot. A concurrent reader can therefore observe
`Running` with `attempts == 0`.

The later `Running -> Unstarted` path also violates the approved distinction.
The scheduler already published the start, so cancellation or deadline before
the task's first poll interrupts an active step; it does not retroactively make
the step never started. `workflow.rs:1457-1507` currently encodes the contrary
expectation. This is reachable through ordinary cancellation, total deadline,
or executor abort and is required TASK-004 behavior, not speculative hardening.

### Supervisor acknowledgement wording

The phrase “unacknowledged participant release” does refer to a forbidden and
nonexistent acknowledgement concept; it is not the name of the local condition
the function counts. `draining_graphs` counts only `Draining` entries with
`settlement_failure: Some(_)`. Those failures are produced by local lifecycle
cleanup or task-join failure. `AnalyticalGraphLifecycle::settle` releases
followers by dropping grant streams, while architecture says followers react
to stream close asynchronously and the leader does not wait.

That factual assessment does not make the unchanged base sentence a valid
cumulative-task finding. No runtime acknowledgement, polling, retry, probe,
setting, option, or readiness mechanism may be added in response to it.

### Qualified and local imports

Each cited import violation is materially within the cumulative candidate:

- `prompt_support::raw_body` replaces `raw_body_text` and changes the return
  contract to `serde_json::Value`; the same module already imports `Value`.
- `cancel_while_opening` and `later` are newly added interfaces in their
  production and test modules.
- `fixture_rows_ipc` gains the event-time argument, schema field, and column;
  the materially changed symbol's full interface is governed by the rule even
  though its qualified return spelling existed before the edit.
- `server_routes_keep_gateway_and_external_ownership` and its function-scoped
  `PermissionsExt as _` import are new. The allowed local trait-import exception
  is for a single generic function where module scope is inappropriate; this
  journey is not generic.

The correction is ordinary Rust import hygiene already mandated by repository
authority. It does not warrant a new enforcement mechanism.

## Prior-finding closure

`FIND-TASK-004-1` through `FIND-TASK-004-18` remain **CLOSED**. The new findings
do not reopen or renumber them.

| Prior IDs | Closure retained at | Status |
|---|---|---|
| `FIND-TASK-004-1`, `-2` | Reservation-time tracker ownership and tracked blocking preparation | **CLOSED** |
| `FIND-TASK-004-3`, `-4` | Original-deadline open cancellation and exact pod-loss settlement/recovery evidence | **CLOSED** |
| `FIND-TASK-004-5`, `-8`, `-14` | Authenticated second-tenant boundaries, pinned/captured authority proof, and aligned design/security authority | **CLOSED** |
| `FIND-TASK-004-6`, `-7`, `-10` | Built-in declaration rustdoc, exact output schemas, and tool bound/denial/terminal-negative journeys | **CLOSED** |
| `FIND-TASK-004-9`, `-11`, `-12`, `-13` | Idempotency, gateway, lifecycle, graph/snapshot, and sibling-service journeys | **CLOSED** |
| `FIND-TASK-004-15` | All three canonical Prompt snippets use the adjacent `provider`/`body` envelope | **CLOSED** |
| `FIND-TASK-004-16` | Every location cited by R2 now uses its module-imported bare type, with `TokioInstant` only for the real collision | **CLOSED** |
| `FIND-TASK-004-17` | Active TASK-004 and R1 remediation identify Revision 13 while immutable R1 inputs retain historical Revision 12 | **CLOSED** |
| `FIND-TASK-004-18` | The cited `AnalyticalGraphLifecycle` owner and transport docs state grant-stream-close release without a leader acknowledgement | **CLOSED** |

The rejected supervisor-doc proposal is not a reopening of
`FIND-TASK-004-18`: that finding named a different, candidate-added owner
description and its decision-complete correction is present.

## Final deduplicated finding ledger

### FIND-TASK-004-19 — CONFIRMED — INCORRECT: a published running step has zero attempts and can regress to unstarted

- **Discovery sources:** `DOMAIN-CONCURRENCY-R3-001`, `FOLLOWUP-R3-001`.
- **Violated obligation:** Revision 13 snapshot invariants at
  `spec.md:977-999` require running and cancelled active steps to have at least
  one begun attempt, interrupted active steps to retain start/end timestamps,
  and unstarted steps to have neither timestamp. Its normative execution flow
  at `spec.md:1661-1687` forbids observable status regression and requires
  cancellation to distinguish interrupted active work from never-started work.
- **Exact location:**
  `crates/skald/skald-workflow/src/workflow.rs:273-289,307-369,451-464`;
  `crates/skald/skald-workflow/src/run.rs:125-130,177-183,215-232`;
  existing contradicted proof at
  `crates/skald/skald-workflow/src/workflow.rs:788-820,1457-1507`.
- **Producer-to-consumer evidence:** the executor initializes each atomic
  attempt count to zero, then `drive` calls `step_started` and publishes the
  complete snapshot before spawning the step task. `step_started` changes only
  status and start time. The task increments the counter on its first poll,
  after an initial cancellation/deadline check. A pre-poll abort therefore
  leaves zero in the producer. `settle` does not mark zero-attempt aborted work
  cancelled, and `finish` rewrites the still-running ledger entry to unstarted.
  The callback is consumed by server `AcceptedRun::observe`, and the public
  prepared-run callback is the sibling consumer.
- **Observable consequence:** a server GET or direct transition observer can
  receive a structurally impossible running step with zero attempts. An
  immediate cancel, deadline, or abort can then produce a terminal snapshot
  claiming the already-published step never started and erase its start time.
- **Decision-complete smallest correction:** at the existing scheduling/ledger
  transition, establish attempt one in both the step snapshot and the shared
  counter before publishing `Running`. Make the existing `StepTask` consume
  that reserved first attempt rather than incrementing it to two; only retries
  advance the counter. Then every abort of a published-running task follows the
  existing cancelled-active settlement path and retains timestamps. Preserve
  the current scheduler, `JoinSet`, ledger, callback, cancellation tree, and
  terminal owner. Add no spawn handshake, acknowledgement channel, lifecycle
  service, setting, or option.
- **Focused closure proof:** extend the existing
  `prepared_run_keeps_its_id` transition assertions to require attempts of at
  least one for every observed running step. Correct the existing pre-poll
  branch in `bounded_attempt_lifecycle` to prove a published-running step
  terminalizes as `Cancelled` with attempts one and retained start/end times.
  Existing retry-accounting tests must continue to prove success on attempt one
  and exhaustion at `max_retries + 1`; no new harness or repository check is
  warranted.

### FIND-TASK-004-20 — CONFIRMED — VIOLATION: materially changed Rust interfaces and one dependency use bypass module imports

- **Discovery sources:** `STD-004-R3-001`, `FOLLOWUP-R3-003`.
- **Violated obligation:** `architecture/agent-rules.md` requires interface
  types to be imported at module scope and referenced by bare name, and requires
  `use` statements at the module's top. Its local `TraitName as _` exception is
  narrow and does not cover an ordinary non-generic journey function.
- **Exact location:**
  `crates/wyrd-spec/src/card/prompt/mod.rs:264`;
  `crates/wyrd/wyrd-server/src/oracle/lifecycle_controls.rs:310-315,393-396`;
  `crates/wyrd/wyrd-testing/tests/bifrost/oracle/peer_cluster.rs:1795-1811`;
  and
  `crates/wyrd/wyrd-server/tests/pg_workflow_runs.rs:2224-2233`.
- **Producer-to-consumer evidence:** `raw_body` is called by Prompt codec tests;
  `cancel_while_opening` is called by `RunningQueryControls::open_cancellable`
  and its focused tests; `fixture_rows_ipc` is called by the real Scribe ingest
  fixture; and the Unix permission trait is used by the required gateway and
  external-ownership journey. Each owning module has an existing top-level
  dependency block, so no new abstraction or file is necessary.
- **Observable consequence:** the cumulative candidate violates a mandatory
  repository source-shape rule and leaves the affected modules' dependency
  surface inconsistent after the R2 cleanup. Recorded formatting and lint
  results do not enforce or waive this rule.
- **Decision-complete smallest correction:** import and use bare `Value`,
  `Instant`, and `Bytes` in their existing owning module import blocks; import
  `PermissionsExt as _` at module scope under `#[cfg(unix)]`. Use an alias only
  if an actual same-module collision requires one. Preserve all behavior. Add
  no scanner, lint, allow attribute, repository check, configuration, setting,
  or option.
- **Focused closure proof:** direct source inspection of the four corrected
  modules, followed by the repository's existing format/lint and already-owned
  affected test lanes during implementation. This mechanical correction needs
  no new test or enforcement mechanism.

## Validated ledger summary

| Stable ID | Status | Classification |
|---|---|---|
| `FIND-TASK-004-1` through `FIND-TASK-004-18` | **CLOSED** | Prior remediation findings |
| `FIND-TASK-004-19` | **CONFIRMED** | INCORRECT |
| `FIND-TASK-004-20` | **CONFIRMED** | VIOLATION |

No retained correction requires a new product, public API, architecture,
security, compatibility, cross-service, concurrency-semantics,
resource-ownership, or persistent-data decision. Both are bounded corrections
inside existing owners under approved Revision 13. No
`SPEC_REVISION_REQUIRED` condition is present.

## Verification limits and blockers

- No build, test, Cargo, or mise command was run, per the strict review
  instruction.
- Recorded evidence was treated as a claim and checked against current source
  and assertions. It credibly closes the prior findings, but the current
  transition test omits the running-attempt invariant, the pre-poll-abort test
  explicitly asserts the contradicted outcome, and formatting/lint evidence
  does not waive the import rule.
- No required report, authority, diff, source, caller trace, or prior ledger was
  missing. The focused follow-up conflicts are resolved from source. There is
  no validation blocker.
